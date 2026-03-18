use serde::{Deserialize, Serialize};

use crate::automation::AutomationCapabilities;
use crate::config::AppConfig;
use crate::llm::schema::{Observation, TaskPlan};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ActionSpec {
    pub name: String,
    #[serde(default)]
    pub expected_outcome: Option<String>,
    #[serde(default)]
    pub target_identity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ObservationEvidence {
    pub source: String,
    pub summary: String,
    #[serde(default)]
    pub details: Option<String>,
    #[serde(default)]
    pub target_identity: Option<String>,
    #[serde(default)]
    pub observed_outcome: Option<String>,
    #[serde(default)]
    pub proof_passed: bool,
    #[serde(default)]
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VerificationResult {
    pub satisfied: bool,
    pub confidence: f32,
    pub summary: String,
    #[serde(default)]
    pub matched_criteria: Vec<String>,
    #[serde(default)]
    pub missing_criteria: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryKind {
    RetryStep,
    RefocusApp,
    ReacquireTarget,
    Replan,
    Abort,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryDecision {
    pub kind: RecoveryKind,
    pub reason: String,
    #[serde(default)]
    pub next_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskJournalEntry {
    pub phase: String,
    pub message: String,
    #[serde(default)]
    pub action: Option<ActionSpec>,
    #[serde(default)]
    pub evidence: Option<ObservationEvidence>,
    #[serde(default)]
    pub verification: Option<VerificationResult>,
    #[serde(default)]
    pub recovery: Option<RecoveryDecision>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskJournal {
    #[serde(default)]
    pub entries: Vec<TaskJournalEntry>,
    #[serde(default)]
    pub final_verification: Option<VerificationResult>,
}

impl TaskJournal {
    pub fn push(&mut self, entry: TaskJournalEntry) {
        self.entries.push(entry);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PermissionStatus {
    pub accessibility: bool,
    pub automation: bool,
    pub microphone: bool,
    pub browser_sidecar: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuntimeManifest {
    pub platform: String,
    pub primary_browser: String,
    pub planner_provider: String,
    pub worker_provider: String,
    #[serde(default)]
    pub fallback_model: Option<String>,
    #[serde(default)]
    pub allowed_paths: Vec<String>,
    pub permissions: PermissionStatus,
    #[serde(default)]
    pub setup_items: Vec<String>,
    #[serde(default)]
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioSpec {
    pub id: String,
    pub summary: String,
    #[serde(default)]
    pub success_criteria: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioOutcome {
    pub id: String,
    pub success: bool,
    pub summary: String,
    #[serde(default)]
    pub verification: Option<VerificationResult>,
    #[serde(default)]
    pub artifact_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunReport {
    pub runtime: RuntimeManifest,
    #[serde(default)]
    pub scenarios: Vec<ScenarioOutcome>,
}

pub fn build_runtime_manifest(
    config: &AppConfig,
    capabilities: &AutomationCapabilities,
    diagnostics: &[String],
    planner_provider: &str,
    worker_provider: &str,
) -> RuntimeManifest {
    let accessibility_ready = !capabilities.setup_items.iter().any(|item| {
        item.to_ascii_lowercase()
            .contains("accessibility permission is blocked")
    });
    let browser_sidecar_ready = capabilities.browser_automation_ready
        && !capabilities.setup_items.iter().any(|item| {
            item.to_ascii_lowercase()
                .contains("browser sidecar is not running yet")
        });
    RuntimeManifest {
        platform: std::env::consts::OS.to_string(),
        primary_browser: config.primary_browser.clone(),
        planner_provider: planner_provider.to_string(),
        worker_provider: worker_provider.to_string(),
        fallback_model: config.provider.fallback_model.clone(),
        allowed_paths: config
            .allowed_paths
            .iter()
            .map(|path| path.display().to_string())
            .collect(),
        permissions: PermissionStatus {
            accessibility: accessibility_ready,
            automation: capabilities.applescript_available && accessibility_ready,
            microphone: true,
            browser_sidecar: browser_sidecar_ready,
        },
        setup_items: capabilities.setup_items.clone(),
        diagnostics: diagnostics.to_vec(),
    }
}

pub fn evidence_from_observation(observation: &Observation) -> ObservationEvidence {
    ObservationEvidence {
        source: observation.source.clone(),
        summary: observation.summary.clone(),
        details: observation.details.clone(),
        target_identity: observation.target_identity.clone(),
        observed_outcome: observation.observed_outcome.clone(),
        proof_passed: observation.proof_passed,
        success: observation.success,
    }
}

pub fn verify_plan_completion(
    plan: &TaskPlan,
    observation: Option<&Observation>,
) -> VerificationResult {
    let Some(observation) = observation else {
        return VerificationResult {
            satisfied: false,
            confidence: 0.0,
            summary: "No observation was available to verify the final task state".to_string(),
            matched_criteria: Vec::new(),
            missing_criteria: plan.success_criteria.clone(),
        };
    };

    let evidence = [
        observation.summary.as_str(),
        observation.details.as_deref().unwrap_or_default(),
        observation.observed_outcome.as_deref().unwrap_or_default(),
        observation.target_identity.as_deref().unwrap_or_default(),
    ]
    .join(" ")
    .to_ascii_lowercase();

    let mut matched = Vec::new();
    let mut missing = Vec::new();
    for criterion in &plan.success_criteria {
        let tokens = criterion
            .split_whitespace()
            .filter(|token| token.len() > 3)
            .map(|token| token.to_ascii_lowercase())
            .collect::<Vec<_>>();
        if tokens.is_empty() || tokens.iter().any(|token| evidence.contains(token)) {
            matched.push(criterion.clone());
        } else {
            missing.push(criterion.clone());
        }
    }

    let satisfied = observation.success && observation.proof_passed && missing.is_empty();
    let confidence = if satisfied {
        0.95
    } else if observation.proof_passed {
        0.55
    } else {
        0.15
    };
    VerificationResult {
        satisfied,
        confidence,
        summary: if satisfied {
            "Final verifier confirmed the requested task outcome".to_string()
        } else {
            "Final verifier could not confirm every success criterion".to_string()
        },
        matched_criteria: matched,
        missing_criteria: missing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifier_requires_observation() {
        let plan = TaskPlan {
            assistant_message: String::new(),
            summary: "test".to_string(),
            goal: "test".to_string(),
            success_criteria: vec!["Finder is frontmost".to_string()],
            steps: Vec::new(),
        };
        let result = verify_plan_completion(&plan, None);
        assert!(!result.satisfied);
        assert_eq!(result.missing_criteria.len(), 1);
    }
}
