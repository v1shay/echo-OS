use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::runtime::Handle;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use crate::automation::{AutomationBackend, RiskLevel, ToolCallRequest, ToolCallResult};
use crate::config::AppConfig;
use crate::llm::{HeuristicPlanningProvider, OpenAiCompatibleProvider, OllamaPlanningProvider, PlanningProvider, ProviderStack};
use crate::sms::{start_approval_webhook, NoopSmsService, SmsApprovalAction, SmsService, TwilioSmsService};
use crate::speech::{MacOsSayTextToSpeech, SpeechToText, TextToSpeech, WhisperCommandSpeechToText};

#[derive(Debug, Clone)]
pub struct TaskContext {
    pub id: String,
    pub user_request: String,
    pub summary: String,
    pub actions: Vec<ToolCallRequest>,
}

#[derive(Debug)]
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

struct AgentRuntime {
    provider: Arc<dyn PlanningProvider>,
    automation: Arc<dyn AutomationBackend>,
    stt: Arc<dyn SpeechToText>,
    tts: Arc<dyn TextToSpeech>,
    sms: Arc<dyn SmsService>,
    config: AppConfig,
    events: UnboundedSender<AgentEvent>,
    pending: Option<TaskContext>,
    speech_muted: bool,
}

pub fn start(
    handle: &Handle,
    config: AppConfig,
    automation: Arc<dyn AutomationBackend>,
) -> StartedAgent {
    let (command_tx, command_rx) = unbounded_channel();
    let (event_tx, event_rx) = unbounded_channel();
    let (sms_tx, sms_rx) = unbounded_channel();

    let provider = build_provider_stack(&config);
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
        provider,
        automation,
        stt,
        tts,
        sms,
        config,
        events: event_tx,
        pending: None,
        speech_muted: false,
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
                self.emit(AgentEvent::Status("Listening for voice input...".to_string()));
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
                        self.emit(AgentEvent::Error(format!("Voice capture failed: {}", error)));
                    }
                }
            }
            AgentCommand::ApprovePending(source) => {
                let Some(task) = self.pending.take() else {
                    self.emit(AgentEvent::Status("No pending approval".to_string()));
                    return;
                };
                self.emit(AgentEvent::ApprovalResolved(format!(
                    "Approved from {:?}",
                    source
                )));
                if let Err(error) = self.execute_task(task).await {
                    self.emit(AgentEvent::Error(error.to_string()));
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
        self.emit(AgentEvent::Status("Planning task...".to_string()));
        let decision = self
            .provider
            .plan_next_step(&input, self.automation.available_tools())
            .await?;

        if !decision.assistant_message.trim().is_empty() {
            self.emit(AgentEvent::AssistantMessage(decision.assistant_message.clone()));
            self.say(&decision.assistant_message).await;
        }

        if decision.actions.is_empty() {
            self.emit(AgentEvent::Completed(
                "Conversation handled without desktop actions".to_string(),
            ));
            return Ok(());
        }

        let task = TaskContext {
            id: next_task_id(),
            user_request: input,
            summary: decision
                .summary
                .clone()
                .unwrap_or_else(|| "Executing desktop task".to_string()),
            actions: decision.actions,
        };

        self.emit(AgentEvent::TaskUpdated(task.summary.clone()));
        self.notify_sms(&format!("Jarvis started: {}", task.summary)).await;

        if task.actions.iter().any(|action| {
            let risk = self.automation.classify_risk(action);
            action.requires_confirmation || matches!(risk, RiskLevel::High | RiskLevel::Critical)
        }) {
            self.pending = Some(task.clone());
            let approval_message = format!(
                "Approval required for {}. Reply YES/NO by SMS or approve in the desktop app.",
                task.summary
            );
            self.emit(AgentEvent::ApprovalRequired(approval_message.clone()));
            self.notify_sms(&approval_message).await;
            self.say("I need approval before running the risky steps.").await;
        } else {
            self.execute_task(task).await?;
        }

        Ok(())
    }

    async fn execute_task(&mut self, task: TaskContext) -> Result<()> {
        self.emit(AgentEvent::Status(format!("Executing {}", task.summary)));
        for action in task.actions {
            let risk = self.automation.classify_risk(&action);
            self.emit(AgentEvent::ToolLog(format!(
                "Running tool {} ({:?})",
                action.name, risk
            )));
            let result = self.automation.call_tool(action.clone()).await.with_context(|| {
                format!("tool {} failed while executing task {}", action.name, task.id)
            })?;
            self.handle_tool_result(&result).await;
        }
        let completion = format!("Task completed: {}", task.summary);
        self.emit(AgentEvent::Completed(completion.clone()));
        self.notify_sms(&completion).await;
        self.say(&completion).await;
        Ok(())
    }

    async fn handle_tool_result(&self, result: &ToolCallResult) {
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

fn build_provider_stack(config: &AppConfig) -> Arc<dyn PlanningProvider> {
    let primary = OllamaPlanningProvider::new(
        config.provider.ollama_base_url.clone(),
        config.provider.ollama_model.clone(),
    );
    let heuristic = Arc::new(HeuristicPlanningProvider::default()) as Arc<dyn PlanningProvider>;
    let fallback = match (
        config.provider.fallback_base_url.clone(),
        config.provider.fallback_model.clone(),
        config.provider.fallback_api_key.clone(),
    ) {
        (Some(base_url), Some(model), Some(api_key)) => {
            Arc::new(OpenAiCompatibleProvider::new(base_url, model, api_key))
                as Arc<dyn PlanningProvider>
        }
        _ => heuristic,
    };

    Arc::new(ProviderStack::new(
        Arc::new(primary),
        fallback,
        Arc::new(HeuristicPlanningProvider::default()),
    ))
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

fn next_task_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("task-{}", millis)
}
