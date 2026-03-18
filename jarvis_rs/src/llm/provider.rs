use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde_json::json;

use crate::automation::{AutomationCapabilities, RiskLevel, ToolCallRequest};

use super::prompt::{build_planner_prompt, build_worker_prompt};
use super::schema::{PlanStep, TaskPlan, TaskState, WorkerAction, WorkerDecision};

#[async_trait]
pub trait PlannerProvider: Send + Sync {
    async fn create_plan(
        &self,
        task_input: &str,
        capabilities: &AutomationCapabilities,
    ) -> Result<TaskPlan>;
    fn provider_name(&self) -> &'static str;
}

#[async_trait]
pub trait WorkerProvider: Send + Sync {
    async fn next_action(
        &self,
        state: &TaskState,
        capabilities: &AutomationCapabilities,
    ) -> Result<WorkerDecision>;
    fn provider_name(&self) -> &'static str;
}

#[derive(Clone)]
pub struct OpenAiCompatibleClient {
    base_url: String,
    model: String,
    api_key: Option<String>,
    http: Client,
}

impl OpenAiCompatibleClient {
    pub fn new(base_url: String, model: String, api_key: Option<String>) -> Self {
        Self {
            base_url,
            model,
            api_key,
            http: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| Client::new()),
        }
    }

    pub fn model_name(&self) -> &str {
        &self.model
    }

    async fn chat_completion(&self, system_prompt: &str, user_prompt: &str) -> Result<String> {
        let endpoint = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let mut request = self.http.post(endpoint).json(&json!({
            "model": self.model,
            "temperature": 0.1,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_prompt}
            ]
        }));
        if let Some(api_key) = &self.api_key {
            request = request.bearer_auth(api_key);
        }
        let response = request.send().await?.error_for_status()?;
        let body: serde_json::Value = response.json().await?;
        Ok(body["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_string())
    }

    async fn parse_json<T: DeserializeOwned>(
        &self,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Result<T> {
        let content = self.chat_completion(system_prompt, user_prompt).await?;
        serde_json::from_str(&content)
            .with_context(|| format!("model {} returned invalid JSON: {}", self.model, content))
    }

    pub async fn complete_text(&self, system_prompt: &str, user_prompt: &str) -> Result<String> {
        self.chat_completion(system_prompt, user_prompt).await
    }
}

#[derive(Clone)]
pub struct LocalLlamaPlannerProvider {
    client: OpenAiCompatibleClient,
}

impl LocalLlamaPlannerProvider {
    pub fn new(base_url: String, model: String, api_key: Option<String>) -> Self {
        Self {
            client: OpenAiCompatibleClient::new(base_url, model, api_key),
        }
    }
}

#[async_trait]
impl PlannerProvider for LocalLlamaPlannerProvider {
    async fn create_plan(
        &self,
        task_input: &str,
        capabilities: &AutomationCapabilities,
    ) -> Result<TaskPlan> {
        self.client
            .parse_json(
                "You are Jarvis Planner. Return strict JSON only.",
                &build_planner_prompt(task_input, capabilities),
            )
            .await
    }

    fn provider_name(&self) -> &'static str {
        "local-llama-planner"
    }
}

#[derive(Clone)]
pub struct HostedPlannerProvider {
    client: OpenAiCompatibleClient,
}

impl HostedPlannerProvider {
    pub fn new(base_url: String, model: String, api_key: Option<String>) -> Self {
        Self {
            client: OpenAiCompatibleClient::new(base_url, model, api_key),
        }
    }
}

#[async_trait]
impl PlannerProvider for HostedPlannerProvider {
    async fn create_plan(
        &self,
        task_input: &str,
        capabilities: &AutomationCapabilities,
    ) -> Result<TaskPlan> {
        self.client
            .parse_json(
                "You are Jarvis Planner. Return strict JSON only.",
                &build_planner_prompt(task_input, capabilities),
            )
            .await
    }

    fn provider_name(&self) -> &'static str {
        "hosted-openai-planner"
    }
}

#[derive(Clone)]
pub struct LocalLlamaWorkerProvider {
    client: OpenAiCompatibleClient,
}

impl LocalLlamaWorkerProvider {
    pub fn new(base_url: String, model: String, api_key: Option<String>) -> Self {
        Self {
            client: OpenAiCompatibleClient::new(base_url, model, api_key),
        }
    }
}

#[async_trait]
impl WorkerProvider for LocalLlamaWorkerProvider {
    async fn next_action(
        &self,
        state: &TaskState,
        capabilities: &AutomationCapabilities,
    ) -> Result<WorkerDecision> {
        self.client
            .parse_json(
                "You are Jarvis Worker. Return strict JSON only.",
                &build_worker_prompt(state, capabilities),
            )
            .await
    }

    fn provider_name(&self) -> &'static str {
        "local-llama-worker"
    }
}

#[derive(Clone)]
pub struct HostedWorkerProvider {
    client: OpenAiCompatibleClient,
}

impl HostedWorkerProvider {
    pub fn new(base_url: String, model: String, api_key: Option<String>) -> Self {
        Self {
            client: OpenAiCompatibleClient::new(base_url, model, api_key),
        }
    }
}

#[async_trait]
impl WorkerProvider for HostedWorkerProvider {
    async fn next_action(
        &self,
        state: &TaskState,
        capabilities: &AutomationCapabilities,
    ) -> Result<WorkerDecision> {
        self.client
            .parse_json(
                "You are Jarvis Worker. Return strict JSON only.",
                &build_worker_prompt(state, capabilities),
            )
            .await
    }

    fn provider_name(&self) -> &'static str {
        "hosted-openai-worker"
    }
}

#[derive(Debug, Default)]
pub struct HeuristicPlannerProvider;

#[async_trait]
impl PlannerProvider for HeuristicPlannerProvider {
    async fn create_plan(
        &self,
        task_input: &str,
        capabilities: &AutomationCapabilities,
    ) -> Result<TaskPlan> {
        let lowered = task_input.to_ascii_lowercase();

        if lowered.contains("schoology")
            || lowered.contains("biology assignment")
            || lowered.contains("assignment")
        {
            return Ok(TaskPlan {
                assistant_message:
                    "I’m opening Chrome, getting into Schoology, and finding the assignment."
                        .to_string(),
                summary: "Open the latest Schoology assignment".to_string(),
                goal: task_input.trim().to_string(),
                success_criteria: vec![
                    "Chrome is open on the Schoology assignment page".to_string(),
                    "The latest biology assignment has been identified or opened".to_string(),
                    "The assignment content is summarized before any submission".to_string(),
                ],
                steps: vec![
                    step(
                        "activate_chrome",
                        "Open Chrome",
                        "Resolve and activate Google Chrome",
                        "Chrome is frontmost",
                    ),
                    step(
                        "open_schoology",
                        "Open Schoology",
                        "Open Schoology in Chrome",
                        "Schoology is visible in Chrome",
                    ),
                    step(
                        "open_assignments",
                        "Open assignments",
                        "Navigate to the assignments section",
                        "The assignments view is visible",
                    ),
                    step(
                        "open_biology",
                        "Open biology assignment",
                        "Open the latest biology-related assignment",
                        "The biology assignment is open",
                    ),
                    step(
                        "summarize_assignment",
                        "Summarize assignment",
                        "Read the assignment page and summarize the work",
                        "The assignment details are summarized",
                    ),
                ],
            });
        }

        if lowered.contains("email") || lowered.contains("mail ") {
            let recipient = extract_after_keyword(task_input, "to");
            let subject = extract_after_keyword(task_input, "about")
                .unwrap_or_else(|| "Jarvis follow-up".to_string());
            return Ok(TaskPlan {
                assistant_message: "I’m preparing a Mail draft.".to_string(),
                summary: "Compose an email draft".to_string(),
                goal: task_input.trim().to_string(),
                success_criteria: vec![
                    "Mail is frontmost".to_string(),
                    "A draft exists with the requested details".to_string(),
                    "Jarvis stops before sending".to_string(),
                ],
                steps: vec![
                    step(
                        "activate_mail",
                        "Open Mail",
                        "Activate Mail",
                        "Mail is frontmost",
                    ),
                    step(
                        "compose_mail",
                        "Compose mail",
                        &format!("Create a draft email to {:?}", recipient),
                        &format!("A visible mail draft exists with subject '{}'", subject),
                    ),
                ],
            });
        }

        if lowered.contains("message ") || lowered.starts_with("text ") {
            let recipient = extract_after_keyword(task_input, "message")
                .or_else(|| extract_after_keyword(task_input, "text"))
                .unwrap_or_else(|| "the requested contact".to_string());
            return Ok(TaskPlan {
                assistant_message: "I’m preparing a Messages draft.".to_string(),
                summary: "Compose a message draft".to_string(),
                goal: task_input.trim().to_string(),
                success_criteria: vec![
                    "Messages is frontmost".to_string(),
                    format!("A draft message exists for {}", recipient),
                    "Jarvis stops before sending".to_string(),
                ],
                steps: vec![
                    step(
                        "activate_messages",
                        "Open Messages",
                        "Activate Messages",
                        "Messages is frontmost",
                    ),
                    step(
                        "compose_message",
                        "Compose message",
                        "Create a draft message",
                        "The message draft is prepared",
                    ),
                ],
            });
        }

        if lowered.contains("open ") {
            let app_name = extract_after_keyword(task_input, "open")
                .unwrap_or_else(|| capabilities.primary_browser.clone());
            return Ok(TaskPlan {
                assistant_message: format!("I’m opening {}.", app_name),
                summary: format!("Open {}", app_name),
                goal: task_input.trim().to_string(),
                success_criteria: vec![format!("{} is frontmost", app_name)],
                steps: vec![
                    step(
                        "resolve_app",
                        "Resolve app",
                        &format!("Resolve the requested app '{}'", app_name),
                        "The requested app target is resolved",
                    ),
                    step(
                        "activate_app",
                        "Activate app",
                        &format!("Activate the requested app '{}'", app_name),
                        "The requested app is frontmost",
                    ),
                ],
            });
        }

        if lowered.contains("search") || lowered.contains("browser") || lowered.contains("web") {
            return Ok(TaskPlan {
                assistant_message: "I’m opening Chrome for that search.".to_string(),
                summary: "Open browser search".to_string(),
                goal: task_input.trim().to_string(),
                success_criteria: vec![
                    "Chrome is open".to_string(),
                    "The target page or search results are visible".to_string(),
                ],
                steps: vec![
                    step(
                        "activate_chrome",
                        "Open Chrome",
                        "Resolve and activate Chrome",
                        "Chrome is frontmost",
                    ),
                    step(
                        "browser_search",
                        "Run search",
                        "Open the relevant search or URL in Chrome",
                        "The relevant page is visible",
                    ),
                ],
            });
        }

        Err(anyhow!(
            "heuristic planner has no deterministic workflow for request '{}'",
            task_input.trim()
        ))
    }

    fn provider_name(&self) -> &'static str {
        "heuristic-planner"
    }
}

#[derive(Debug, Default)]
pub struct HeuristicWorkerProvider;

#[async_trait]
impl WorkerProvider for HeuristicWorkerProvider {
    async fn next_action(
        &self,
        state: &TaskState,
        _capabilities: &AutomationCapabilities,
    ) -> Result<WorkerDecision> {
        let Some(step) = state.current_step() else {
            return Ok(WorkerDecision {
                assistant_message: Some("The plan has no remaining steps.".to_string()),
                action: WorkerAction::Complete {
                    message: "Task completed".to_string(),
                },
            });
        };

        let last_details = state
            .last_observation
            .as_ref()
            .and_then(|observation| observation.details.as_ref())
            .map(|value| value.to_ascii_lowercase())
            .unwrap_or_default();

        let decision = match step.id.as_str() {
            "resolve_app" => {
                let app_name = extract_after_keyword(&state.user_request, "open")
                    .unwrap_or_else(|| "Google Chrome".to_string());
                if state.last_tool_name.as_deref() == Some("app_resolve") {
                    WorkerAction::AdvanceStep {
                        note: format!("Resolved {}", app_name),
                    }
                } else {
                    WorkerAction::Tool {
                        request: tool(
                            "app_resolve",
                            json!({ "app_name": app_name }),
                            RiskLevel::Low,
                        ),
                    }
                }
            }
            "activate_app" => {
                let app_name = extract_after_keyword(&state.user_request, "open")
                    .unwrap_or_else(|| "Google Chrome".to_string());
                if observation_proved(state) || frontmost_app_matches(state, &app_name) {
                    WorkerAction::Complete {
                        message: format!("Opened {}", app_name),
                    }
                } else if state.last_tool_name.as_deref() == Some("app_activate") {
                    WorkerAction::Replan {
                        reason: format!("{} did not become frontmost after activation", app_name),
                    }
                } else {
                    let mut request = tool(
                        "app_activate",
                        json!({ "app_name": app_name }),
                        RiskLevel::Low,
                    );
                    request.expected_outcome = Some("requested app is frontmost".to_string());
                    WorkerAction::Tool { request }
                }
            }
            "activate_chrome" => {
                if observation_proved(state) || frontmost_app_matches(state, "Google Chrome") {
                    WorkerAction::AdvanceStep {
                        note: "Chrome is active".to_string(),
                    }
                } else if state.last_tool_name.as_deref() == Some("app_activate") {
                    WorkerAction::Replan {
                        reason: "Chrome did not become frontmost after activation".to_string(),
                    }
                } else {
                    let mut request = tool(
                        "app_activate",
                        json!({ "app_name": "Google Chrome" }),
                        RiskLevel::Low,
                    );
                    request.expected_outcome = Some("Google Chrome is frontmost".to_string());
                    WorkerAction::Tool { request }
                }
            }
            "open_schoology" => {
                if state.last_tool_name.as_deref() == Some("browser_assert")
                    && observation_proved(state)
                {
                    WorkerAction::AdvanceStep {
                        note: "Schoology is open".to_string(),
                    }
                } else if matches!(
                    state.last_tool_name.as_deref(),
                    Some("browser_open") | Some("chrome_open_tab")
                ) {
                    let mut request = tool(
                        "browser_assert",
                        json!({ "url_contains": "schoology", "text_contains": "schoology" }),
                        RiskLevel::Low,
                    );
                    request.expected_outcome = Some("Schoology is visible in Chrome".to_string());
                    WorkerAction::Tool { request }
                } else {
                    let mut request = tool(
                        "browser_open",
                        json!({ "url": "https://app.schoology.com/home" }),
                        RiskLevel::Low,
                    );
                    request.expected_outcome = Some("Schoology home is loaded".to_string());
                    WorkerAction::Tool { request }
                }
            }
            "open_assignments" => {
                if state.last_tool_name.as_deref() == Some("browser_assert")
                    && observation_proved(state)
                {
                    WorkerAction::AdvanceStep {
                        note: "Opened the assignments view".to_string(),
                    }
                } else if matches!(
                    state.last_tool_name.as_deref(),
                    Some("browser_click") | Some("chrome_click")
                ) {
                    let mut request = tool(
                        "browser_assert",
                        json!({ "url_contains": "assign", "text_contains": "assignment" }),
                        RiskLevel::Low,
                    );
                    request.expected_outcome = Some("Assignments view is visible".to_string());
                    WorkerAction::Tool { request }
                } else {
                    let mut request = tool(
                        "browser_click",
                        json!({ "text": "Assignments" }),
                        RiskLevel::Medium,
                    );
                    request.expected_outcome = Some("Assignments tab opens".to_string());
                    WorkerAction::Tool { request }
                }
            }
            "open_biology" => {
                if state.last_tool_name.as_deref() == Some("browser_assert")
                    && observation_proved(state)
                {
                    WorkerAction::AdvanceStep {
                        note: "Opened the biology assignment".to_string(),
                    }
                } else if matches!(
                    state.last_tool_name.as_deref(),
                    Some("browser_click") | Some("chrome_click")
                ) {
                    let mut request = tool(
                        "browser_assert",
                        json!({ "text_contains": "biology" }),
                        RiskLevel::Low,
                    );
                    request.expected_outcome =
                        Some("The biology assignment page is open".to_string());
                    WorkerAction::Tool { request }
                } else {
                    let mut request = tool(
                        "browser_click",
                        json!({ "text": "biology" }),
                        RiskLevel::Medium,
                    );
                    request.expected_outcome =
                        Some("The latest biology assignment opens".to_string());
                    WorkerAction::Tool { request }
                }
            }
            "summarize_assignment" => {
                if state.last_tool_name.as_deref() == Some("browser_extract_text") {
                    let summary = summarize_text(&last_details);
                    WorkerAction::Complete {
                        message: if summary.is_empty() {
                            "I opened the assignment, but I need a stronger model to summarize the full page cleanly.".to_string()
                        } else {
                            format!("Assignment opened. Summary: {}", summary)
                        },
                    }
                } else {
                    let mut request = tool("browser_extract_text", json!({}), RiskLevel::Low);
                    request.expected_outcome = Some("Assignment text is extracted".to_string());
                    WorkerAction::Tool { request }
                }
            }
            "activate_mail" => {
                if observation_proved(state) || frontmost_app_matches(state, "Mail") {
                    WorkerAction::AdvanceStep {
                        note: "Mail is active".to_string(),
                    }
                } else if state.last_tool_name.as_deref() == Some("app_activate") {
                    WorkerAction::Replan {
                        reason: "Mail did not become frontmost after activation".to_string(),
                    }
                } else {
                    let mut request = tool(
                        "app_activate",
                        json!({ "app_name": "Mail" }),
                        RiskLevel::Low,
                    );
                    request.expected_outcome = Some("Mail is frontmost".to_string());
                    WorkerAction::Tool { request }
                }
            }
            "compose_mail" => {
                if state.last_tool_name.as_deref() == Some("mail_compose")
                    && observation_proved(state)
                {
                    WorkerAction::Complete {
                        message: "Mail draft prepared. I stopped before sending.".to_string(),
                    }
                } else if state.last_tool_name.as_deref() == Some("mail_compose") {
                    WorkerAction::Replan {
                        reason: "Mail draft could not be verified".to_string(),
                    }
                } else {
                    let recipient = extract_after_keyword(&state.user_request, "to");
                    let subject = extract_after_keyword(&state.user_request, "about")
                        .unwrap_or_else(|| "Jarvis follow-up".to_string());
                    let request = ToolCallRequest {
                        name: "mail_compose".to_string(),
                        arguments: json!({
                            "to": recipient,
                            "subject": subject,
                            "body": format!("Drafted by Jarvis for request: {}", state.user_request.trim()),
                        }),
                        risk: RiskLevel::Medium,
                        requires_confirmation: false,
                        target_identity: Some("com.apple.mail".to_string()),
                        expected_outcome: Some(
                            "A visible Mail draft exists with the requested details".to_string(),
                        ),
                    };
                    WorkerAction::Tool { request }
                }
            }
            "activate_messages" => {
                if observation_proved(state) || frontmost_app_matches(state, "Messages") {
                    WorkerAction::AdvanceStep {
                        note: "Messages is active".to_string(),
                    }
                } else if state.last_tool_name.as_deref() == Some("app_activate") {
                    WorkerAction::Replan {
                        reason: "Messages did not become frontmost after activation".to_string(),
                    }
                } else {
                    let mut request = tool(
                        "app_activate",
                        json!({ "app_name": "Messages" }),
                        RiskLevel::Low,
                    );
                    request.expected_outcome = Some("Messages is frontmost".to_string());
                    WorkerAction::Tool { request }
                }
            }
            "compose_message" => {
                if state.last_tool_name.as_deref() == Some("messages_compose")
                    && observation_proved(state)
                {
                    WorkerAction::Complete {
                        message: "Message draft prepared. I stopped before sending.".to_string(),
                    }
                } else if state.last_tool_name.as_deref() == Some("messages_compose") {
                    WorkerAction::Replan {
                        reason: "Messages draft could not be verified".to_string(),
                    }
                } else {
                    let recipient = extract_after_keyword(&state.user_request, "message")
                        .or_else(|| extract_after_keyword(&state.user_request, "text"))
                        .unwrap_or_else(|| "the requested contact".to_string());
                    let request = ToolCallRequest {
                        name: "messages_compose".to_string(),
                        arguments: json!({
                            "recipient": recipient,
                            "body": format!("Drafted by Jarvis for request: {}", state.user_request.trim()),
                        }),
                        risk: RiskLevel::Medium,
                        requires_confirmation: false,
                        target_identity: Some("com.apple.MobileSMS".to_string()),
                        expected_outcome: Some(
                            "A visible Messages draft exists for the requested recipient"
                                .to_string(),
                        ),
                    };
                    WorkerAction::Tool { request }
                }
            }
            "browser_search" => {
                if state.last_tool_name.as_deref() == Some("browser_assert")
                    && observation_proved(state)
                {
                    WorkerAction::Complete {
                        message: "Opened the browser target.".to_string(),
                    }
                } else if matches!(
                    state.last_tool_name.as_deref(),
                    Some("browser_open") | Some("chrome_open_tab")
                ) {
                    let mut request = tool("browser_snapshot", json!({}), RiskLevel::Low);
                    request.expected_outcome = Some("The relevant page is visible".to_string());
                    WorkerAction::Tool { request }
                } else if state.last_tool_name.as_deref() == Some("browser_snapshot") {
                    WorkerAction::Replan {
                        reason:
                            "Browser snapshot observed a page, but nothing verified that it satisfied the request"
                                .to_string(),
                    }
                } else {
                    let search_url =
                        if let Some(url) = state.user_request.split_whitespace().find(|token| {
                            token.starts_with("http://") || token.starts_with("https://")
                        }) {
                            url.to_string()
                        } else {
                            format!(
                                "https://www.google.com/search?q={}",
                                state.user_request.replace(' ', "+")
                            )
                        };
                    let mut request =
                        tool("browser_open", json!({ "url": search_url }), RiskLevel::Low);
                    request.expected_outcome = Some("The browser target is visible".to_string());
                    WorkerAction::Tool { request }
                }
            }
            _ => {
                if state.last_observation.is_some() {
                    WorkerAction::AdvanceStep {
                        note: format!("Completed {}", step.title),
                    }
                } else {
                    WorkerAction::Replan {
                        reason: format!("No worker rule for step '{}'", step.id),
                    }
                }
            }
        };

        Ok(WorkerDecision {
            assistant_message: Some(format!("Working on {}", step.title)),
            action: decision,
        })
    }

    fn provider_name(&self) -> &'static str {
        "heuristic-worker"
    }
}

fn frontmost_app_matches(state: &TaskState, expected: &str) -> bool {
    let expected = expected.to_ascii_lowercase();
    let observation = match state.last_observation.as_ref() {
        Some(observation) => observation,
        None => return false,
    };

    if let Some(target_identity) = observation.target_identity.as_ref() {
        if target_identity.to_ascii_lowercase().contains(&expected) {
            return true;
        }
    }

    let summary = observation.summary.to_ascii_lowercase();
    let details = observation
        .details
        .as_ref()
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();

    summary.contains(&expected) || details.contains(&expected)
}

fn observation_proved(state: &TaskState) -> bool {
    state
        .last_observation
        .as_ref()
        .map(|observation| observation.proof_passed && observation.success)
        .unwrap_or(false)
}

pub struct PlannerStack {
    providers: Vec<Arc<dyn PlannerProvider>>,
}

impl PlannerStack {
    pub fn new(providers: Vec<Arc<dyn PlannerProvider>>) -> Self {
        Self { providers }
    }
}

#[async_trait]
impl PlannerProvider for PlannerStack {
    async fn create_plan(
        &self,
        task_input: &str,
        capabilities: &AutomationCapabilities,
    ) -> Result<TaskPlan> {
        let mut errors = Vec::new();
        for provider in &self.providers {
            match provider.create_plan(task_input, capabilities).await {
                Ok(plan) => return Ok(plan),
                Err(error) => errors.push(format!("{}={}", provider.provider_name(), error)),
            }
        }
        bail!("planner failed across providers: {}", errors.join(" | "))
    }

    fn provider_name(&self) -> &'static str {
        "planner-stack"
    }
}

pub struct WorkerStack {
    providers: Vec<Arc<dyn WorkerProvider>>,
}

impl WorkerStack {
    pub fn new(providers: Vec<Arc<dyn WorkerProvider>>) -> Self {
        Self { providers }
    }
}

#[async_trait]
impl WorkerProvider for WorkerStack {
    async fn next_action(
        &self,
        state: &TaskState,
        capabilities: &AutomationCapabilities,
    ) -> Result<WorkerDecision> {
        let mut errors = Vec::new();
        for provider in &self.providers {
            match provider.next_action(state, capabilities).await {
                Ok(decision) => return Ok(decision),
                Err(error) => errors.push(format!("{}={}", provider.provider_name(), error)),
            }
        }
        bail!("worker failed across providers: {}", errors.join(" | "))
    }

    fn provider_name(&self) -> &'static str {
        "worker-stack"
    }
}

fn tool(name: &str, arguments: serde_json::Value, risk: RiskLevel) -> ToolCallRequest {
    ToolCallRequest {
        name: name.to_string(),
        arguments,
        risk,
        requires_confirmation: false,
        target_identity: None,
        expected_outcome: None,
    }
}

fn step(id: &str, title: &str, instruction: &str, completion_hint: &str) -> PlanStep {
    PlanStep {
        id: id.to_string(),
        title: title.to_string(),
        instruction: instruction.to_string(),
        completion_hint: completion_hint.to_string(),
        expected_outcome: Some(completion_hint.to_string()),
        status: Default::default(),
    }
}

fn extract_after_keyword(input: &str, keyword: &str) -> Option<String> {
    let lowered = input.to_ascii_lowercase();
    let needle = format!("{} ", keyword);
    let start = lowered.find(&needle)?;
    let slice = &input[start + needle.len()..];
    let value = slice
        .split(" about ")
        .next()
        .map(str::trim)
        .unwrap_or_default()
        .trim_matches('"')
        .trim_matches('\'');

    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn summarize_text(value: &str) -> String {
    value
        .split_whitespace()
        .take(40)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::AutomationCapabilities;

    fn caps() -> AutomationCapabilities {
        AutomationCapabilities {
            tools: vec!["app_activate".to_string(), "chrome_open_tab".to_string()],
            primary_browser: "Google Chrome".to_string(),
            chrome_installed: true,
            chrome_javascript_enabled: true,
            applescript_available: true,
            accessibility_expected: true,
            browser_automation_ready: true,
            browser_mode: Some("attached_existing".to_string()),
            setup_items: Vec::new(),
            browser_sidecar_endpoint: Some("http://127.0.0.1:4317".to_string()),
            known_apps: Vec::new(),
        }
    }

    #[tokio::test]
    async fn heuristic_planner_creates_schoology_plan() {
        let provider = HeuristicPlannerProvider;
        let plan = provider
            .create_plan("do my latest biology assignment in schoology", &caps())
            .await
            .unwrap();

        assert!(plan.steps.iter().any(|step| step.id == "open_schoology"));
    }

    #[tokio::test]
    async fn heuristic_worker_advances_after_app_resolve() {
        let worker = HeuristicWorkerProvider;
        let plan = HeuristicPlannerProvider
            .create_plan("open google chrome", &caps())
            .await
            .unwrap();
        let mut state = TaskState::new("open google chrome".to_string(), plan);
        state.last_tool_name = Some("app_resolve".to_string());
        state.last_observation = Some(crate::llm::schema::Observation {
            source: "app_resolve".to_string(),
            summary: "Resolved Google Chrome".to_string(),
            details: None,
            target_identity: Some("com.google.Chrome".to_string()),
            success: true,
            retryable: false,
            proof_passed: true,
            observed_outcome: Some("Google Chrome".to_string()),
        });

        let decision = worker.next_action(&state, &caps()).await.unwrap();
        assert!(matches!(decision.action, WorkerAction::AdvanceStep { .. }));
    }

    #[tokio::test]
    async fn heuristic_planner_errors_on_unknown_requests() {
        let provider = HeuristicPlannerProvider;
        let result = provider
            .create_plan("coordinate a vague autonomous workflow", &caps())
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn compose_mail_requires_verified_draft_before_completion() {
        let worker = HeuristicWorkerProvider;
        let plan = HeuristicPlannerProvider
            .create_plan("send an email to teacher about missing homework", &caps())
            .await
            .unwrap();
        let mut state = TaskState::new(
            "send an email to teacher about missing homework".to_string(),
            plan,
        );
        state.active_step_index = 1;
        state.last_tool_name = Some("mail_compose".to_string());
        state.last_observation = Some(crate::llm::schema::Observation {
            source: "mail_compose".to_string(),
            summary: "Prepared Mail draft (verification incomplete)".to_string(),
            details: None,
            target_identity: Some("com.apple.mail".to_string()),
            success: true,
            retryable: false,
            proof_passed: false,
            observed_outcome: None,
        });

        let decision = worker.next_action(&state, &caps()).await.unwrap();
        assert!(matches!(decision.action, WorkerAction::Replan { .. }));
    }

    #[tokio::test]
    async fn schoology_step_only_advances_after_browser_proof() {
        let worker = HeuristicWorkerProvider;
        let plan = HeuristicPlannerProvider
            .create_plan("open my latest biology assignment in schoology", &caps())
            .await
            .unwrap();
        let mut state = TaskState::new(
            "open my latest biology assignment in schoology".to_string(),
            plan,
        );
        state.active_step_index = 1;
        state.last_tool_name = Some("browser_assert".to_string());
        state.last_observation = Some(crate::llm::schema::Observation {
            source: "browser_assert".to_string(),
            summary: "Asserted browser state (verified)".to_string(),
            details: Some("Schoology".to_string()),
            target_identity: Some("https://app.schoology.com/home".to_string()),
            success: true,
            retryable: true,
            proof_passed: true,
            observed_outcome: Some("Log in to Schoology".to_string()),
        });

        let decision = worker.next_action(&state, &caps()).await.unwrap();
        assert!(matches!(decision.action, WorkerAction::AdvanceStep { .. }));
    }
}
