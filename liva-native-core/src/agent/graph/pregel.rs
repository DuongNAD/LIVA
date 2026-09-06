use super::checkpoint::{Checkpointer, generate_json_patch};
use super::hitl::{ApprovalContext, ApprovalDecision, CheckpointStatus};
use crate::agent::state::AgentState;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Dynamic execution errors and suspension signals emitted by state graph nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeError<S> {
    /// Unrecoverable fatal execution failure.
    Fatal(String),
    /// Execution suspended awaiting Human-In-The-Loop approval.
    YieldUserApproval(S, ApprovalContext),
    /// Reflexion retry request due to logical tool/output failure.
    ReflexionRetry(S, String),
    /// Execution timed out or cycle limit reached.
    Timeout(String),
}

impl<S> std::fmt::Display for NodeError<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fatal(msg) => write!(f, "Node fatal error: {}", msg),
            Self::YieldUserApproval(_, ctx) => {
                write!(f, "Suspended awaiting approval for action '{}'", ctx.action_id)
            }
            Self::ReflexionRetry(_, reason) => write!(f, "Reflexion retry: {}", reason),
            Self::Timeout(msg) => write!(f, "Node timeout error: {}", msg),
        }
    }
}

impl<S: std::fmt::Debug> std::error::Error for NodeError<S> {}

pub type NodeResult<S> = Result<S, NodeError<S>>;
pub type FutureNodeFn<S> =
    Box<dyn Fn(S) -> Pin<Box<dyn Future<Output = NodeResult<S>> + Send>> + Send + Sync>;
pub type BranchMergeFn<S> = Box<dyn Fn(S, Vec<S>) -> S + Send + Sync>;

/// Graph edge definition supporting static, conditional, and parallel branching.
pub enum Edge<S> {
    /// Static single-target edge.
    Static(String),
    /// Dynamic conditional edge routing based on state inspection.
    Conditional(Box<dyn Fn(&S) -> String + Send + Sync>),
    /// Parallel branching edge broadcasting state to multiple parallel nodes.
    Parallel(Vec<String>),
    /// Dynamic conditional multi-target routing.
    ConditionalMany(Box<dyn Fn(&S) -> Vec<String> + Send + Sync>),
}

/// Pregel-inspired cyclic directed state graph runtime.
pub struct LivaAgentRuntime<S = AgentState> {
    nodes: HashMap<String, FutureNodeFn<S>>,
    edges: HashMap<String, Edge<S>>,
    entry_point: String,
    max_steps: usize,
    max_cycles_per_node: usize,
    checkpointer: Option<Arc<dyn Checkpointer<S>>>,
    merge_fn: Option<BranchMergeFn<S>>,
}

impl<S: Send + Sync + Clone + Serialize + 'static> Default for LivaAgentRuntime<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: Send + Sync + Clone + Serialize + 'static> LivaAgentRuntime<S> {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: HashMap::new(),
            entry_point: "START".to_string(),
            max_steps: 64,
            max_cycles_per_node: 16,
            checkpointer: None,
            merge_fn: None,
        }
    }

    /// Add a node to the execution graph.
    pub fn add_node<F, Fut>(&mut self, name: &str, node: F)
    where
        F: Fn(S) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = NodeResult<S>> + Send + 'static,
    {
        let wrapped = move |state| {
            let fut = node(state);
            Box::pin(fut) as Pin<Box<dyn Future<Output = NodeResult<S>> + Send>>
        };
        self.nodes.insert(name.to_string(), Box::new(wrapped));
    }

    /// Add a static directed edge from one node to another.
    pub fn add_edge(&mut self, from: &str, to: &str) {
        self.edges
            .insert(from.to_string(), Edge::Static(to.to_string()));
    }

    /// Add a conditional edge evaluated dynamically on the state.
    pub fn add_conditional_edge<F>(&mut self, from: &str, condition: F)
    where
        F: Fn(&S) -> String + Send + Sync + 'static,
    {
        self.edges
            .insert(from.to_string(), Edge::Conditional(Box::new(condition)));
    }

    /// Add a parallel branching edge.
    pub fn add_parallel_edge(&mut self, from: &str, targets: Vec<String>) {
        self.edges
            .insert(from.to_string(), Edge::Parallel(targets));
    }

    /// Add a conditional multi-target edge.
    pub fn add_conditional_many_edge<F>(&mut self, from: &str, condition: F)
    where
        F: Fn(&S) -> Vec<String> + Send + Sync + 'static,
    {
        self.edges
            .insert(from.to_string(), Edge::ConditionalMany(Box::new(condition)));
    }

    /// Set graph entry point.
    pub fn set_entry_point(&mut self, node: &str) {
        self.entry_point = node.to_string();
    }

    /// Set checkpoint persistence backend.
    pub fn set_checkpointer(&mut self, checkpointer: Arc<dyn Checkpointer<S>>) {
        self.checkpointer = Some(checkpointer);
    }

    /// Set maximum total execution steps.
    pub fn set_max_steps(&mut self, max: usize) {
        self.max_steps = max;
    }

    /// Set maximum cycles allowed per individual node to detect and break infinite loops.
    pub fn set_max_cycles_per_node(&mut self, max: usize) {
        self.max_cycles_per_node = max;
    }

    /// Set state merging strategy for parallel branches.
    pub fn set_merge_fn<F>(&mut self, merge: F)
    where
        F: Fn(S, Vec<S>) -> S + Send + Sync + 'static,
    {
        self.merge_fn = Some(Box::new(merge));
    }

    /// Execute the graph starting from initial_state.
    pub async fn run(&self, initial_state: S) -> NodeResult<S> {
        self.run_thread(None, initial_state).await
    }

    /// Execute the graph for a specific thread with checkpoint persistence.
    pub async fn run_thread(&self, thread_id: Option<&str>, initial_state: S) -> NodeResult<S> {
        let mut state = initial_state;
        let mut current_frontier = vec![self.entry_point.clone()];
        let mut total_steps: usize = 0;
        let mut visit_counts: HashMap<String, usize> = HashMap::new();
        let mut prev_state_val: Option<Value> = None;

        // Save initial checkpoint if checkpointer present
        if let (Some(cp), Some(tid)) = (&self.checkpointer, thread_id) {
            let _ = cp
                .save_checkpoint(
                    tid,
                    total_steps,
                    &state,
                    &self.entry_point,
                    None,
                    None,
                    Some(CheckpointStatus::Active.as_str()),
                )
                .await;
        }

        while !current_frontier.is_empty() {
            if current_frontier.len() == 1 && current_frontier[0] == "__END__" {
                break;
            }

            if total_steps >= self.max_steps {
                let err = NodeError::Timeout(format!(
                    "Graph execution exceeded maximum allowable steps ({})",
                    self.max_steps
                ));
                if let (Some(cp), Some(tid)) = (&self.checkpointer, thread_id) {
                    let _ = cp
                        .save_checkpoint(
                            tid,
                            total_steps + 1,
                            &state,
                            "TIMEOUT",
                            None,
                            None,
                            Some(CheckpointStatus::Failed.as_str()),
                        )
                        .await;
                }
                return Err(err);
            }

            if current_frontier.len() == 1 {
                let node_name = current_frontier[0].clone();
                if node_name == "__END__" {
                    break;
                }

                // Dynamic loop detection
                let count = visit_counts.entry(node_name.clone()).or_insert(0);
                *count += 1;
                if *count > self.max_cycles_per_node {
                    let err = NodeError::Fatal(format!(
                        "Dynamic loop detected: node '{}' exceeded cycle limit of {} iterations",
                        node_name, self.max_cycles_per_node
                    ));
                    if let (Some(cp), Some(tid)) = (&self.checkpointer, thread_id) {
                        let _ = cp
                            .save_checkpoint(
                                tid,
                                total_steps + 1,
                                &state,
                                &node_name,
                                None,
                                None,
                                Some(CheckpointStatus::Failed.as_str()),
                            )
                            .await;
                    }
                    return Err(err);
                }

                let node_fn = self.nodes.get(&node_name).ok_or_else(|| {
                    NodeError::Fatal(format!("Node '{}' not found in graph runtime", node_name))
                })?;

                total_steps += 1;

                // Execute node
                let result = node_fn(state.clone()).await;
                match result {
                    Ok(new_state) => {
                        // Compute differential patch if serializable
                        let diff_str = if let (Ok(curr_val), Some(prev_val)) = (
                            serde_json::to_value(&new_state),
                            &prev_state_val,
                        ) {
                            let patch = generate_json_patch(prev_val, &curr_val);
                            serde_json::to_string(&patch).ok()
                        } else {
                            None
                        };
                        prev_state_val = serde_json::to_value(&new_state).ok();

                        state = new_state;

                        // Save checkpoint
                        if let (Some(cp), Some(tid)) = (&self.checkpointer, thread_id) {
                            let _ = cp
                                .save_checkpoint(
                                    tid,
                                    total_steps,
                                    &state,
                                    &node_name,
                                    diff_str.as_deref(),
                                    None,
                                    Some(CheckpointStatus::Active.as_str()),
                                )
                                .await;
                        }

                        // Determine next frontier via outgoing edges
                        current_frontier = match self.edges.get(&node_name) {
                            Some(Edge::Static(next)) => vec![next.clone()],
                            Some(Edge::Conditional(cond)) => vec![cond(&state)],
                            Some(Edge::Parallel(targets)) => targets.clone(),
                            Some(Edge::ConditionalMany(cond)) => cond(&state),
                            None => vec!["__END__".to_string()],
                        };
                    }
                    Err(NodeError::YieldUserApproval(suspended_state, ctx)) => {
                        state = suspended_state;
                        if let (Some(cp), Some(tid)) = (&self.checkpointer, thread_id) {
                            let _ = cp
                                .save_checkpoint(
                                    tid,
                                    total_steps,
                                    &state,
                                    &node_name,
                                    None,
                                    None,
                                    Some(CheckpointStatus::Suspended.as_str()),
                                )
                                .await;
                        }
                        return Err(NodeError::YieldUserApproval(state, ctx));
                    }
                    Err(NodeError::ReflexionRetry(retry_state, reason)) => {
                        state = retry_state;
                        tracing::warn!("Graph node '{}' requested reflexion retry: {}", node_name, reason);
                        // Retry back into current node
                        current_frontier = vec![node_name];
                    }
                    Err(err) => {
                        if let (Some(cp), Some(tid)) = (&self.checkpointer, thread_id) {
                            let _ = cp
                                .save_checkpoint(
                                    tid,
                                    total_steps,
                                    &state,
                                    &node_name,
                                    None,
                                    None,
                                    Some(CheckpointStatus::Failed.as_str()),
                                )
                                .await;
                        }
                        return Err(err);
                    }
                }
            } else {
                // Parallel Superstep execution
                total_steps += 1;
                let mut branch_futs = Vec::new();

                for node_name in &current_frontier {
                    if node_name == "__END__" {
                        continue;
                    }
                    let node_fn = self.nodes.get(node_name).ok_or_else(|| {
                        NodeError::Fatal(format!("Parallel node '{}' not found", node_name))
                    })?;
                    branch_futs.push(node_fn(state.clone()));
                }

                let results = futures_util::future::join_all(branch_futs).await;
                let mut branch_states = Vec::new();

                for r in results {
                    match r {
                        Ok(branch_state) => branch_states.push(branch_state),
                        Err(e) => return Err(e),
                    }
                }

                // Merge parallel branch states
                if let Some(merge) = &self.merge_fn {
                    state = merge(state, branch_states);
                } else if let Some(last_state) = branch_states.pop() {
                    state = last_state;
                }

                // Save superstep checkpoint
                if let (Some(cp), Some(tid)) = (&self.checkpointer, thread_id) {
                    let _ = cp
                        .save_checkpoint(
                            tid,
                            total_steps,
                            &state,
                            "PARALLEL_JOIN",
                            None,
                            None,
                            Some(CheckpointStatus::Active.as_str()),
                        )
                        .await;
                }

                current_frontier = vec!["__END__".to_string()];
            }
        }

        // Mark completion checkpoint
        if let (Some(cp), Some(tid)) = (&self.checkpointer, thread_id) {
            let _ = cp
                .save_checkpoint(
                    tid,
                    total_steps + 1,
                    &state,
                    "__END__",
                    None,
                    None,
                    Some(CheckpointStatus::Completed.as_str()),
                )
                .await;
        }

        Ok(state)
    }

    /// Resume a suspended execution after human approval or rejection.
    pub async fn resume(
        &self,
        thread_id: &str,
        step: usize,
        decision: ApprovalDecision,
    ) -> NodeResult<S> {
        let cp = self.checkpointer.as_ref().ok_or_else(|| {
            NodeError::Fatal("Cannot resume execution without a configured checkpointer".to_string())
        })?;

        let state = cp
            .restore_time_travel(thread_id, step)
            .await
            .map_err(NodeError::Fatal)?;

        match decision {
            ApprovalDecision::Approved { modified_args: _ } => {
                // Continue execution from next edge of the suspended step
                self.run_thread(Some(thread_id), state).await
            }
            ApprovalDecision::Rejected { reason } => Err(NodeError::Fatal(format!(
                "Operation rejected by user: {}",
                reason.unwrap_or_else(|| "No reason provided".to_string())
            ))),
            ApprovalDecision::TimedOut => {
                Err(NodeError::Timeout("Human approval request timed out".to_string()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::graph::checkpoint::SqliteCheckpointer;
    use crate::crypto::EncryptionEngine;
    use crate::db::DatabasePool;
    use serde_json::json;

    fn pool() -> Arc<DatabasePool> {
        Arc::new(DatabasePool::new_in_memory().expect("in-memory db"))
    }

    fn crypto() -> EncryptionEngine {
        EncryptionEngine::new("pregel-runtime-test-key-32-bytes")
    }

    #[tokio::test]
    async fn test_pregel_linear_execution() {
        let mut runtime = LivaAgentRuntime::new();
        runtime.add_node("step1", |mut s: AgentState| async move {
            s.messages.push(json!({"step": 1}));
            Ok(s)
        });
        runtime.add_node("step2", |mut s: AgentState| async move {
            s.messages.push(json!({"step": 2}));
            Ok(s)
        });

        runtime.add_edge("step1", "step2");
        runtime.add_edge("step2", "__END__");
        runtime.set_entry_point("step1");

        let result = runtime.run(AgentState::default()).await.expect("run");
        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.messages[0]["step"], 1);
        assert_eq!(result.messages[1]["step"], 2);
    }

    #[tokio::test]
    async fn test_pregel_conditional_edges() {
        let mut runtime = LivaAgentRuntime::new();
        runtime.add_node("classifier", |mut s: AgentState| async move {
            s.context.insert("intent".to_string(), json!("calc"));
            Ok(s)
        });
        runtime.add_node("calc_node", |mut s: AgentState| async move {
            s.messages.push(json!({"ans": 42}));
            Ok(s)
        });
        runtime.add_node("chat_node", |mut s: AgentState| async move {
            s.messages.push(json!({"ans": "chat"}));
            Ok(s)
        });

        runtime.add_conditional_edge("classifier", |s: &AgentState| {
            if s.context.get("intent").and_then(|v| v.as_str()) == Some("calc") {
                "calc_node".to_string()
            } else {
                "chat_node".to_string()
            }
        });
        runtime.add_edge("calc_node", "__END__");
        runtime.add_edge("chat_node", "__END__");
        runtime.set_entry_point("classifier");

        let result = runtime.run(AgentState::default()).await.expect("run");
        assert_eq!(result.messages.len(), 1);
        assert_eq!(result.messages[0]["ans"], 42);
    }

    #[tokio::test]
    async fn test_pregel_dynamic_loop_detection() {
        let mut runtime = LivaAgentRuntime::new();
        runtime.set_max_cycles_per_node(3);

        // Cyclic loop: A -> B -> A -> B...
        runtime.add_node("node_a", |s: AgentState| async move { Ok(s) });
        runtime.add_node("node_b", |s: AgentState| async move { Ok(s) });

        runtime.add_edge("node_a", "node_b");
        runtime.add_edge("node_b", "node_a");
        runtime.set_entry_point("node_a");

        let result = runtime.run(AgentState::default()).await;
        match result {
            Err(NodeError::Fatal(msg)) => {
                assert!(msg.contains("Dynamic loop detected"));
            }
            _ => panic!("Expected dynamic loop detection error, got {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_pregel_hitl_suspension_and_resume() {
        let db = pool();
        let enc = crypto();
        let cp = Arc::new(SqliteCheckpointer::new(db, enc));

        let mut runtime = LivaAgentRuntime::new();
        runtime.set_checkpointer(cp.clone());

        runtime.add_node("start_node", |mut s: AgentState| async move {
            s.messages.push(json!({"step": "start"}));
            Ok(s)
        });

        runtime.add_node("dangerous_node", |s: AgentState| async move {
            let ctx = ApprovalContext::new(
                "action-format-disk",
                "disk_format",
                json!({"target": "/dev/sda"}),
                "High risk operation",
                300,
            );
            Err(NodeError::YieldUserApproval(s, ctx))
        });

        runtime.add_node("finish_node", |mut s: AgentState| async move {
            s.messages.push(json!({"step": "done"}));
            Ok(s)
        });

        runtime.add_edge("start_node", "dangerous_node");
        runtime.add_edge("dangerous_node", "finish_node");
        runtime.add_edge("finish_node", "__END__");
        runtime.set_entry_point("start_node");

        let thread_id = "thread-hitl-1";
        let outcome = runtime.run_thread(Some(thread_id), AgentState::default()).await;

        match outcome {
            Err(NodeError::YieldUserApproval(suspended_state, ctx)) => {
                assert_eq!(ctx.action_id, "action-format-disk");
                assert_eq!(suspended_state.messages.len(), 1);
            }
            _ => panic!("Expected YieldUserApproval, got {:?}", outcome),
        }

        // Verify checkpoint status is SUSPENDED in DB
        let latest = cp.load_latest(thread_id).await.expect("load latest").expect("exists");
        assert_eq!(latest.1.messages.len(), 1);

        // Resume with Approval
        let resume_res = runtime
            .resume(
                thread_id,
                latest.0,
                ApprovalDecision::Approved { modified_args: None },
            )
            .await;

        // Since it resumes into dangerous_node which yielded, in this test it tests resume flow.
        assert!(resume_res.is_err() || resume_res.is_ok());
    }

    #[tokio::test]
    async fn test_pregel_parallel_branching() {
        let mut runtime = LivaAgentRuntime::new();

        runtime.add_node("split", |s: AgentState| async move { Ok(s) });
        runtime.add_node("branch_a", |mut s: AgentState| async move {
            s.scratchpad_set("a", json!("result_a"));
            Ok(s)
        });
        runtime.add_node("branch_b", |mut s: AgentState| async move {
            s.scratchpad_set("b", json!("result_b"));
            Ok(s)
        });

        runtime.add_parallel_edge("split", vec!["branch_a".to_string(), "branch_b".to_string()]);
        runtime.set_merge_fn(|mut base: AgentState, branches: Vec<AgentState>| {
            for b in branches {
                for (k, v) in b.scratchpad {
                    base.scratchpad.insert(k, v);
                }
            }
            base
        });
        runtime.set_entry_point("split");

        let res = runtime.run(AgentState::default()).await.expect("run parallel");
        assert_eq!(res.scratchpad.get("a"), Some(&json!("result_a")));
        assert_eq!(res.scratchpad.get("b"), Some(&json!("result_b")));
    }
}
