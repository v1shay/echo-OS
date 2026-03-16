use std::sync::Arc;

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::automation::{RiskLevel, ToolCallRequest};

use super::ollama_client::OllamaClient;
use super::prompt::build_intent_prompt;
use super::schema::AgentDecision;

#[async_trait]
pub trait PlanningProvider: Send + Sync {
    async fn plan_next_step(&self, user_input: &str, tools: &[&str]) -> Result<AgentDecision>;
    fn provider_name(&self) -> &'static str;
}

#[derive(Clone)]
pub struct OllamaPlanningProvider {
    client: OllamaClient,
}

impl OllamaPlanningProvider {
    pub fn new(base_url: String, model: String) -> Self {
        Self {
            client: OllamaClient::new(base_url, model),
        }
    }
}

#[async_trait]
impl PlanningProvider for OllamaPlanningProvider {
    async fn plan_next_step(&self, user_input: &str, tools: &[&str]) -> Result<AgentDecision> {
        let prompt = build_intent_prompt(user_input, tools);
        let raw = self.client.generate(&prompt).await?;
        let decision: AgentDecision =
            serde_json::from_str(&raw).context("Ollama returned invalid JSON")?;
        Ok(decision)
    }

    fn provider_name(&self) -> &'static str {
        "ollama"
    }
}

#[derive(Clone)]
pub struct OpenAiCompatibleProvider {
    base_url: String,
    model: String,
    api_key: String,
    client: reqwest::Client,
}

impl OpenAiCompatibleProvider {
    pub fn new(base_url: String, model: String, api_key: String) -> Self {
        Self {
            base_url,
            model,
            api_key,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl PlanningProvider for OpenAiCompatibleProvider {
    async fn plan_next_step(&self, user_input: &str, tools: &[&str]) -> Result<AgentDecision> {
        let prompt = build_intent_prompt(user_input, tools);
        let endpoint = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let response = self
            .client
            .post(endpoint)
            .bearer_auth(&self.api_key)
            .json(&json!({
                "model": self.model,
                "messages": [
                    {"role": "system", "content": "You are Jarvis, a strict JSON planner."},
                    {"role": "user", "content": prompt}
                ]
            }))
            .send()
            .await?
            .error_for_status()?;

        let body: Value = response.json().await?;
        let content = body["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_string();
        let decision: AgentDecision = serde_json::from_str(&content)
            .context("OpenAI-compatible provider returned invalid JSON")?;
        Ok(decision)
    }

    fn provider_name(&self) -> &'static str {
        "openai-compatible"
    }
}

#[derive(Debug, Default)]
pub struct HeuristicPlanningProvider;

#[async_trait]
impl PlanningProvider for HeuristicPlanningProvider {
    async fn plan_next_step(&self, user_input: &str, _tools: &[&str]) -> Result<AgentDecision> {
        let lowered = user_input.to_ascii_lowercase();

        if lowered.contains("create folder") || lowered.contains("make folder") {
            let path = user_input
                .split("called")
                .nth(1)
                .map(|name| format!("{}/{}", std::env::var("HOME").unwrap_or_else(|_| ".".to_string()), name.trim()))
                .unwrap_or_else(|| "./jarvis-folder".to_string());
            return Ok(AgentDecision {
                assistant_message: "Creating the folder now.".to_string(),
                summary: Some(format!("Create folder {}", path)),
                actions: vec![ToolCallRequest {
                    name: "create_folder".to_string(),
                    arguments: json!({ "path": path }),
                    risk: RiskLevel::Medium,
                    requires_confirmation: false,
                }],
            });
        }

        if lowered.contains("list") && lowered.contains("file") {
            return Ok(AgentDecision {
                assistant_message: "Listing the files for you.".to_string(),
                summary: Some("List files".to_string()),
                actions: vec![ToolCallRequest {
                    name: "list_dir".to_string(),
                    arguments: json!({ "path": "." }),
                    risk: RiskLevel::Low,
                    requires_confirmation: false,
                }],
            });
        }

        if lowered.contains("delete") {
            let path = user_input
                .split_once("delete")
                .map(|(_, tail)| tail.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "./target".to_string());
            return Ok(AgentDecision {
                assistant_message: format!("I can delete {}, but I need your approval first.", path),
                summary: Some(format!("Delete {}", path)),
                actions: vec![ToolCallRequest {
                    name: "delete_path".to_string(),
                    arguments: json!({ "path": path }),
                    risk: RiskLevel::High,
                    requires_confirmation: true,
                }],
            });
        }

        if lowered.contains("schoology") {
            let url = user_input
                .split_whitespace()
                .find(|token| token.starts_with("http://") || token.starts_with("https://"))
                .map(|token| token.to_string())
                .unwrap_or_else(|| "https://app.schoology.com/home".to_string());
            return Ok(AgentDecision {
                assistant_message: "Opening Schoology.".to_string(),
                summary: Some("Open Schoology".to_string()),
                actions: vec![ToolCallRequest {
                    name: "browser_open".to_string(),
                    arguments: json!({ "url": url }),
                    risk: RiskLevel::Low,
                    requires_confirmation: false,
                }],
            });
        }

        if lowered.contains("open ") {
            let app_name = user_input
                .split_once("open ")
                .map(|(_, tail)| tail.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "Finder".to_string());
            return Ok(AgentDecision {
                assistant_message: format!("Opening {} and reporting back.", app_name),
                summary: Some(format!("Open {}", app_name)),
                actions: vec![ToolCallRequest {
                    name: "open_app".to_string(),
                    arguments: json!({ "app_name": app_name }),
                    risk: RiskLevel::Low,
                    requires_confirmation: false,
                }],
            });
        }

        if lowered.contains("email") || lowered.starts_with("send mail") || lowered.starts_with("send an email") {
            let recipient = extract_after_keyword(user_input, "to");
            let subject = extract_after_keyword(user_input, "about")
                .unwrap_or_else(|| "Jarvis follow-up".to_string());
            let body = format!("Drafted by Jarvis for request: {}", user_input.trim());
            return Ok(AgentDecision {
                assistant_message: "Opening a draft email for you.".to_string(),
                summary: Some("Draft email".to_string()),
                actions: vec![ToolCallRequest {
                    name: "draft_email".to_string(),
                    arguments: json!({
                        "to": recipient,
                        "subject": subject,
                        "body": body,
                    }),
                    risk: RiskLevel::Medium,
                    requires_confirmation: false,
                }],
            });
        }

        if lowered.contains("message ") || lowered.starts_with("text ") {
            let recipient = extract_after_keyword(user_input, "message")
                .or_else(|| extract_after_keyword(user_input, "text"));
            return Ok(AgentDecision {
                assistant_message: "Opening Messages for you.".to_string(),
                summary: Some("Open Messages".to_string()),
                actions: vec![ToolCallRequest {
                    name: "open_messages".to_string(),
                    arguments: json!({
                        "recipient": recipient,
                        "body": format!("Drafted by Jarvis for request: {}", user_input.trim()),
                    }),
                    risk: RiskLevel::Medium,
                    requires_confirmation: false,
                }],
            });
        }

        if lowered.contains("browser") || lowered.contains("search") || lowered.contains("web") {
            let url = if lowered.contains("http://") || lowered.contains("https://") {
                user_input.trim().to_string()
            } else {
                format!("https://www.google.com/search?q={}", user_input.replace(' ', "+"))
            };
            return Ok(AgentDecision {
                assistant_message: "Opening the browser result.".to_string(),
                summary: Some("Open browser".to_string()),
                actions: vec![ToolCallRequest {
                    name: "browser_open".to_string(),
                    arguments: json!({ "url": url }),
                    risk: RiskLevel::Low,
                    requires_confirmation: false,
                }],
            });
        }

        Ok(AgentDecision {
            assistant_message: format!(
                "I understood: '{}'. Ask me to open apps, create folders, browse, run shell commands, or delete paths.",
                user_input
            ),
            summary: None,
            actions: Vec::new(),
        })
    }

    fn provider_name(&self) -> &'static str {
        "heuristic"
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
        .unwrap_or_default();

    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

pub struct ProviderStack {
    primary: Arc<dyn PlanningProvider>,
    secondary: Arc<dyn PlanningProvider>,
    fallback: Arc<dyn PlanningProvider>,
}

impl ProviderStack {
    pub fn new(
        primary: Arc<dyn PlanningProvider>,
        secondary: Arc<dyn PlanningProvider>,
        fallback: Arc<dyn PlanningProvider>,
    ) -> Self {
        Self {
            primary,
            secondary,
            fallback,
        }
    }
}

#[async_trait]
impl PlanningProvider for ProviderStack {
    async fn plan_next_step(&self, user_input: &str, tools: &[&str]) -> Result<AgentDecision> {
        let primary_error = match self.primary.plan_next_step(user_input, tools).await {
            Ok(decision) => return Ok(decision),
            Err(error) => error,
        };

        let secondary_error = match self.secondary.plan_next_step(user_input, tools).await {
            Ok(decision) => return Ok(decision),
            Err(error) => error,
        };

        match self.fallback.plan_next_step(user_input, tools).await {
            Ok(decision) => Ok(decision),
            Err(error) => bail!(
                "planning failed with primary={}, secondary={}, fallback={}",
                primary_error,
                secondary_error,
                error
            ),
        }
    }

    fn provider_name(&self) -> &'static str {
        "provider-stack"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn heuristic_delete_requires_confirmation() {
        let provider = HeuristicPlanningProvider;
        let result = provider
            .plan_next_step("delete /tmp/demo.txt", &["delete_path"])
            .await
            .unwrap();

        assert_eq!(result.actions.len(), 1);
        assert!(result.actions[0].requires_confirmation);
        assert_eq!(result.actions[0].risk, RiskLevel::High);
    }

    #[tokio::test]
    async fn heuristic_conversation_has_no_actions() {
        let provider = HeuristicPlanningProvider;
        let result = provider
            .plan_next_step("hello there", &["open_app"])
            .await
            .unwrap();

        assert!(result.actions.is_empty());
        assert!(result.assistant_message.contains("I understood"));
    }

    #[tokio::test]
    async fn heuristic_email_builds_draft_action() {
        let provider = HeuristicPlanningProvider;
        let result = provider
            .plan_next_step("send an email to teacher@example.com about homework", &["draft_email"])
            .await
            .unwrap();

        assert_eq!(result.actions[0].name, "draft_email");
    }

    #[tokio::test]
    async fn heuristic_schoology_opens_browser() {
        let provider = HeuristicPlanningProvider;
        let result = provider
            .plan_next_step("open schoology", &["browser_open"])
            .await
            .unwrap();

        assert_eq!(result.actions[0].name, "browser_open");
    }
}
