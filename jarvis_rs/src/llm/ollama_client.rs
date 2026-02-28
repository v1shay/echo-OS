use anyhow::Result;
use reqwest::Client;

use crate::llm::schema::{LlmPrompt, LlmResponse};

#[derive(Debug, Clone)]
pub struct OllamaClient {
    http: Client,
    base_url: String,
}

impl OllamaClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            http: Client::new(),
            base_url: base_url.into(),
        }
    }

    pub async fn complete(&self, _prompt: LlmPrompt) -> Result<LlmResponse> {
        // TODO: implement Ollama HTTP call via reqwest.
        anyhow::bail!("ollama client not implemented")
    }
}
