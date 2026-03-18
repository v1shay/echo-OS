use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentType {
    OpenApplication,
    ListFiles,
    CreateFolder,
    DeleteFile,
    SearchWeb,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IntentObject {
    pub intent: IntentType,
    pub parameters: serde_json::Value,
    pub risk_level: RiskLevel,
    pub requires_confirmation: bool,
}
