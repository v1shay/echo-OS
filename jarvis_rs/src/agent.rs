use anyhow::{bail, Context, Result};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use tokio::runtime::Handle;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use crate::agent_core::{
    evidence_from_observation, verify_plan_completion, ActionSpec, RecoveryDecision, RecoveryKind,
    TaskJournalEntry,
};
use crate::automation::{
    AppTarget, AutomationBackend, BrowserTarget, RiskLevel, ToolCallRequest, ToolCallResult,
};
use crate::config::AppConfig;
use crate::llm::{
    CompletionStatus, HeuristicPlannerProvider, HeuristicWorkerProvider, HostedPlannerProvider,
    HostedWorkerProvider, LocalLlamaPlannerProvider, LocalLlamaWorkerProvider, Observation,
    PlannerProvider, PlannerStack, TaskPlan, TaskState, WorkerAction, WorkerProvider, WorkerStack,
};
use crate::sms::{
    start_approval_webhook, NoopSmsService, SmsApprovalAction, SmsService, TwilioSmsService,
};
use crate::speech::{MacOsSayTextToSpeech, SpeechToText, TextToSpeech, WhisperCommandSpeechToText};

#[derive(Debug, Clone)]
pub enum AgentCommand {
    SubmitText(String),
    StartListening,
    ApprovePending(ApprovalSource),
    RejectPending(ApprovalSource),
    CancelActive,
    SetSpeechMuted(bool),
}

#[derive(Debug, Clone, Copy)]
pub enum ApprovalSource {
    Desktop,
    Sms,
}

#[derive(Debug, Clone)]
pub enum AgentEvent {
    Status(String),
    Transcript(String),
    AssistantMessage(String),
    GoalUpdated(String),
    StepUpdated(String),
    ObservationUpdated(String),
    ToolPlanned(String),
    ToolLog(String),
    TaskUpdated(String),
    ApprovalRequired(String),
    ApprovalResolved(String),
    Completed(String),
    Error(String),
    Listening(bool),
}

pub struct StartedAgent {
    pub commands: UnboundedSender<AgentCommand>,
    pub events: UnboundedReceiver<AgentEvent>,
}

#[derive(Debug)]
struct PendingApproval {
    state: TaskState,
    request: ToolCallRequest,
}

struct ModelRuntime {
    children: Vec<Child>,
    diagnostics: Vec<String>,
}

impl ModelRuntime {
    fn boot(config: &AppConfig) -> Self {
        let mut runtime = Self {
            children: Vec::new(),
            diagnostics: Vec::new(),
        };

        runtime
            .diagnostics
            .push(format!("Primary browser: {}", config.primary_browser));

        let binary = &config.provider.llama_server_binary;
        if !binary_exists(binary) {
            runtime.diagnostics.push(format!(
                "Local model server '{}' is not installed; falling back to heuristic planner/worker",
                binary
            ));
            return runtime;
        }

        if endpoint_available(&config.provider.planner_endpoint) {
            runtime.diagnostics.push(format!(
                "Planner endpoint already available at {}",
                config.provider.planner_endpoint
            ));
        } else {
            let planner_started = runtime.try_spawn_model(
                "planner",
                binary,
                &config.provider.planner_endpoint,
                config.provider.planner_model_path.as_ref(),
                config.provider.model_context_size,
            );
            if let Err(error) = planner_started {
                runtime
                    .diagnostics
                    .push(format!("Planner model unavailable: {}", error));
            }
        }

        if endpoint_available(&config.provider.worker_endpoint) {
            runtime.diagnostics.push(format!(
                "Worker endpoint already available at {}",
                config.provider.worker_endpoint
            ));
        } else {
            let worker_started = runtime.try_spawn_model(
                "worker",
                binary,
                &config.provider.worker_endpoint,
                config.provider.worker_model_path.as_ref(),
                config.provider.model_context_size,
            );
            if let Err(error) = worker_started {
                runtime
                    .diagnostics
                    .push(format!("Worker model unavailable: {}", error));
            }
        }

        runtime
    }

    fn try_spawn_model(
        &mut self,
        role: &str,
        binary: &str,
        endpoint: &str,
        model_path: Option<&std::path::PathBuf>,
        context_size: usize,
    ) -> Result<()> {
        let Some(model_path) = model_path else {
            bail!("no GGUF model configured");
        };
        if !model_path.exists() {
            bail!("model file {} does not exist", model_path.display());
        }

        let port = parse_port(endpoint)?;
        let child = Command::new(binary)
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .arg("-m")
            .arg(model_path)
            .arg("-c")
            .arg(context_size.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("failed to start {} model server", role))?;
        self.children.push(child);
        self.diagnostics.push(format!(
            "Started {} model server on {} using {}",
            role,
            endpoint,
            model_path.display()
        ));
        Ok(())
    }
}

struct AgentRuntime {
    planner: Arc<dyn PlannerProvider>,
    worker: Arc<dyn WorkerProvider>,
    automation: Arc<dyn AutomationBackend>,
    stt: Arc<dyn SpeechToText>,
    tts: Arc<dyn TextToSpeech>,
    sms: Arc<dyn SmsService>,
    config: AppConfig,
    events: UnboundedSender<AgentEvent>,
    pending: Option<PendingApproval>,
    speech_muted: bool,
    _model_runtime: ModelRuntime,
}

pub fn start(
    handle: &Handle,
    config: AppConfig,
    automation: Arc<dyn AutomationBackend>,
) -> StartedAgent {
    let (command_tx, command_rx) = unbounded_channel();
    let (event_tx, event_rx) = unbounded_channel();
    let (sms_tx, sms_rx) = unbounded_channel();

    let model_runtime = ModelRuntime::boot(&config);
    let planner = build_planner_stack(&config);
    let worker = build_worker_stack(&config);
    let stt: Arc<dyn SpeechToText> = Arc::new(WhisperCommandSpeechToText::new(
        config.whisper_model_path.clone(),
    ));
    let tts: Arc<dyn TextToSpeech> = build_tts(&config);
    let sms: Arc<dyn SmsService> = build_sms_service(&config);

    if sms.is_configured() {
        let bind_addr = config.sms.webhook_bind.clone();
        let event_tx_clone = event_tx.clone();
        handle.spawn(async move {
            if let Err(error) = start_approval_webhook(bind_addr, sms_tx).await {
                let _ = event_tx_clone.send(AgentEvent::Error(format!(
                    "Failed to start SMS webhook: {}",
                    error
                )));
            }
        });
    }

    let runtime = AgentRuntime {
        planner,
        worker,
        automation,
        stt,
        tts,
        sms,
        config,
        events: event_tx,
        pending: None,
        speech_muted: false,
        _model_runtime: model_runtime,
    };

    handle.spawn(async move {
        runtime.run(command_rx, sms_rx).await;
    });

    StartedAgent {
        commands: command_tx,
        events: event_rx,
    }
}

impl AgentRuntime {
    async fn run(
        mut self,
        mut commands: UnboundedReceiver<AgentCommand>,
        mut sms_actions: UnboundedReceiver<SmsApprovalAction>,
    ) {
        self.emit(AgentEvent::Status("Workspace booted".to_string()));
        self.emit(AgentEvent::Status(self.config.config_summary()));
        for line in &self._model_runtime.diagnostics {
            self.emit(AgentEvent::Status(line.clone()));
        }
        let capabilities = self.automation.capabilities();
        self.emit(AgentEvent::Status(format!(
            "Automation: browser={}, chrome_installed={}, browser_ready={}, applescript={}",
            capabilities.primary_browser,
            capabilities.chrome_installed,
            capabilities.browser_automation_ready,
            capabilities.applescript_available
        )));
        for item in &capabilities.setup_items {
            self.emit(AgentEvent::Status(format!("Setup: {}", item)));
        }

        while !commands.is_closed() || !sms_actions.is_closed() {
            tokio::select! {
                Some(command) = commands.recv() => self.handle_command(command).await,
                Some(action) = sms_actions.recv() => {
                    match action {
                        SmsApprovalAction::Approve => self.handle_command(AgentCommand::ApprovePending(ApprovalSource::Sms)).await,
                        SmsApprovalAction::Reject => self.handle_command(AgentCommand::RejectPending(ApprovalSource::Sms)).await,
                    }
                }
                else => break,
            }
        }
    }

    async fn handle_command(&mut self, command: AgentCommand) {
        match command {
            AgentCommand::SubmitText(input) => {
                if input.trim().is_empty() {
                    return;
                }
                if let Err(error) = self.handle_user_request(input).await {
                    self.emit(AgentEvent::Error(error.to_string()));
                }
            }
            AgentCommand::StartListening => {
                self.emit(AgentEvent::Listening(true));
                self.emit(AgentEvent::Status(
                    "Listening for voice input...".to_string(),
                ));
                match self
                    .stt
                    .record_and_transcribe(&self.config.recording_path, self.config.capture_seconds)
                    .await
                {
                    Ok(transcript) => {
                        self.emit(AgentEvent::Listening(false));
                        self.emit(AgentEvent::Transcript(transcript.clone()));
                        if let Err(error) = self.handle_user_request(transcript).await {
                            self.emit(AgentEvent::Error(error.to_string()));
                        }
                    }
                    Err(error) => {
                        self.emit(AgentEvent::Listening(false));
                        self.emit(AgentEvent::Error(format!(
                            "Voice capture failed: {}",
                            error
                        )));
                    }
                }
            }
            AgentCommand::ApprovePending(source) => {
                let Some(pending) = self.pending.take() else {
                    self.emit(AgentEvent::Status("No pending approval".to_string()));
                    return;
                };
                self.emit(AgentEvent::ApprovalResolved(format!(
                    "Approved from {:?}",
                    source
                )));
                match self.execute_tool(&pending.request, pending.state).await {
                    Ok(next_state) => {
                        if let Err(error) = self.execute_task_loop(next_state).await {
                            self.emit(AgentEvent::Error(error.to_string()));
                        }
                    }
                    Err(error) => self.emit(AgentEvent::Error(error.to_string())),
                }
            }
            AgentCommand::RejectPending(source) => {
                self.pending = None;
                self.emit(AgentEvent::ApprovalResolved(format!(
                    "Rejected from {:?}",
                    source
                )));
                self.emit(AgentEvent::Status("Pending task cancelled".to_string()));
            }
            AgentCommand::CancelActive => {
                self.pending = None;
                self.emit(AgentEvent::Status(
                    "Emergency cancel requested; queued work has been cleared".to_string(),
                ));
            }
            AgentCommand::SetSpeechMuted(value) => {
                self.speech_muted = value;
                self.emit(AgentEvent::Status(if value {
                    "Speech output muted".to_string()
                } else {
                    "Speech output unmuted".to_string()
                }));
            }
        }
    }

    async fn handle_user_request(&mut self, input: String) -> Result<()> {
        self.emit(AgentEvent::Status(format!(
            "Planning with {}",
            self.planner.provider_name()
        )));
        let capabilities = self.automation.capabilities();
        let plan = self.planner.create_plan(&input, &capabilities).await?;
        self.begin_task(input, plan).await
    }

    async fn begin_task(&mut self, input: String, plan: TaskPlan) -> Result<()> {
        self.emit(AgentEvent::GoalUpdated(plan.goal.clone()));
        self.emit(AgentEvent::TaskUpdated(plan.summary.clone()));

        if !plan.assistant_message.trim().is_empty() {
            self.emit(AgentEvent::AssistantMessage(plan.assistant_message.clone()));
            self.say(&plan.assistant_message).await;
        }

        if plan.steps.is_empty() {
            let message = "Conversation handled without desktop actions".to_string();
            self.emit(AgentEvent::Completed(message.clone()));
            return Ok(());
        }

        self.notify_sms(&format!("Jarvis started: {}", plan.summary))
            .await;

        let mut state = TaskState::new(input, plan);
        state.journal.push(TaskJournalEntry {
            phase: "plan".to_string(),
            message: format!("Started task '{}'", state.plan.summary),
            action: None,
            evidence: None,
            verification: None,
            recovery: None,
        });
        self.execute_task_loop(state).await
    }

    async fn execute_task_loop(&mut self, mut state: TaskState) -> Result<()> {
        for _ in 0..self.config.max_worker_iterations {
            state.mark_in_progress();
            if let Some(step) = state.current_step() {
                self.emit(AgentEvent::StepUpdated(format!(
                    "{}: {}",
                    step.title, step.instruction
                )));
            }

            self.emit(AgentEvent::Status(format!(
                "Working with {}",
                self.worker.provider_name()
            )));
            let capabilities = self.automation.capabilities();
            let decision = self.worker.next_action(&state, &capabilities).await?;

            if let Some(message) = decision.assistant_message.as_deref() {
                self.emit(AgentEvent::AssistantMessage(message.to_string()));
            }

            match decision.action {
                WorkerAction::Tool { request } => {
                    let risk = self.automation.classify_risk(&request);
                    self.emit(AgentEvent::ToolPlanned(format!(
                        "Next tool: {} ({:?})",
                        request.name, risk
                    )));
                    state.journal.push(TaskJournalEntry {
                        phase: "action".to_string(),
                        message: format!("Scheduling tool {}", request.name),
                        action: Some(ActionSpec {
                            name: request.name.clone(),
                            expected_outcome: request.expected_outcome.clone(),
                            target_identity: request.target_identity.clone(),
                        }),
                        evidence: None,
                        verification: None,
                        recovery: None,
                    });
                    if requires_user_approval(&request, risk) {
                        let message = format!(
                            "Approval required before {}. Approve in the app or reply YES by SMS.",
                            request.name
                        );
                        state.awaiting_approval = true;
                        state.completion_status = CompletionStatus::AwaitingApproval;
                        state.block_current_step();
                        self.pending = Some(PendingApproval { state, request });
                        self.emit(AgentEvent::ApprovalRequired(message.clone()));
                        self.notify_sms(&message).await;
                        self.say("I need approval before the next irreversible step.")
                            .await;
                        return Ok(());
                    }

                    state = self.execute_tool(&request, state).await?;
                }
                WorkerAction::AdvanceStep { note } => {
                    if !step_proved(&state) {
                        self.emit(AgentEvent::Status(
                            "Worker attempted to advance without proof; keeping the current step active"
                                .to_string(),
                        ));
                        continue;
                    }
                    self.emit(AgentEvent::ObservationUpdated(note));
                    state.advance_step();
                    if state.current_step().is_none() {
                        let message = format!("Task completed: {}", state.plan.summary);
                        match self.attempt_completion(state, message).await? {
                            Some(next_state) => {
                                state = next_state;
                                continue;
                            }
                            None => return Ok(()),
                        }
                    }
                }
                WorkerAction::Replan { reason } => {
                    self.emit(AgentEvent::Status(format!("Replanning: {}", reason)));
                    let capabilities = self.automation.capabilities();
                    let replanned = self
                        .planner
                        .create_plan(
                            &format!(
                                "Original request: {}\nReplan reason: {}\nLast observation: {:?}",
                                state.user_request, reason, state.last_observation
                            ),
                            &capabilities,
                        )
                        .await?;
                    self.emit(AgentEvent::TaskUpdated(replanned.summary.clone()));
                    state.journal.push(TaskJournalEntry {
                        phase: "recovery".to_string(),
                        message: format!("Replanning because {}", reason),
                        action: None,
                        evidence: None,
                        verification: None,
                        recovery: Some(RecoveryDecision {
                            kind: RecoveryKind::Replan,
                            reason,
                            next_hint: Some(replanned.summary.clone()),
                        }),
                    });
                    state.reset_for_replan(replanned);
                }
                WorkerAction::Complete { message } => {
                    if state.current_step().is_some() && !step_proved(&state) {
                        self.emit(AgentEvent::Status(
                            "Worker attempted to complete the task without proof; waiting for a verified outcome"
                                .to_string(),
                        ));
                        continue;
                    }
                    match self.attempt_completion(state, message).await? {
                        Some(next_state) => state = next_state,
                        None => return Ok(()),
                    }
                }
            }
        }

        bail!(
            "worker loop exceeded {} iterations for task {}",
            self.config.max_worker_iterations,
            state.plan.summary
        )
    }

    async fn execute_tool(
        &mut self,
        request: &ToolCallRequest,
        mut state: TaskState,
    ) -> Result<TaskState> {
        self.emit(AgentEvent::ToolLog(format!(
            "Running tool {}",
            request.name
        )));
        state.last_expected_outcome = request.expected_outcome.clone();
        let result = self
            .automation
            .call_tool(request.clone())
            .await
            .with_context(|| {
                format!(
                    "tool {} failed while executing '{}'",
                    request.name, state.plan.summary
                )
            })?;
        self.handle_tool_result(&result, &mut state).await;
        Ok(state)
    }

    async fn handle_tool_result(&self, result: &ToolCallResult, state: &mut TaskState) {
        self.emit(AgentEvent::ToolLog(result.summary.clone()));
        if let Some(output) = &result.output {
            self.emit(AgentEvent::Status(output.clone()));
        }
        if let Some(artifact) = &result.artifact_path {
            self.emit(AgentEvent::Status(format!(
                "Artifact ready: {}",
                artifact.display()
            )));
        }

        state.last_tool_name = Some(result.name.clone());
        state.last_observation = Some(Observation {
            source: result.name.clone(),
            summary: result.summary.clone(),
            details: result.output.clone(),
            target_identity: result.target_identity.clone(),
            success: result.success,
            retryable: result.retryable,
            proof_passed: result.proof_passed,
            observed_outcome: result.observed_outcome.clone(),
        });
        if let Some(observation) = state.last_observation.as_ref() {
            state.journal.push(TaskJournalEntry {
                phase: "observe".to_string(),
                message: result.summary.clone(),
                action: None,
                evidence: Some(evidence_from_observation(observation)),
                verification: None,
                recovery: None,
            });
        }
        self.emit(AgentEvent::ObservationUpdated(result.summary.clone()));

        if let Ok(app_target) = serde_json::from_value::<AppTarget>(result.observation.clone()) {
            state.active_app = Some(app_target);
        }
        if let Ok(browser_target) =
            serde_json::from_value::<BrowserTarget>(result.observation.clone())
        {
            state.active_browser = Some(browser_target);
        }
    }

    async fn attempt_completion(
        &mut self,
        mut state: TaskState,
        message: String,
    ) -> Result<Option<TaskState>> {
        let verification = verify_plan_completion(&state.plan, state.last_observation.as_ref());
        state.final_verification = Some(verification.clone());
        state.journal.final_verification = Some(verification.clone());
        state.journal.push(TaskJournalEntry {
            phase: "verify".to_string(),
            message: verification.summary.clone(),
            action: None,
            evidence: state
                .last_observation
                .as_ref()
                .map(evidence_from_observation),
            verification: Some(verification.clone()),
            recovery: None,
        });

        if verification.satisfied {
            self.complete_task(&message).await;
            return Ok(None);
        }

        self.emit(AgentEvent::Status(format!(
            "Final verifier requested recovery: {}",
            verification.summary
        )));
        let capabilities = self.automation.capabilities();
        let replanned = self
            .planner
            .create_plan(
                &format!(
                    "Original request: {}\nFinal verifier summary: {}\nMissing criteria: {:?}\nLast observation: {:?}",
                    state.user_request,
                    verification.summary,
                    verification.missing_criteria,
                    state.last_observation
                ),
                &capabilities,
            )
            .await?;
        state.journal.push(TaskJournalEntry {
            phase: "recovery".to_string(),
            message: "Final verifier triggered a recovery replan".to_string(),
            action: None,
            evidence: None,
            verification: Some(verification),
            recovery: Some(RecoveryDecision {
                kind: RecoveryKind::Replan,
                reason: "Final verifier could not confirm completion".to_string(),
                next_hint: Some(replanned.summary.clone()),
            }),
        });
        self.emit(AgentEvent::TaskUpdated(replanned.summary.clone()));
        state.reset_for_replan(replanned);
        Ok(Some(state))
    }

    async fn complete_task(&self, message: &str) {
        self.emit(AgentEvent::Completed(message.to_string()));
        self.notify_sms(message).await;
        self.say(message).await;
    }

    async fn notify_sms(&self, message: &str) {
        if let Err(error) = self.sms.send_message(message).await {
            self.emit(AgentEvent::Error(format!("SMS delivery failed: {}", error)));
        }
    }

    async fn say(&self, message: &str) {
        if self.speech_muted {
            return;
        }
        let text = message.to_string();
        let tts = Arc::clone(&self.tts);
        tokio::task::spawn_blocking(move || {
            let _ = tts.speak(&text);
        });
    }

    fn emit(&self, event: AgentEvent) {
        let _ = self.events.send(event);
    }
}

fn build_planner_stack(config: &AppConfig) -> Arc<dyn PlannerProvider> {
    let mut providers = Vec::<Arc<dyn PlannerProvider>>::new();
    if let (Some(base_url), Some(model)) = (
        config.provider.fallback_base_url.clone(),
        config.provider.fallback_model.clone(),
    ) {
        providers.push(Arc::new(HostedPlannerProvider::new(
            base_url,
            model,
            config.provider.fallback_api_key.clone(),
        )));
    }
    providers.push(Arc::new(LocalLlamaPlannerProvider::new(
        config.provider.planner_endpoint.clone(),
        config.provider.planner_model.clone(),
        config.provider.fallback_api_key.clone(),
    )));
    providers.push(Arc::new(HeuristicPlannerProvider));
    Arc::new(PlannerStack::new(providers))
}

fn build_worker_stack(config: &AppConfig) -> Arc<dyn WorkerProvider> {
    let mut providers = Vec::<Arc<dyn WorkerProvider>>::new();
    if let (Some(base_url), Some(model)) = (
        config.provider.fallback_base_url.clone(),
        config.provider.fallback_model.clone(),
    ) {
        providers.push(Arc::new(HostedWorkerProvider::new(
            base_url,
            model,
            config.provider.fallback_api_key.clone(),
        )));
    }
    providers.push(Arc::new(LocalLlamaWorkerProvider::new(
        config.provider.worker_endpoint.clone(),
        config.provider.worker_model.clone(),
        config.provider.fallback_api_key.clone(),
    )));
    providers.push(Arc::new(HeuristicWorkerProvider));
    Arc::new(WorkerStack::new(providers))
}

fn build_tts(config: &AppConfig) -> Arc<dyn TextToSpeech> {
    #[cfg(target_os = "macos")]
    {
        Arc::new(MacOsSayTextToSpeech::new(config.voice_name.clone()))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = config;
        Arc::new(crate::speech::NoopTextToSpeech)
    }
}

fn build_sms_service(config: &AppConfig) -> Arc<dyn SmsService> {
    let service = TwilioSmsService::new(config.sms.clone());
    if service.is_configured() {
        Arc::new(service)
    } else {
        Arc::new(NoopSmsService)
    }
}

fn binary_exists(binary: &str) -> bool {
    Command::new("which")
        .arg(binary)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn parse_port(endpoint: &str) -> Result<u16> {
    let after_scheme = endpoint
        .split("//")
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("invalid endpoint {}", endpoint))?;
    let host_and_path = after_scheme
        .split('/')
        .next()
        .ok_or_else(|| anyhow::anyhow!("invalid endpoint {}", endpoint))?;
    let port = host_and_path
        .split(':')
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("endpoint missing port {}", endpoint))?;
    port.parse::<u16>()
        .with_context(|| format!("invalid port in endpoint {}", endpoint))
}

fn endpoint_available(endpoint: &str) -> bool {
    let url = format!("{}/models", endpoint.trim_end_matches('/'));
    Command::new("curl")
        .arg("-s")
        .arg(url)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn requires_user_approval(request: &ToolCallRequest, risk: RiskLevel) -> bool {
    if request.requires_confirmation {
        return true;
    }

    matches!(request.name.as_str(), "filesystem_delete" | "shell_run")
        || matches!(risk, RiskLevel::Critical)
}

fn step_proved(state: &TaskState) -> bool {
    state
        .last_observation
        .as_ref()
        .map(|observation| observation.success && observation.proof_passed)
        .unwrap_or(false)
}
