use serde::{Deserialize, Serialize};

// Intentionally minimal placeholder types.
// These should be replaced with strict request/response structs matching Ollama's API.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmPrompt {
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub text: String,
}
