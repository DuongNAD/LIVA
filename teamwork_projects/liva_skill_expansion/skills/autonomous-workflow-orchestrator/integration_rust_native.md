# Rust Native Core Integration: autonomous-workflow-orchestrator

## 1. Native Engine Architecture Alignment

### 1.1 Module Domain & Routing
- **Native Implementation**: `liva-native-core/src/agent/`, `liva-native-core/src/commands/orchestrator.rs`, `liva-native-core/src/authorization.rs`, `liva-native-core/src/llm/tool_calling.rs`.
- **Command Routing Matrix**:
  - `orchestrator:decompose_plan`: Deconstructs goals into DAGs and computes topological execution waves. Gated under `ExecPolicy::Auto`.
  - `orchestrator:dispatch_task`: Spawns subagents with specific roles and scoped capabilities. Gated under `ExecPolicy::Auto`.
  - `orchestrator:vote_consensus`: Evaluates quorum and supermajority decisions across subagents. Gated under `ExecPolicy::Auto`.
  - `orchestrator:hitl_checkpoint`: Suspends workflow and presents approval prompt to operator. Gated under `ExecPolicy::ProposeOnly`.
  - `write_markdown`: Persists execution trace and Mermaid graphs into `vault/Knowledge/Orchestration - <Title>.md`. Gated under `ExecPolicy::ProposeOnly`.

### 1.2 Execution Policy (`ExecPolicy`)
```rust
// In liva-native-core/src/llm/tool_calling.rs
pub fn for_tool(server: &str, name: &str) -> Self {
    match (server, name) {
        ("orchestrator", "decompose_plan") | ("orchestrator", "dispatch_task") | ("orchestrator", "vote_consensus") => Self::Auto,
        ("orchestrator", "hitl_checkpoint") => Self::ProposeOnly,
        ("obsidian", "write_markdown") => Self::ProposeOnly,
        _ => Self::ProposeOnly,
    }
}
```

---

## 2. Tool Schema Mapping & Compact Prompt Rendering

### 2.1 Compact Tool Signatures
```
[1] orchestrator:decompose_plan: Decompose high-level goal into validated DAG task waves
   tham số (* = bắt buộc): goal* (string), max_subtasks (integer)
[2] orchestrator:dispatch_task: Dispatch an isolated sub-task to a specialized subagent
   tham số (* = bắt buộc): task_id* (string), role* (string: implementer|reviewer|challenger|auditor), prompt* (string), context_artifacts (array)
[3] orchestrator:vote_consensus: Evaluate consensus votes for a critical workflow decision
   tham số (* = bắt buộc): gate_id* (string), votes_json* (string), required_majority (number)
[4] orchestrator:hitl_checkpoint: Request explicit human-in-the-loop authorization
   tham số (* = bắt buộc): checkpoint_id* (string), action_summary* (string), diff_payload (string)
```

### 2.2 Input Schema & Validation Matrix
```json
{
  "orchestrator:decompose_plan": {
    "type": "object",
    "properties": {
      "goal": { "type": "string", "minLength": 5, "description": "High level mission description" },
      "max_subtasks": { "type": "integer", "minimum": 1, "maximum": 25, "default": 10 }
    },
    "required": ["goal"]
  },
  "orchestrator:dispatch_task": {
    "type": "object",
    "properties": {
      "task_id": { "type": "string", "minLength": 1 },
      "role": { "type": "string", "enum": ["implementer", "reviewer", "challenger", "auditor"] },
      "prompt": { "type": "string", "minLength": 5 },
      "context_artifacts": { "type": "array", "items": { "type": "string" } }
    },
    "required": ["task_id", "role", "prompt"]
  },
  "orchestrator:vote_consensus": {
    "type": "object",
    "properties": {
      "gate_id": { "type": "string", "minLength": 1 },
      "votes_json": { "type": "string", "minLength": 2, "description": "JSON map of agent votes" },
      "required_majority": { "type": "number", "minimum": 0.5, "maximum": 1.0, "default": 0.66 }
    },
    "required": ["gate_id", "votes_json"]
  },
  "orchestrator:hitl_checkpoint": {
    "type": "object",
    "properties": {
      "checkpoint_id": { "type": "string", "minLength": 1 },
      "action_summary": { "type": "string", "minLength": 5 },
      "diff_payload": { "type": "string" }
    },
    "required": ["checkpoint_id", "action_summary"]
  }
}
```

---

## 3. DAG Scheduler & Consensus Engine in Rust

### 3.1 Topological Sort & Cycle Detection (Kahn's Algorithm)
```rust
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone)]
pub struct TaskNode {
    pub id: String,
    pub dependencies: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DagValidation {
    Valid(Vec<Vec<String>>), // Waves of parallel task IDs
    CycleDetected(Vec<String>),
}

pub fn schedule_dag(tasks: &[TaskNode]) -> DagValidation {
    let mut in_degree: HashMap<String, usize> = HashMap::new();
    let mut adj_list: HashMap<String, Vec<String>> = HashMap::new();
    let all_nodes: HashSet<String> = tasks.iter().map(|t| t.id.clone()).collect();

    for task in tasks {
        in_degree.entry(task.id.clone()).or_insert(0);
        for dep in &task.dependencies {
            adj_list.entry(dep.clone()).or_default().push(task.id.clone());
            *in_degree.entry(task.id.clone()).or_insert(0) += 1;
        }
    }

    let mut current_wave: Vec<String> = in_degree
        .iter()
        .filter(|&(_, &deg)| deg == 0)
        .map(|(id, _)| id.clone())
        .collect();

    let mut waves: Vec<Vec<String>> = Vec::new();
    let mut processed_count = 0;

    while !current_wave.is_empty() {
        processed_count += current_wave.len();
        let mut next_wave: Vec<String> = Vec::new();

        for node in &current_wave {
            if let Some(neighbors) = adj_list.get(node) {
                for neighbor in neighbors {
                    let deg = in_degree.get_mut(neighbor).unwrap();
                    *deg -= 1;
                    if *deg == 0 {
                        next_wave.push(neighbor.clone());
                    }
                }
            }
        }

        waves.push(current_wave);
        current_wave = next_wave;
    }

    if processed_count == all_nodes.len() {
        DagValidation::Valid(waves)
    } else {
        DagValidation::CycleDetected(vec!["Cycle detected in task dependencies".into()])
    }
}
```

### 3.2 Multi-Agent Consensus Voting Logic
```rust
#[derive(Debug, PartialEq, Eq)]
pub enum ConsensusOutcome {
    Approved,
    Rejected,
    NoQuorum,
}

pub fn evaluate_consensus(
    votes: &HashMap<String, String>,
    min_quorum: usize,
    supermajority: f64,
) -> ConsensusOutcome {
    if votes.len() < min_quorum {
        return ConsensusOutcome::NoQuorum;
    }

    let approve_count = votes.values().filter(|&v| v == "APPROVE").count();
    let ratio = approve_count as f64 / votes.len() as f64;

    if ratio >= supermajority {
        ConsensusOutcome::Approved
    } else {
        ConsensusOutcome::Rejected
    }
}
```

---

## 4. State Persistence & SQLite WAL Schema

```sql
-- Workflow instance tracking
CREATE TABLE IF NOT EXISTS orchestrator_workflows (
    workflow_id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    status TEXT NOT NULL, -- PENDING, RUNNING, COMPLETED, FAILED
    total_tasks INTEGER NOT NULL,
    completed_tasks INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Sub-task execution ledger
CREATE TABLE IF NOT EXISTS orchestrator_tasks (
    task_id TEXT PRIMARY KEY,
    workflow_id TEXT NOT NULL REFERENCES orchestrator_workflows(workflow_id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    status TEXT NOT NULL, -- PENDING, SCHEDULED, RUNNING, COMPLETED, FAILED, BLOCKED
    retry_count INTEGER NOT NULL DEFAULT 0,
    output_summary TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Consensus decision records
CREATE TABLE IF NOT EXISTS orchestrator_consensus_votes (
    gate_id TEXT PRIMARY KEY,
    workflow_id TEXT NOT NULL REFERENCES orchestrator_workflows(workflow_id) ON DELETE CASCADE,
    quorum_reached INTEGER NOT NULL,
    outcome TEXT NOT NULL, -- APPROVED, REJECTED, NO_QUORUM
    votes_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

---

## 5. Verification Checklist & Unit Test Scenarios

### 5.1 Verification Checklist
- [x] Frontmatter in `SKILL.md` conforms strictly to `name` and `description` only.
- [x] Frontmatter in generated vault notes conforms strictly to `title`, `tags`, `author`, `last_update`.
- [x] DAG scheduler computes parallel waves correctly and detects dependency cycles.
- [x] Consensus engine enforces minimum quorum and supermajority thresholds.
- [x] HITL checkpoints suspend execution and require explicit operator consent.

### 5.2 Unit Test Scenarios

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dag_scheduler_linear_waves() {
        let tasks = vec![
            TaskNode { id: "t1".into(), dependencies: vec![] },
            TaskNode { id: "t2".into(), dependencies: vec!["t1".into()] },
            TaskNode { id: "t3".into(), dependencies: vec!["t2".into()] },
        ];
        let result = schedule_dag(&tasks);
        match result {
            DagValidation::Valid(waves) => {
                assert_eq!(waves.len(), 3);
                assert_eq!(waves[0], vec!["t1"]);
                assert_eq!(waves[1], vec!["t2"]);
                assert_eq!(waves[2], vec!["t3"]);
            }
            DagValidation::CycleDetected(_) => panic!("Expected valid DAG"),
        }
    }

    #[test]
    fn test_dag_scheduler_cycle_detection() {
        let tasks = vec![
            TaskNode { id: "t1".into(), dependencies: vec!["t2".into()] },
            TaskNode { id: "t2".into(), dependencies: vec!["t1".into()] },
        ];
        let result = schedule_dag(&tasks);
        assert!(matches!(result, DagValidation::CycleDetected(_)));
    }

    #[test]
    fn test_consensus_voting_quorum_and_supermajority() {
        let mut votes = HashMap::new();
        votes.insert("agent1".into(), "APPROVE".into());
        votes.insert("agent2".into(), "APPROVE".into());
        votes.insert("agent3".into(), "REJECT".into());

        assert_eq!(
            evaluate_consensus(&votes, 3, 0.66),
            ConsensusOutcome::Approved
        );
        assert_eq!(
            evaluate_consensus(&votes, 4, 0.66),
            ConsensusOutcome::NoQuorum
        );
    }
}
```
