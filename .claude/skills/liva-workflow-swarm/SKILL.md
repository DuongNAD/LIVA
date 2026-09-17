---
name: liva-workflow-swarm
description: Coordinate multi-agent swarms using Directed Acyclic Graphs (DAG), voting consensus protocols, Dead Letter Queue (DLQ) task isolation, and Human-in-the-Loop (HITL) checkpoints. Use when decomposing complex multi-step objectives into parallel workstreams, convening agent debate panels, managing task retries, or synchronizing swarm states.
---

# LIVA Workflow Swarm

## Workflow

1. **Objective Ingestion & Directed Acyclic Graph (DAG) Synthesis**:
   - Decompose high-level mission into discrete, atomic, verifiable task nodes.
   - Model task dependencies as a DAG ($G = (V, E)$).
   - Execute topological sorting (Kahn's algorithm) to guarantee acyclicity, identify parallel branch opportunities (fan-out), and set synchronization join gates (fan-in).

2. **Specialized Subagent Archetype Allocation**:
   - Assign tasks dynamically to dedicated agent archetypes:
     - **Planner**: DAG synthesis, dependency scheduling, and resource allocation.
     - **Implementer**: Code synthesis, configuration changes, and data transforms.
     - **Reviewer / QA**: Static analysis, schema verification, and unit test execution.
     - **Challenger**: Adversarial probing, negative test cases, and boundary fuzzing.
     - **Auditor**: Security attestation, Git boundary compliance, and policy checks.
   - Bind scoped memory contexts and tool execution permissions to each subagent.

3. **Asynchronous Dispatch & Message Correlation**:
   - Dispatch unblocked tasks concurrently using Tokio mpsc channels and `AgentDispatcher`.
   - Track every inter-agent message with unique `message_id`, `trace_id`, and `correlation_id`.
   - Propagate upstream task outputs downstream as structured JSON context.

4. **Multi-Agent Deliberation & Voting Consensus Gates**:
   - For critical decision points (architectural migrations, schema breaking changes, security approvals):
     - Convene a multi-agent voting panel (minimum quorum: 3 agents).
     - Require supermajority approval (>= 66%).
     - If consensus fails or divergence exceeds threshold, initiate structured multi-round debate.

5. **Fault Isolation & Dead Letter Queue (DLQ) Recovery**:
   - Handle transient subagent failures with exponential backoff (up to 3 retries).
   - If retries fail, isolate the faulty task node into the Dead Letter Queue (DLQ) and generate an automated diagnostic incident report without crashing unrelated parallel branches.

6. **Human-in-the-Loop (HITL) Checkpoint Enforcement**:
   - Enforce mandatory operator approval checkpoints for high-impact actions:
     - Production database schema migrations, cloud service deletions, credential rotations.
   - Suspend workflow execution, display interactive diff and risk report, and resume only upon receiving a cryptographically valid confirmation token.

7. **Swarm State Synchronization & Obsidian Archival**:
   - Record workflow execution logs and DAG task state transitions in SQLite WAL (`orchestrator_workflows`, `orchestrator_tasks`).
   - Persist final workflow execution dossiers into `teamwork_projects/obsidian_llm_wiki/vault/Knowledge/Swarm - <Workflow_Title>.md` via `write_markdown`.
   - Adhere strictly to the Obsidian frontmatter standard (`title`, `tags: [liva/knowledge, liva/swarm, workflow/orchestration]`, `author: "codex"`, `last_update`).

## Platform Constraints

- **Execution Mode**: Supervised Swarm Orchestration. Subagent spawning and message routing run automatically. High-risk mutating actions enforce strict HITL approval gates (`ProposeOnly`).
- **Graph Limits**: Maximum 30 tasks per workflow graph; maximum 10 concurrent active subagents; graph acyclicity strictly verified prior to dispatch.
- **Fail-Closed Principle**: Unhandled subagent panics or security violations immediately halt dependent graph branches.

## Stop Conditions

Stop and report immediately when:
- The task decomposition graph contains circular dependencies preventing topological ordering.
- A mandatory HITL checkpoint is rejected by the operator or times out.
- A critical-path task fails and exhausts all DLQ retries without a fallback execution path.
- An unauthorized privilege escalation attempt is detected from a sandboxed subagent.
