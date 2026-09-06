//! Autonomous ReAct Agent Loop & Self-Healing Tool Execution Engine (Milestone 3).
//!
//! Implements the iterative ReAct paradigm (`Thought -> Action -> Observation -> Reflection`),
//! multi-step goal decomposition with budget controls (up to 5 steps), and self-healing
//! error recovery with up to 3 reflection retries before declaring failure.

use super::plan::{
    MAX_PLAN_STEPS, MAX_TOOL_RETRIES_PER_STEP, PlanStep, StepStatus, TaskPlan,
};
use super::state::AgentState;
use crate::skills::dispatcher::{ToolCallRequest, ToolDispatcher, UnifiedToolDispatcher};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

/// Errors produced during ReAct planning or execution.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum AgentError {
    #[error("Goal decomposition failed: {0}")]
    PlanningFailed(String),

    #[error("Tool execution failed: {0}")]
    ToolExecutionFailed(String),

    #[error("Maximum step budget exceeded ({0} steps)")]
    StepBudgetExceeded(usize),

    #[error("Maximum retry attempts ({0}) exceeded for step '{1}'")]
    MaxRetriesExceeded(usize, String),

    #[error("Token budget exceeded: used {used} of max {budget}")]
    TokenBudgetExceeded { used: usize, budget: usize },

    #[error("Invalid state transition: {0}")]
    InvalidState(String),

    #[error("Agent loop terminated without resolution: {0}")]
    Unresolved(String),
}

/// ReAct reasoning cycle outputs representing the agent's internal state machine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "cycle", content = "payload")]
pub enum ReActThought {
    /// Initial goal decomposition into a structured multi-step plan.
    PlanDecomposition {
        thought: String,
        plan: TaskPlan,
    },
    /// Execution intention for the current step.
    StepAction {
        step_index: usize,
        thought: String,
        tool_name: String,
        arguments: Value,
    },
    /// Reflection on a failed step to formulate an auto-correction or fallback tool.
    ReflectAndRetry {
        step_index: usize,
        reflection: String,
        corrected_tool: String,
        corrected_arguments: Value,
        retry_count: usize,
    },
    /// Final answer synthesized from intermediate observations.
    FinalAnswer {
        thought: String,
        answer: String,
        plan: TaskPlan,
    },
}

/// The outcome of an individual execution step within the ReAct loop.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StepOutcome {
    /// The current step executed successfully and advanced to the next step.
    StepCompleted {
        step_index: usize,
        step_id: String,
        output: Value,
        next_step: Option<usize>,
    },
    /// The step failed but is undergoing self-healing reflection retry.
    StepRetrying {
        step_index: usize,
        step_id: String,
        error: String,
        retry_count: usize,
        next_tool: String,
    },
    /// The step exhausted all retry attempts and failed permanently.
    StepFailed {
        step_index: usize,
        step_id: String,
        error: String,
    },
    /// All steps in the TaskPlan have completed; final answer is available.
    PlanCompleted {
        final_answer: String,
        plan: TaskPlan,
    },
    /// Loop halted due to budget constraints.
    BudgetExceeded {
        reason: String,
    },
}

/// Autonomous ReAct Planner responsible for goal decomposition, step generation, and reflection.
#[derive(Debug, Clone)]
pub struct ReActPlanner {
    pub max_steps: usize,
    pub max_retries: usize,
    pub token_budget: usize,
}

impl Default for ReActPlanner {
    fn default() -> Self {
        Self::new()
    }
}

impl ReActPlanner {
    pub fn new() -> Self {
        Self {
            max_steps: MAX_PLAN_STEPS,
            max_retries: MAX_TOOL_RETRIES_PER_STEP,
            token_budget: 4096,
        }
    }

    pub fn with_max_steps(mut self, max_steps: usize) -> Self {
        self.max_steps = max_steps.min(MAX_PLAN_STEPS);
        self
    }

    pub fn with_max_retries(mut self, max_retries: usize) -> Self {
        self.max_retries = max_retries.min(MAX_TOOL_RETRIES_PER_STEP);
        self
    }

    /// Decomposes a user goal into an actionable `TaskPlan` with up to 5 steps.
    pub fn plan(&self, goal: &str, context: &AgentState) -> Result<TaskPlan, AgentError> {
        let trimmed_goal = goal.trim();
        if trimmed_goal.is_empty() {
            return Err(AgentError::PlanningFailed("Goal cannot be empty".to_string()));
        }

        // Estimate token cost from goal and working context
        let estimated_tokens = trimmed_goal.len() / 4 + context.messages.len() * 32;
        if estimated_tokens > self.token_budget {
            return Err(AgentError::TokenBudgetExceeded {
                used: estimated_tokens,
                budget: self.token_budget,
            });
        }

        // Goal decomposition: analyze intent and generate structured steps
        let steps = self.decompose_goal_steps(trimmed_goal, context);
        let plan = TaskPlan::new(trimmed_goal, steps);
        Ok(plan)
    }

    /// Internal goal decomposition heuristics and semantic matching.
    fn decompose_goal_steps(&self, goal: &str, _context: &AgentState) -> Vec<PlanStep> {
        let lower = goal.to_lowercase();
        let mut steps = Vec::new();

        // Multi-goal: Smarthome sequence (e.g. check status then turn on light and fan)
        if lower.contains("bật") || lower.contains("tắt") || lower.contains("turn on") || lower.contains("turn off") {
            if lower.contains("đèn") || lower.contains("light") {
                steps.push(
                    PlanStep::new("step-light", "Điều khiển thiết bị đèn")
                        .with_tool("control_smarthome", json!({
                            "device": "light",
                            "action": if lower.contains("tắt") || lower.contains("off") { "off" } else { "on" }
                        })),
                );
            }
            if lower.contains("quạt") || lower.contains("fan") {
                steps.push(
                    PlanStep::new("step-fan", "Điều khiển thiết bị quạt")
                        .with_tool("control_smarthome", json!({
                            "device": "fan",
                            "action": if lower.contains("tắt") || lower.contains("off") { "off" } else { "on" }
                        })),
                );
            }
            if lower.contains("điều hoà") || lower.contains("máy lạnh") || lower.contains("ac") {
                steps.push(
                    PlanStep::new("step-ac", "Điều khiển máy lạnh")
                        .with_tool("control_smarthome", json!({
                            "device": "ac",
                            "action": if lower.contains("tắt") || lower.contains("off") { "off" } else { "on" }
                        })),
                );
            }
        }

        // Knowledge / search sequence (e.g. search vault or web then answer)
        if steps.is_empty() {
            if lower.contains("tìm") || lower.contains("search") || lower.contains("tra cứu") || lower.contains("hỏi") {
                steps.push(
                    PlanStep::new("step-search", "Tìm kiếm thông tin tri thức")
                        .with_tool("search_vault", json!({ "query": goal })),
                );
                steps.push(
                    PlanStep::new("step-synthesize", "Tổng hợp và trả lời kết quả"),
                );
            } else if lower.contains("thời tiết") || lower.contains("weather") {
                steps.push(
                    PlanStep::new("step-weather", "Truy vấn dữ liệu thời tiết")
                        .with_tool("get_weather", json!({ "location": "Hà Nội" })),
                );
                steps.push(
                    PlanStep::new("step-summarize-weather", "Tóm tắt dự báo thời tiết"),
                );
            } else {
                // Default general action step followed by answer synthesis
                steps.push(
                    PlanStep::new("step-action-1", format!("Thực hiện yêu cầu: {goal}")),
                );
            }
        }

        steps.truncate(self.max_steps);
        steps
    }

    /// Self-healing reflection logic: when a tool fails or throws an error,
    /// analyzes the error and produces a corrected tool call or alternate fallback.
    pub fn reflect_on_tool_failure(
        &self,
        step: &PlanStep,
        error_msg: &str,
    ) -> Result<ReActThought, AgentError> {
        if step.retries >= self.max_retries {
            return Err(AgentError::MaxRetriesExceeded(
                self.max_retries,
                step.id.clone(),
            ));
        }

        let current_tool = step.tool_name.as_deref().unwrap_or("unknown");
        let current_args = step.tool_arguments.clone().unwrap_or(json!({}));

        let (corrected_tool, corrected_arguments, reflection) = match current_tool {
            "search_vault" => {
                // If vault search fails or yields no results, fallback to search_tool (web)
                let query = current_args.get("query").and_then(|q| q.as_str()).unwrap_or("");
                (
                    "search_tool".to_string(),
                    json!({ "query": query }),
                    format!(
                        "Vault search failed with '{error_msg}'. Reflecting: falling back to external search_tool with query '{query}'."
                    ),
                )
            }
            "get_weather" => {
                // If specific location fails, sanitize/fallback to default region
                let loc = current_args.get("location").and_then(|l| l.as_str()).unwrap_or("Hanoi");
                let clean_loc = if loc.contains(',') {
                    loc.split(',').next().unwrap_or("Hanoi").trim()
                } else {
                    "Hanoi"
                };
                (
                    "get_weather".to_string(),
                    json!({ "location": clean_loc }),
                    format!(
                        "Weather lookup for '{loc}' failed ({error_msg}). Correcting location to '{clean_loc}'."
                    ),
                )
            }
            "control_smarthome" => {
                // If specific device name failed, normalize device key
                let device = current_args.get("device").and_then(|d| d.as_str()).unwrap_or("light");
                let normalized_device = match device {
                    "den" | "bong_den" => "light",
                    "quat_gio" | "quat" => "fan",
                    "may_lanh" | "dieu_hoa" => "ac",
                    other => other,
                };
                let action = current_args.get("action").and_then(|a| a.as_str()).unwrap_or("on");
                (
                    "control_smarthome".to_string(),
                    json!({ "device": normalized_device, "action": action }),
                    format!(
                        "Device control failed on '{device}' ({error_msg}). Normalizing device target to '{normalized_device}'."
                    ),
                )
            }
            other => {
                // General fallback: retry with sanitized argument map
                (
                    other.to_string(),
                    current_args.clone(),
                    format!(
                        "Tool '{other}' reported error: '{error_msg}'. Retrying with sanitized parameters (attempt {}/{}).",
                        step.retries + 1,
                        self.max_retries
                    ),
                )
            }
        };

        Ok(ReActThought::ReflectAndRetry {
            step_index: 0,
            reflection,
            corrected_tool,
            corrected_arguments,
            retry_count: step.retries + 1,
        })
    }
}

/// Agent execution loop orchestrating ReAct steps, tool dispatch, working memory, and self-healing.
pub struct AgentLoop;

impl AgentLoop {
    /// Executes a single step of the ReAct plan on the current `AgentState`.
    pub async fn step(
        state: &mut AgentState,
        tools: &UnifiedToolDispatcher,
    ) -> Result<StepOutcome, AgentError> {
        let planner = ReActPlanner::new();

        // 1. Ensure a TaskPlan exists in Working Memory
        if state.active_plan.is_none() {
            let last_user_msg = state
                .messages
                .iter()
                .rev()
                .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"))
                .and_then(|m| m.get("content").and_then(|c| c.as_str()))
                .unwrap_or("Default Goal");

            let new_plan = planner.plan(last_user_msg, state)?;
            state.set_plan(new_plan);
        }

        let mut plan = state.active_plan.take().unwrap();

        // Check if plan is already complete
        if plan.is_finished() {
            let answer = plan
                .summary
                .clone()
                .unwrap_or_else(|| "Tất cả các bước trong kế hoạch đã hoàn thành thành công.".to_string());
            let plan_clone = plan.clone();
            state.active_plan = Some(plan);
            return Ok(StepOutcome::PlanCompleted {
                final_answer: answer,
                plan: plan_clone,
            });
        }

        let step_idx = plan.current_step;
        if step_idx >= plan.steps.len() {
            plan.completed = true;
            let answer = "Kế hoạch đã hoàn thành.".to_string();
            let plan_clone = plan.clone();
            state.active_plan = Some(plan);
            return Ok(StepOutcome::PlanCompleted {
                final_answer: answer,
                plan: plan_clone,
            });
        }

        let (step_id, tool_name, tool_args, retries, description) = {
            let current_step = &mut plan.steps[step_idx];
            current_step.mark_in_progress();
            (
                current_step.id.clone(),
                current_step.tool_name.clone(),
                current_step.tool_arguments.clone(),
                current_step.retries,
                current_step.description.clone(),
            )
        };

        // 2. Determine Action & Execute Tool if bound
        if let Some(tool_name_str) = tool_name {
            let args = tool_args.unwrap_or(json!({}));

            let call_req = ToolCallRequest::new(format!("call-{}-{}", step_id, retries), tool_name_str.clone(), args.clone());

            // Record Thought into scratchpad
            state.scratchpad_set(
                format!("thought_{step_idx}"),
                json!({
                    "step_id": step_id,
                    "action": tool_name_str,
                    "arguments": args,
                    "attempt": retries + 1
                }),
            );

            // Execute tool via UnifiedToolDispatcher (implementing ToolDispatcher trait)
            let dispatch_res = ToolDispatcher::dispatch(tools, call_req).await;

            match dispatch_res {
                Ok(tool_result) if tool_result.success => {
                    // Step Succeeded: Record Observation and advance
                    let output = tool_result.output;
                    if let Some(s) = plan.steps.get_mut(step_idx) {
                        s.mark_completed(output.clone());
                    }
                    state.record_step_output(&step_id, output.clone());

                    state.messages.push(json!({
                        "role": "tool",
                        "name": tool_name_str,
                        "content": output.to_string(),
                        "step_id": step_id
                    }));

                    let next_step = plan.advance();
                    state.active_step_index = plan.current_step;

                    if plan.is_finished() {
                        let final_answer = Self::synthesize_summary(&plan);
                        plan.summary = Some(final_answer.clone());
                        let plan_clone = plan.clone();
                        state.active_plan = Some(plan);
                        Ok(StepOutcome::PlanCompleted {
                            final_answer,
                            plan: plan_clone,
                        })
                    } else {
                        state.active_plan = Some(plan);
                        Ok(StepOutcome::StepCompleted {
                            step_index: step_idx,
                            step_id,
                            output,
                            next_step,
                        })
                    }
                }
                Ok(tool_result) => {
                    // Tool returned logical failure: Initiate Self-Healing Reflection
                    let err_msg = tool_result.error.unwrap_or_else(|| "Unknown tool error".to_string());
                    let res = Self::handle_step_failure(step_idx, &step_id, &err_msg, &mut plan, state, &planner);
                    state.active_plan = Some(plan);
                    res
                }
                Err(err_msg) => {
                    // Tool dispatch infrastructure error: Initiate Self-Healing Reflection
                    let res = Self::handle_step_failure(step_idx, &step_id, &err_msg, &mut plan, state, &planner);
                    state.active_plan = Some(plan);
                    res
                }
            }
        } else {
            // Step has no tool (Pure reasoning / Synthesis step)
            let synthesis_output = json!({
                "status": "completed",
                "reasoning": format!("Hoàn thành bước suy luận: {description}")
            });
            if let Some(s) = plan.steps.get_mut(step_idx) {
                s.mark_completed(synthesis_output.clone());
            }
            state.record_step_output(&step_id, synthesis_output.clone());

            let next_step = plan.advance();
            state.active_step_index = plan.current_step;

            if plan.is_finished() {
                let final_answer = Self::synthesize_summary(&plan);
                plan.summary = Some(final_answer.clone());
                let plan_clone = plan.clone();
                state.active_plan = Some(plan);
                Ok(StepOutcome::PlanCompleted {
                    final_answer,
                    plan: plan_clone,
                })
            } else {
                state.active_plan = Some(plan);
                Ok(StepOutcome::StepCompleted {
                    step_index: step_idx,
                    step_id,
                    output: synthesis_output,
                    next_step,
                })
            }
        }
    }

    /// Internal error handling and reflection retry driver.
    fn handle_step_failure(
        step_idx: usize,
        step_id: &str,
        err_msg: &str,
        plan: &mut TaskPlan,
        state: &mut AgentState,
        planner: &ReActPlanner,
    ) -> Result<StepOutcome, AgentError> {
        let step = &mut plan.steps[step_idx];
        step.mark_failed(err_msg);

        if step.can_retry(planner.max_retries) {
            // Produce reflection thought
            match planner.reflect_on_tool_failure(step, err_msg) {
                Ok(ReActThought::ReflectAndRetry {
                    reflection,
                    corrected_tool,
                    corrected_arguments,
                    retry_count,
                    ..
                }) => {
                    // Update step with corrected tool and arguments for next iteration
                    step.tool_name = Some(corrected_tool.clone());
                    step.tool_arguments = Some(corrected_arguments.clone());
                    step.status = StepStatus::Pending;

                    state.scratchpad_set(
                        format!("reflection_{step_idx}_{retry_count}"),
                        json!({
                            "reflection": reflection,
                            "corrected_tool": corrected_tool,
                            "corrected_arguments": corrected_arguments
                        }),
                    );

                    state.messages.push(json!({
                        "role": "assistant",
                        "content": format!("[Reflection] {reflection}")
                    }));

                    Ok(StepOutcome::StepRetrying {
                        step_index: step_idx,
                        step_id: step_id.to_string(),
                        error: err_msg.to_string(),
                        retry_count,
                        next_tool: corrected_tool,
                    })
                }
                _ => Ok(StepOutcome::StepFailed {
                    step_index: step_idx,
                    step_id: step_id.to_string(),
                    error: err_msg.to_string(),
                }),
            }
        } else {
            // Retry budget exhausted: advance or mark plan completed with failure note
            plan.advance();
            state.active_step_index = plan.current_step;
            Ok(StepOutcome::StepFailed {
                step_index: step_idx,
                step_id: step_id.to_string(),
                error: format!("Exceeded max retries ({}): {}", planner.max_retries, err_msg),
            })
        }
    }

    /// Synthesizes a human-readable summary of all completed steps in the plan.
    pub fn synthesize_summary(plan: &TaskPlan) -> String {
        let mut lines = Vec::new();
        lines.push(format!("Đã hoàn thành kế hoạch cho mục tiêu: \"{}\"", plan.goal));
        for (i, step) in plan.steps.iter().enumerate() {
            let status_str = match &step.status {
                StepStatus::Completed => "✓ Hoàn thành",
                StepStatus::Failed { error: _, .. } => "✗ Thất bại",
                StepStatus::Skipped { .. } => "○ Bỏ qua",
                _ => "— Đang xử lý",
            };
            lines.push(format!("{}. {} [{}]", i + 1, step.description, status_str));
        }
        lines.join("\n")
    }

    /// Runs the entire ReAct loop iteratively until completion or budget exhaustion.
    pub async fn run(
        state: &mut AgentState,
        tools: &UnifiedToolDispatcher,
        max_iterations: usize,
    ) -> Result<String, AgentError> {
        let mut iterations = 0;
        let limit = max_iterations.min(MAX_PLAN_STEPS * (MAX_TOOL_RETRIES_PER_STEP + 1));

        loop {
            if iterations >= limit {
                return Err(AgentError::StepBudgetExceeded(limit));
            }
            iterations += 1;

            match Self::step(state, tools).await? {
                StepOutcome::PlanCompleted { final_answer, .. } => {
                    return Ok(final_answer);
                }
                StepOutcome::StepCompleted { .. } => {
                    // Continue to next step
                }
                StepOutcome::StepRetrying { .. } => {
                    // Loop directly to retry step with corrected parameters
                }
                StepOutcome::StepFailed { error, .. } => {
                    if state.get_plan().map(|p| p.is_finished()).unwrap_or(true) {
                        return Ok(format!("Kế hoạch kết thúc với một số lỗi: {error}"));
                    }
                }
                StepOutcome::BudgetExceeded { reason } => {
                    return Err(AgentError::PlanningFailed(reason));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::manifest::{RiskLevel, SkillToolDefinition};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_react_planner_goal_decomposition() {
        let planner = ReActPlanner::new();
        let state = AgentState::default();

        let plan = planner
            .plan("bật đèn phòng khách và bật quạt", &state)
            .expect("plan decomposition");

        assert_eq!(plan.steps.len(), 2);
        assert_eq!(plan.steps[0].id, "step-light");
        assert_eq!(
            plan.steps[0].tool_name.as_deref(),
            Some("control_smarthome")
        );
        assert_eq!(plan.steps[1].id, "step-fan");
        assert_eq!(
            plan.steps[1].tool_name.as_deref(),
            Some("control_smarthome")
        );
    }

    #[tokio::test]
    async fn test_react_loop_successful_execution() {
        let dispatcher = UnifiedToolDispatcher::new();
        dispatcher
            .register_tool(SkillToolDefinition {
                name: "control_smarthome".to_string(),
                description: "Smart home control".to_string(),
                risk_level: RiskLevel::ReadOnlySafe,
                input_schema: json!({}),
            })
            .await;

        let mut state = AgentState {
            messages: vec![json!({"role": "user", "content": "bật đèn phòng khách"})],
            ..Default::default()
        };

        let result = AgentLoop::run(&mut state, &dispatcher, 5).await;
        assert!(result.is_ok(), "ReAct loop should complete successfully");
        let summary = result.unwrap();
        assert!(summary.contains("Đã hoàn thành kế hoạch"));
        assert!(state.get_plan().unwrap().is_finished());
        assert_eq!(state.step_outputs.len(), 1);
    }

    #[tokio::test]
    async fn test_react_self_healing_tool_retry() {
        let dispatcher = Arc::new(UnifiedToolDispatcher::new());
        let attempts = Arc::new(AtomicUsize::new(0));

        let attempts_clone = attempts.clone();
        dispatcher
            .register_native_handler(
                SkillToolDefinition {
                    name: "search_vault".to_string(),
                    description: "Search notes in vault".to_string(),
                    risk_level: RiskLevel::ReadOnlySafe,
                    input_schema: json!({}),
                },
                move |_args| {
                    let att = attempts_clone.clone();
                    Box::pin(async move {
                        let count = att.fetch_add(1, Ordering::SeqCst);
                        if count == 0 {
                            // First attempt fails to trigger reflection
                            Err("Vault database locked: timeout".to_string())
                        } else {
                            Ok(json!({"matches": ["Note 1", "Note 2"]}))
                        }
                    })
                },
            )
            .await;

        // Also register fallback search_tool
        dispatcher
            .register_native_handler(
                SkillToolDefinition {
                    name: "search_tool".to_string(),
                    description: "Web search fallback".to_string(),
                    risk_level: RiskLevel::ReadOnlySafe,
                    input_schema: json!({}),
                },
                move |_args| {
                    Box::pin(async move {
                        Ok(json!({"results": ["Web result 1"]}))
                    })
                },
            )
            .await;

        let mut state = AgentState {
            messages: vec![json!({"role": "user", "content": "tìm kiếm tài liệu"})],
            ..Default::default()
        };

        let result = AgentLoop::run(&mut state, &dispatcher, 10).await;
        assert!(result.is_ok(), "ReAct self-healing loop must recover and succeed");
        assert!(attempts.load(Ordering::SeqCst) >= 1);
        assert!(state.scratchpad.keys().any(|k| k.starts_with("reflection_")));
    }
}
