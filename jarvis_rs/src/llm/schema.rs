use serde::{Deserialize, Serialize};

use crate::automation::ToolCallRequest;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDecision {
    pub assistant_message: String,
    pub summary: Option<String>,
    #[serde(default)]
    pub actions: Vec<ToolCallRequest>,
}
