use serde::{Deserialize, Serialize};

use crate::automation::{AppTarget, BrowserTarget, ToolCallRequest, UiElementRef};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPlan {
    pub assistant_message: String,
    pub summary: String,
    pub goal: String,
    #[serde(default)]
    pub success_criteria: Vec<String>,
    #[serde(default)]
    pub steps: Vec<PlanStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub id: String,
    pub title: String,
    pub instruction: String,
    #[serde(default)]
    pub completion_hint: String,
    #[serde(default)]
    pub status: StepStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    #[default]
    Pending,
    InProgress,
    Completed,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub source: String,
    pub summary: String,
    #[serde(default)]
    pub details: Option<String>,
    #[serde(default)]
    pub target_identity: Option<String>,
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub retryable: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CompletionStatus {
    #[default]
    InProgress,
    AwaitingApproval,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskState {
    pub user_request: String,
    pub plan: TaskPlan,
    #[serde(default)]
    pub active_step_index: usize,
    #[serde(default)]
    pub last_observation: Option<Observation>,
    #[serde(default)]
    pub retries: u32,
    #[serde(default)]
    pub awaiting_approval: bool,
    #[serde(default)]
    pub completion_status: CompletionStatus,
    #[serde(default)]
    pub last_tool_name: Option<String>,
    #[serde(default)]
    pub active_app: Option<AppTarget>,
    #[serde(default)]
    pub active_browser: Option<BrowserTarget>,
    #[serde(default)]
    pub cached_ui_targets: Vec<UiElementRef>,
}

impl TaskState {
    pub fn new(user_request: String, plan: TaskPlan) -> Self {
        let mut state = Self {
            user_request,
            plan,
            active_step_index: 0,
            last_observation: None,
            retries: 0,
            awaiting_approval: false,
            completion_status: CompletionStatus::InProgress,
            last_tool_name: None,
            active_app: None,
            active_browser: None,
            cached_ui_targets: Vec::new(),
        };
        state.sync_step_statuses();
        state
    }

    pub fn current_step(&self) -> Option<&PlanStep> {
        self.plan.steps.get(self.active_step_index)
    }

    pub fn current_step_mut(&mut self) -> Option<&mut PlanStep> {
        self.plan.steps.get_mut(self.active_step_index)
    }

    pub fn advance_step(&mut self) {
        if let Some(step) = self.current_step_mut() {
            step.status = StepStatus::Completed;
        }
        self.active_step_index = self.active_step_index.saturating_add(1);
        self.sync_step_statuses();
    }

    pub fn block_current_step(&mut self) {
        if let Some(step) = self.current_step_mut() {
            step.status = StepStatus::Blocked;
        }
    }

    pub fn mark_in_progress(&mut self) {
        self.sync_step_statuses();
    }

    pub fn reset_for_replan(&mut self, plan: TaskPlan) {
        self.plan = plan;
        self.active_step_index = 0;
        self.retries = self.retries.saturating_add(1);
        self.sync_step_statuses();
    }

    fn sync_step_statuses(&mut self) {
        for (index, step) in self.plan.steps.iter_mut().enumerate() {
            if matches!(step.status, StepStatus::Completed | StepStatus::Blocked) {
                continue;
            }
            step.status = if index < self.active_step_index {
                StepStatus::Completed
            } else if index == self.active_step_index {
                StepStatus::InProgress
            } else {
                StepStatus::Pending
            };
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerDecision {
    #[serde(default)]
    pub assistant_message: Option<String>,
    pub action: WorkerAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkerAction {
    Tool { request: ToolCallRequest },
    AdvanceStep { note: String },
    Replan { reason: String },
    Complete { message: String },
}
