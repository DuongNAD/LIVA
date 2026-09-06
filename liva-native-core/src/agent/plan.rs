//! Task Planning & Step Tracking for Autonomous ReAct Agent (Milestone 3).
//!
//! Provides structured goal decomposition into a directed list of steps (up to 5 steps),
//! tracking step status, tool binding, intermediate results, and self-healing retries.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Maximum number of sequential steps allowed in a single ReAct TaskPlan.
pub const MAX_PLAN_STEPS: usize = 5;

/// Maximum number of tool-call retries allowed per plan step before declaring failure.
pub const MAX_TOOL_RETRIES_PER_STEP: usize = 3;

/// Default token budget allocated for a ReAct task execution loop.
pub const DEFAULT_REACT_TOKEN_BUDGET: usize = 4096;

/// Lifecycle status for an individual step in a `TaskPlan`.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(tag = "type", content = "details")]
pub enum StepStatus {
    Pending,
    InProgress,
    Completed,
    Failed { error: String, retry_count: usize },
    Skipped { reason: String },
}

impl Default for StepStatus {
    fn default() -> Self {
        Self::Pending
    }
}

/// An individual actionable step within a `TaskPlan`.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct PlanStep {
    pub id: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_arguments: Option<Value>,
    #[serde(default)]
    pub status: StepStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default)]
    pub retries: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl PlanStep {
    pub fn new(id: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            description: description.into(),
            tool_name: None,
            tool_arguments: None,
            status: StepStatus::Pending,
            result: None,
            retries: 0,
            error: None,
        }
    }

    pub fn with_tool(
        mut self,
        tool_name: impl Into<String>,
        tool_arguments: Value,
    ) -> Self {
        self.tool_name = Some(tool_name.into());
        self.tool_arguments = Some(tool_arguments);
        self
    }

    pub fn is_completed(&self) -> bool {
        matches!(self.status, StepStatus::Completed)
    }

    pub fn is_failed(&self) -> bool {
        matches!(self.status, StepStatus::Failed { .. })
    }

    pub fn can_retry(&self, max_retries: usize) -> bool {
        self.retries < max_retries
    }

    pub fn mark_in_progress(&mut self) {
        self.status = StepStatus::InProgress;
    }

    pub fn mark_completed(&mut self, output: Value) {
        self.status = StepStatus::Completed;
        self.result = Some(output);
        self.error = None;
    }

    pub fn mark_failed(&mut self, error: impl Into<String>) {
        let err_str = error.into();
        self.retries += 1;
        self.status = StepStatus::Failed {
            error: err_str.clone(),
            retry_count: self.retries,
        };
        self.error = Some(err_str);
    }

    pub fn mark_skipped(&mut self, reason: impl Into<String>) {
        self.status = StepStatus::Skipped {
            reason: reason.into(),
        };
    }
}

/// A structured multi-step plan decomposed from a user goal.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct TaskPlan {
    pub id: String,
    pub goal: String,
    pub steps: Vec<PlanStep>,
    #[serde(default)]
    pub current_step: usize,
    #[serde(default)]
    pub completed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

impl TaskPlan {
    pub fn new(goal: impl Into<String>, steps: Vec<PlanStep>) -> Self {
        let goal_str = goal.into();
        let plan_id = format!("plan-{}", uuid::Uuid::new_v4());
        let mut steps = steps;
        if steps.len() > MAX_PLAN_STEPS {
            tracing::warn!(
                "TaskPlan steps ({}) exceeded MAX_PLAN_STEPS ({}). Truncating.",
                steps.len(),
                MAX_PLAN_STEPS
            );
            steps.truncate(MAX_PLAN_STEPS);
        }
        Self {
            id: plan_id,
            goal: goal_str,
            steps,
            current_step: 0,
            completed: false,
            summary: None,
        }
    }

    pub fn add_step(&mut self, step: PlanStep) -> Result<(), String> {
        if self.steps.len() >= MAX_PLAN_STEPS {
            return Err(format!(
                "Cannot add step: maximum allowed steps ({MAX_PLAN_STEPS}) reached"
            ));
        }
        self.steps.push(step);
        Ok(())
    }

    pub fn active_step(&self) -> Option<&PlanStep> {
        self.steps.get(self.current_step)
    }

    pub fn active_step_mut(&mut self) -> Option<&mut PlanStep> {
        self.steps.get_mut(self.current_step)
    }

    pub fn next_pending_step(&self) -> Option<(usize, &PlanStep)> {
        for (idx, step) in self.steps.iter().enumerate() {
            if matches!(step.status, StepStatus::Pending | StepStatus::InProgress) {
                return Some((idx, step));
            }
        }
        None
    }

    pub fn mark_step_completed(&mut self, step_index: usize, output: Value) {
        if let Some(step) = self.steps.get_mut(step_index) {
            step.mark_completed(output);
        }
        self.check_completion();
    }

    pub fn mark_step_failed(&mut self, step_index: usize, error: impl Into<String>) {
        if let Some(step) = self.steps.get_mut(step_index) {
            step.mark_failed(error);
        }
        self.check_completion();
    }

    pub fn advance(&mut self) -> Option<usize> {
        if self.current_step + 1 < self.steps.len() {
            self.current_step += 1;
            Some(self.current_step)
        } else {
            self.completed = true;
            None
        }
    }

    pub fn check_completion(&mut self) {
        if self.steps.is_empty() {
            self.completed = true;
            return;
        }
        let all_done = self.steps.iter().all(|s| {
            matches!(
                s.status,
                StepStatus::Completed | StepStatus::Skipped { .. } | StepStatus::Failed { .. }
            )
        });
        if all_done {
            self.completed = true;
        }
    }

    pub fn is_finished(&self) -> bool {
        self.completed || (self.current_step >= self.steps.len() && !self.steps.is_empty())
    }

    pub fn successful_steps_count(&self) -> usize {
        self.steps.iter().filter(|s| s.is_completed()).count()
    }

    pub fn progress_percentage(&self) -> f32 {
        if self.steps.is_empty() {
            return 100.0;
        }
        (self.successful_steps_count() as f32 / self.steps.len() as f32) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_task_plan_creation_and_bounds() {
        let mut steps = Vec::new();
        for i in 1..=10 {
            steps.push(PlanStep::new(format!("s{i}"), format!("Step {i}")));
        }
        let plan = TaskPlan::new("Goal with 10 steps", steps);
        assert_eq!(
            plan.steps.len(),
            MAX_PLAN_STEPS,
            "Must truncate to MAX_PLAN_STEPS"
        );
        assert_eq!(plan.goal, "Goal with 10 steps");
        assert!(!plan.is_finished());
    }

    #[test]
    fn test_step_lifecycle_and_retries() {
        let mut step = PlanStep::new("step-1", "Fetch weather data")
            .with_tool("get_weather", json!({"location": "Hanoi"}));

        assert_eq!(step.status, StepStatus::Pending);
        assert!(step.can_retry(MAX_TOOL_RETRIES_PER_STEP));

        step.mark_in_progress();
        assert_eq!(step.status, StepStatus::InProgress);

        // Fail once
        step.mark_failed("Network timeout");
        assert_eq!(step.retries, 1);
        assert!(step.is_failed());
        assert!(step.can_retry(MAX_TOOL_RETRIES_PER_STEP));

        // Fail twice
        step.mark_failed("503 Service Unavailable");
        assert_eq!(step.retries, 2);
        assert!(step.can_retry(MAX_TOOL_RETRIES_PER_STEP));

        // Fail third time
        step.mark_failed("404 Not Found");
        assert_eq!(step.retries, 3);
        assert!(!step.can_retry(MAX_TOOL_RETRIES_PER_STEP));

        // Complete
        step.mark_completed(json!({"temp": 28, "unit": "C"}));
        assert!(step.is_completed());
        assert_eq!(step.result, Some(json!({"temp": 28, "unit": "C"})));
    }

    #[test]
    fn test_task_plan_advance_and_progress() {
        let steps = vec![
            PlanStep::new("s1", "Search user prefs"),
            PlanStep::new("s2", "Control light"),
            PlanStep::new("s3", "Send confirmation"),
        ];
        let mut plan = TaskPlan::new("Evening routine", steps);

        assert_eq!(plan.current_step, 0);
        assert_eq!(plan.progress_percentage(), 0.0);

        plan.mark_step_completed(0, json!({"status": "found"}));
        assert_eq!(plan.advance(), Some(1));
        assert_eq!(plan.current_step, 1);

        plan.mark_step_completed(1, json!({"status": "on"}));
        assert_eq!(plan.advance(), Some(2));
        assert_eq!(plan.current_step, 2);

        plan.mark_step_completed(2, json!({"sent": true}));
        assert_eq!(plan.advance(), None);
        assert!(plan.is_finished());
        assert_eq!(plan.progress_percentage(), 100.0);
    }
}
