use anyhow::Result;
use reqwest::Client;
use serde_json::json;

#[derive(Clone, Debug)]
pub struct OllamaClient {
    http: Client,
    base_url: String,
    model: String,
}

impl OllamaClient {
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            http: Client::new(),
            base_url: base_url.into(),
            model: model.into(),
        }
    }

    pub async fn generate(&self, prompt: &str) -> Result<String> {
        let body = json!({
            "model": self.model,
            "prompt": prompt,
            "stream": false
        });

        let response = self
            .http
            .post(format!("{}/api/generate", self.base_url))
            .json(&body)
            .send()
            .await?
            .error_for_status()?;

        let json: serde_json::Value = response.json().await?;

        let output = json["response"]
            .as_str()
            .unwrap_or("")
            .trim()
            .to_string();

        Ok(output)
    }
}
