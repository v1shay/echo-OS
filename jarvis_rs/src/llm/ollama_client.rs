use anyhow::Result;
use reqwest::Client;
use serde_json::json;

#[derive(Clone, Debug)]
pub struct OllamaClient {
    http: Client,
    base_url: String,
}

impl OllamaClient {
    pub fn new() -> Self {
        Self {
            http: Client::new(),
            base_url: "http://localhost:11434".to_string(),
        }
    }

    pub async fn generate(&self, prompt: &str) -> Result<String> {
        let body = json!({
            "model": "llama3",
            "prompt": prompt,
            "stream": false
        });

        let response = self
            .http
            .post(format!("{}/api/generate", self.base_url))
            .json(&body)
            .send()
            .await?;

        let json: serde_json::Value = response.json().await?;

        let output = json["response"]
            .as_str()
            .unwrap_or("")
            .trim()
            .to_string();

        Ok(output)
    }
}