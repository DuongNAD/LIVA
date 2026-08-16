# LIVA Phase 2 Skill Pack — Technical Architecture Specification & Native Core Integration Blueprint

**Document Version**: `2.0.0-RELEASE`  
**Target Engine**: `liva-native-core` (Rust Unified Native Engine) + Tauri v2 IPC + SQLite WAL (Schema v8) + Obsidian Knowledge Mesh  
**Status**: Authoritative Architectural Standard  
**Classification**: Phase 2 Engineering Specification & System Blueprint  
**Date**: 2026-08-14  

---

## 1. Executive Summary & Architectural Vision

### 1.1 Scope & Mission
Phase 2 of the LIVA Skill Ecosystem represents a major evolutionary leap from localized productivity assistants to an enterprise-grade, privacy-first, autonomous engineering and operational intelligence mesh. All skills in Phase 2 operate directly against LIVA’s Unified Native Engine (`liva-native-core` in Rust), strictly eliminating legacy node/python runtime overhead, achieving deterministic sub-5ms native tool execution, and enforcing local data sovereignty under strict Zero-Knowledge invariants (GDPR and Vietnamese Decree 13/2023/NĐ-CP).

The Phase 2 Skill Pack delivers six specialized, production-ready capabilities divided into two symmetrical domains:
1. **Developer & Infrastructure Operations Pack (DevOps & DevSecOps)**:
   - `liva-smart-devops`: Autonomous container diagnostics (Docker/Kubernetes), structured log streaming, CI/CD telemetry analysis, and operator-gated patch proposals.
   - `liva-security-pdg`: GitNexus Program Dependence Graph (PDG) and AST-level taint analysis, control dependency tracking (`CDG`), reaching definitions (`REACHING_DEF`), and upstream blast radius calculation.
   - `liva-workflow-orchestrator`: Multi-agent Directed Acyclic Graph (DAG) task scheduling, Kahn’s topological wave resolution, SQLite WAL checkpointing, voting consensus (quorum & supermajority), and Dead Letter Queue (DLQ) self-healing.
2. **Enterprise, Business Intelligence & Financial Intelligence Pack (Enterprise & Data)**:
   - `liva-bi-analyst`: Natural Language to SQL (Text-to-SQL), AST syntax & security verification, read-only connection pooling, KPI computation, and declarative Vega-Lite / Mermaid data visualization.
   - `liva-crm-erp-bridge`: Bi-directional synchronization across Salesforce, HubSpot, and Odoo via REST/JSON-RPC, SHA-256 idempotency hashing, vector clock conflict resolution, and two-phase write gating.
   - `liva-financial-advisor`: Comprehensive financial statement analysis, double-entry bookkeeping invariant validation ($\sum \text{Debits} \equiv \sum \text{Credits}$), AES-256-GCM encrypted ledger persistence, and probabilistic cash flow runway forecasting.

```
+-------------------------------------------------------------------------------------------------------------------+
|                                          LIVA DESKTOP CLIENT (TAURI v2 / WEBVIEW)                                  |
|                                       Vue 3 + TypeScript Interactive Control Plane                                |
+-------------------------------------------------------------------------------------------------------------------+
                                                          |
                                           [Tauri IPC / CapBAC Enforced Commands]
                                                          v
+-------------------------------------------------------------------------------------------------------------------+
|                                               liva-native-core (RUST)                                             |
|                                                                                                                   |
|  +-------------------------------------------------------------------------------------------------------------+  |
|  |                                      PRINCIPAL AUTHORIZATION & DISPATCHER                                   |  |
|  |     CommandPrincipal: LocalCli | TauriDashboard | TauriWidget | WebSocketRemote | Telegram                    |  |
|  |     ExecPolicy: Auto (Safe Read-Only) <---> ProposeOnly (Mutations / 2-Phase HITL Gate)                     |  |
|  +-------------------------------------------------------------------------------------------------------------+  |
|                                                         |                                                         |
|         +-----------------------------------------------+-----------------------------------------------+         |
|         v                                                                                               v         |
|  +-------------------------------------+                                                 +---------------------+  |
|  |       DEV & OPS SKILL ENGINES       |                                                 | ENTERPRISE & DATA   |  |
|  | - liva-smart-devops                 |                                                 | - liva-bi-analyst   |  |
|  | - liva-security-pdg                 |                                                 | - liva-crm-erp      |  |
|  | - liva-workflow-orchestrator        |                                                 | - liva-finance      |  |
|  +-------------------------------------+                                                 +---------------------+  |
|         |                                                                                               |         |
|         +-----------------------------------------------+-----------------------------------------------+         |
|                                                         v                                                         |
|  +-------------------------------------------------------------------------------------------------------------+  |
|  |                                          PERSISTENCE & SECURITY FABRIC                                      |  |
|  |  * SQLite WAL (r2d2 Pool: 1 Writer, 4 Readers) [Schema v8]      * AES-256-GCM Ring / OS Keychain Key Derivation  |  |
|  |  * sqlite-vec Vector KNN (vec0) + FTS5 Hybrid Search            * Zero-Knowledge PII Tokenizer (Decree 13)    |  |
|  +-------------------------------------------------------------------------------------------------------------+  |
+-------------------------------------------------------------------------------------------------------------------+
                                                          |
                                           [Model Context Protocol (MCP) Mesh]
                                                          v
+-------------------------------------------------------------------------------------------------------------------+
|               GitNexus MCP Server  |  Obsidian Vault MCP Server  |  Browser-Use  |  Postgres/MySQL DB             |
+-------------------------------------------------------------------------------------------------------------------+
```

---

## 2. Core Architectural Pillars

### 2.1 Capability-Based Access Control (CapBAC) & Fail-Closed Dispatching
Every incoming command from the frontend or external gateways is subjected to strict principal verification (`liva-native-core/src/authorization.rs`). The system operates on two execution tiers:
1. **`ExecPolicy::Auto`**: Deterministic, read-only, non-destructive operations (e.g., container log inspection, PDG taint tracing, read-only SQL queries, financial ratio math, CRM record lookup). Executed immediately in-process.
2. **`ExecPolicy::ProposeOnly`**: Mutating, disruptive, or external side-effect operations (e.g., container restart/patching, code refactoring, CRM record creation/deletion, ledger writes, external communication). The engine constructs a structured draft preview and stages it. Execution is blocked until an authenticated operator signs off via an explicit cryptographic or UI token.

### 2.2 Progressive Context Disclosure & Ephemeral Caching
To prevent system prompt token saturation and keep memory overhead minimal:
- **Tier-0 Discovery**: Only skill metadata (`name` + 1-sentence `description`) is injected into the active LLM context (~35 tokens per skill).
- **Tier-1 Hydration**: The full `SKILL.md` is pulled into context on-demand when the skill is selected by the router.
- **Tier-2 Execution Artifacts**: Native tools, schemas, and verification scripts are invoked via in-process Rust FFI or MCP.
- **Prompt Caching**: Static skill instruction blocks use Anthropic/Gemini ephemeral caching headers (`cache_control: {"type": "ephemeral"}`), maintaining TTFT below 200ms and reducing token costs by up to 90%.

### 2.3 Local-First Zero-Knowledge Data Privacy
In strict compliance with **EU GDPR** (Article 17/25) and **Vietnam Decree 13/2023/NĐ-CP**:
- All financial balances, personal tax records, and CRM customer identifiers are encrypted at rest using authenticated **AES-256-GCM** with unique 96-bit nonces.
- Outbound LLM prompts undergo native PII sanitization (masking Vietnamese Citizen ID / CCCD, tax codes, bank accounts, emails, and phone numbers) before crossing external API boundaries.
- Cryptographic keys are never persisted in plaintext; master keys are derived from the OS secure keychain (Windows Credential Manager / Apple Keychain / Linux Secret Service) using PBKDF2/Argon2id.

---

## 3. Deep-Dive Skill Blueprints

```
===================================================================================================================
SECTION 3.1: liva-smart-devops (Autonomous Container Diagnostics & CI/CD Telemetry)
===================================================================================================================
```

### 3.1 `liva-smart-devops`

#### 3.1.1 Purpose & Scope
Provides continuous observability, container diagnostics, automated crash dump triage, and telemetry analysis across Docker, Podman, Kubernetes, and CI/CD pipelines (GitHub Actions, GitLab CI).

#### 3.1.2 Functional Workflow
1. **Telemetry Capture**: Queries native container runtime sockets (`/var/run/docker.sock` or Windows Named Pipe `//./pipe/docker_engine`) or Kubernetes API endpoints to stream logs and resource metrics (CPU, Memory, Disk I/O, Network, OOM kills).
2. **Pattern Recognition & Triage**: Runs high-speed regex and heuristic tokenizers in Rust over raw log streams to identify crash loops (`CrashLoopBackOff`), Out-Of-Memory (`OOMKilled` / Exit Code 137), segmentation faults (SIGSEGV / Exit Code 139), and unhandled runtime exceptions.
3. **Patch Generation**: In the event of configuration errors (e.g., malformed `docker-compose.yml`, invalid Kubernetes resource limits, Dockerfile dependency conflicts), constructs a unified diff patch (`diff -u`).
4. **Safety Gating**: Patch application or container restart commands are gated under `ExecPolicy::ProposeOnly`.

```
[ Container / Pipeline Error ] ---> ( Native Log Streamer )
                                           |
                                           v
                             [ Burst & Pattern Detector ]
                                           |
                    +----------------------+----------------------+
                    v                                             v
           [ Diagnostic Report ]                        [ Remediation Patch ]
                    |                                             |
                    v                                             v
           ( ExecPolicy::Auto )                        ( ExecPolicy::ProposeOnly )
                    |                                             |
                    v                                             v
           [ Render Dashboard ]                         [ Two-Phase HITL Gate ]
                                                                  |
                                                                  v (Approved)
                                                        [ Apply Patch / Restart ]
```

#### 3.1.3 Rust Structs & Data Contracts
```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContainerEngine {
    Docker,
    Podman,
    Kubernetes { namespace: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerLogFilter {
    pub engine: ContainerEngine,
    pub container_id: String,
    pub tail_lines: usize,
    pub since_timestamp: Option<i64>,
    pub grep_pattern: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Degraded { reason: String },
    Critical { exit_code: Option<i32>, oom_killed: bool, message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerDiagnosticReport {
    pub container_id: String,
    pub image: String,
    pub status: HealthStatus,
    pub memory_usage_bytes: u64,
    pub memory_limit_bytes: u64,
    pub cpu_percentage: f32,
    pub error_burst_count: u32,
    pub log_snippets: Vec<String>,
    pub recommended_action: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevOpsPatchProposal {
    pub proposal_id: String,
    pub target_file: PathBuf,
    pub original_sha256: String,
    pub unified_diff: String,
    pub rationale: String,
    pub rollback_instructions: String,
    pub requires_restart: bool,
}
```

---

```
===================================================================================================================
SECTION 3.2: liva-security-pdg (Program Dependence Graph Security & AST Taint Analysis)
===================================================================================================================
```

### 3.2 `liva-security-pdg`

#### 3.2.1 Purpose & Scope
Performs deep structural security auditing by interfacing directly with GitNexus’s Program Dependence Graph (PDG), Control Dependence Graph (CDG), and AST Reaching Definitions (`REACHING_DEF`). Detects injection sinks, unauthorized path traversals, broken access controls, and upstream blast radiuses.

#### 3.2.2 Taint Tracking & Blast Radius Algorithm
1. **Source Identification**: Identifies untrusted user ingress points (HTTP request parameters, WebSocket messages, CLI args, unvalidated file inputs).
2. **PDG Traversal**: Traces data flow across AST nodes using `REACHING_DEF` chains and checks guard conditions via `CDG`.
3. **Sink Verification**: Detects if tainted data reaches sensitive execution sinks (e.g., `rusqlite::execute`, `std::process::Command`, `std::fs::write`, `eval`) without passing through an authorized sanitizer.
4. **Blast Radius Computation**: Calculates the upstream transitive closure of callers and affected business processes. If blast radius count exceeds 10 execution flows or touches critical crypto/auth routines, a `Critical` risk escalation is triggered.

```
[ Untrusted Input (Source) ]
             |
             v (REACHING_DEF)
     [ Variable Assignment ]
             |
             v (CDG: Guard Clause Check)
      { Is Sanitized? }
      /               \
   (Yes)              (No)
    /                   \
[ Safe Flow ]     [ Sensitive Sink (SQL/Exec/FS) ]
                         |
                         v
              [ TAINT VULNERABILITY DETECTED ]
                         |
                         v
              [ Calculate Blast Radius ]
                         |
                         +---> Upstream Callers
                         +---> Dependent Flows
                         +---> Risk Level (High/Critical)
```

#### 3.2.3 Rust Structs & Data Contracts
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BlastRiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaintSinkType {
    SqlInjection,
    CommandInjection,
    PathTraversal,
    MemoryUnsafeDeref,
    InsecureDeserialization,
    UnsanitizedXss,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaintPathNode {
    pub file_path: String,
    pub line_number: u32,
    pub column_number: u32,
    pub symbol_name: String,
    pub statement_snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaintTraceReport {
    pub trace_id: String,
    pub sink_type: TaintSinkType,
    pub source_symbol: String,
    pub sink_symbol: String,
    pub path: Vec<TaintPathNode>,
    pub is_sanitized: bool,
    pub sanitization_guard: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlastRadiusAssessment {
    pub target_symbol: String,
    pub risk_level: BlastRiskLevel,
    pub direct_callers: Vec<String>,
    pub affected_execution_flows: Vec<String>,
    pub affected_file_count: usize,
    pub touches_auth_or_crypto: bool,
    pub mitigation_summary: String,
}
```

---

```
===================================================================================================================
SECTION 3.3: liva-workflow-orchestrator (Autonomous DAG Multi-Agent Scheduling & Self-Healing)
===================================================================================================================
```

### 3.3 `liva-workflow-orchestrator`

#### 3.3.1 Purpose & Scope
Coordinates complex, multi-stage, multi-agent workflows modelled as Directed Acyclic Graphs (DAGs). Manages execution checkpoints in SQLite WAL, coordinates role-based consensus voting, and triggers automated self-healing via Dead Letter Queues (DLQ).

#### 3.3.2 Execution Lifecycle & Wave Resolution
1. **DAG Validation**: Accepts task nodes and dependency declarations. Validates acyclicity using Kahn’s Topological Sort Algorithm. Detects circular dependencies prior to execution.
2. **Wave Partitioning**: Groups independent tasks into concurrent execution waves (`Vec<Vec<TaskNode>>`). Tasks in Wave $N$ only execute after all dependencies in Wave $N-1$ have succeeded.
3. **State Checkpointing**: Persists intermediate task state in `orchestrator_tasks` after every wave.
4. **Consensus Voting**: For high-stakes architectural or financial choices, gathers votes from distinct agent roles (`architect`, `security_auditor`, `qa`, `domain_specialist`). Enforces Quorum ($\ge 3$ agents) and Supermajority ($\ge 66\%$ approval).
5. **Self-Healing & DLQ**: If a task fails, retries with exponential backoff. If max retries are exhausted, routes the failed state to `dlq_consolidation`, isolates the branch, and notifies the operator.

```
       [ Goal Specification ]
                 |
                 v
      [ Kahn's DAG Validator ]
                 |
                 v
   +-------------+-------------+
   |   WAVE 0: Independent     | ---> [ Agent 1: Research ] & [ Agent 2: Scan ]
   +-------------+-------------+
                 |
                 v (Checkpoint to SQLite WAL)
   +-------------+-------------+
   |   WAVE 1: Dependent Tasks | ---> [ Agent 3: Implement Code ]
   +-------------+-------------+
                 |
                 v
   +-------------+-------------+
   |   WAVE 2: Consensus Gate  | ---> [ Architect ] + [ Security ] + [ QA ]
   +-------------+-------------+
                 |
         { Quorum & >=66%? }
         /                 \
     (Pass)               (Fail)
       /                     \
[ Workflow Success ]    [ DLQ Isolation & Recovery ]
```

#### 3.3.3 Rust Structs & Data Contracts
```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum AgentRole {
    Researcher,
    Architect,
    Implementer,
    QualityAssurance,
    SecurityAuditor,
    DomainSpecialist,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed { error_message: String, retry_count: u32 },
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorTaskNode {
    pub task_id: String,
    pub title: String,
    pub assigned_role: AgentRole,
    pub dependencies: Vec<String>,
    pub input_context: serde_json::Value,
    pub status: TaskStatus,
    pub output_result: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDagSpec {
    pub workflow_id: String,
    pub title: String,
    pub tasks: HashMap<String, OrchestratorTaskNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DagScheduleResult {
    ScheduledWaves(Vec<Vec<String>>),
    CycleDetected { cyclic_task_ids: Vec<String> },
    InvalidDependency { task_id: String, missing_dep: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentVote {
    pub voter_role: AgentRole,
    pub approved: bool,
    pub confidence_score: f32,
    pub reasoning: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusResult {
    pub quorum_reached: bool,
    pub total_votes: usize,
    pub approvals: usize,
    pub rejections: usize,
    pub is_supermajority_approved: bool,
    pub dissent_summaries: Vec<String>,
}
```

---

```
===================================================================================================================
SECTION 3.4: liva-bi-analyst (Secure Text-to-SQL, AST Verification & Data Visualization)
===================================================================================================================
```

### 3.4 `liva-bi-analyst`

#### 3.4.1 Purpose & Scope
Converts natural language analytical questions into highly optimized, safe, read-only SQL queries. Enforces strict AST validation, executes against dedicated read-only connection pools with timeouts and row limits, computes key business KPIs, and formats declarative Vega-Lite and Mermaid charts.

#### 3.4.2 SQL AST Safety Engine & Execution Sandboxing
1. **Schema Introspection**: Pulls table DDL, column types, foreign keys, and indexes into context.
2. **Natural Language Generation**: Generates compliant ANSI/SQLite/PostgreSQL queries.
3. **Native AST Hard Gate**: Before any query touches a database connection, it is parsed by `sqlparser-rs` in native Rust.
   - **Allowed Root Statements**: `Statement::Query` (e.g., `SELECT`, `WITH ... SELECT`), `Statement::Explain`.
   - **Strictly Blocked Statements**: `INSERT`, `UPDATE`, `DELETE`, `DROP`, `ALTER`, `TRUNCATE`, `CREATE`, `GRANT`, `REVOKE`, `ATTACH`, `DETACH`, `PRAGMA` mutations.
   - **Multi-Statement Blocker**: Semicolon-separated multiple queries are rejected to prevent piggybacked injections.
4. **Sandboxed Connection Pool**: Queries execute on the read-only pool (`CustomSqliteManager { read_only: true }`) with a strict **5,000ms execution timeout** and a hard ceiling of **1,000 result rows**.
5. **Visualization Formatter**: Renders outputs as structured JSON tables, KPI scorecard aggregates, or Vega-Lite specification payloads.

```
[ Natural Language Query ] ---> ( LLM Text-to-SQL Generator )
                                             |
                                             v
                                  [ Raw SQL Query String ]
                                             |
                                             v
                             { Rust AST sqlparser-rs Engine }
                             /                              \
                      (Valid SELECT / WITH)        (Mutation / Multi-Statement)
                             /                                \
                 [ Read-Only Pool (5s Timeout) ]         [ HARD REJECTION & AUDIT ]
                             |
                             v
                 [ Result Rows (<=1000) ]
                             |
                    +--------+--------+
                    v                 v
           [ KPI Aggregations ]   [ Vega-Lite / Mermaid Specs ]
```

#### 3.4.3 Rust Structs & Data Contracts
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SqlSafetyVerdict {
    SafeReadOnly,
    BlockedMutationStatement { detected_keyword: String },
    BlockedMultiStatement,
    SyntaxParseError { details: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiQueryRequest {
    pub natural_prompt: String,
    pub target_schema: Option<String>,
    pub max_rows: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiQueryResult {
    pub executed_sql: String,
    pub execution_time_ms: u64,
    pub column_headers: Vec<String>,
    pub row_count: usize,
    pub data_rows: Vec<Vec<serde_json::Value>>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VisualizationType {
    BarChart,
    LineTrend,
    AreaChart,
    PieChart,
    ScorecardKpi,
    MermaidFlow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VegaLiteChartSpec {
    pub chart_type: VisualizationType,
    pub title: String,
    pub vega_lite_json: serde_json::Value,
    pub summary_takeaway: String,
}
```

---

```
===================================================================================================================
SECTION 3.5: liva-crm-erp-bridge (Enterprise Synchronization & Idempotency Bridge)
===================================================================================================================
```

### 3.5 `liva-crm-erp-bridge`

#### 3.5.1 Purpose & Scope
Provides seamless integration, bidirectional sync, and entity resolution across enterprise platforms including Salesforce (REST), HubSpot (CRM v3 API), and Odoo (XML-RPC / JSON-RPC).

#### 3.5.2 Idempotency & Conflict Resolution Protocol
1. **Encrypted Credential Storage**: Platform API keys, client secrets, and OAuth refresh tokens are stored in the local encrypted keychain and injected at runtime.
2. **Idempotency Hashing**: Every sync request computes an SHA-256 hash of `(platform, entity_type, external_id, payload_checksum)`. Repeated attempts with matching hashes are deduplicated.
3. **Conflict Resolution**: Employs **Last-Write-Wins with Revision Clocks**. If the remote revision timestamp is newer than the local cache, the local model is updated. If both have diverged, a manual resolution diff is staged.
4. **Two-Phase Write Confirmation**: All mutating operations (e.g., creating quotes, updating sales deals, modifying contact ownership) require explicit operator confirmation before firing network requests.

```
[ CRM/ERP Sync Request ] ---> ( Idempotency Engine: SHA-256 )
                                             |
                                  { In Sync Ledger? }
                                  /                 \
                              (Match)              (New)
                                /                     \
                     [ Return Cached Ack ]      [ Fetch Remote Record ]
                                                       |
                                            { Check Vector Clocks }
                                            /                     \
                                      (No Conflict)            (Conflict)
                                          /                         \
                             [ Stage Mutation Draft ]     [ Generate 3-Way Diff ]
                                          |                         |
                                          v                         v
                             [ Operator HITL Approval ]   [ Resolve by Operator ]
                                          |
                                          v (Approved)
                             [ Dispatch REST/JSON-RPC ]
                                          |
                                          v
                             [ Record in SQLite WAL ]
```

#### 3.5.3 Rust Structs & Data Contracts
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EnterprisePlatform {
    Salesforce,
    HubSpot,
    Odoo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CrmEntityType {
    Contact,
    Account,
    DealOpportunity,
    Invoice,
    Quotation,
    ProductItem,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrmSyncEvent {
    pub sync_id: String,
    pub platform: EnterprisePlatform,
    pub entity_type: CrmEntityType,
    pub external_id: String,
    pub idempotency_key: String,
    pub payload_hash: String,
    pub payload_json: serde_json::Value,
    pub remote_updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncResolutionStrategy {
    UseRemote,
    UseLocal,
    ManualMerge(serde_json::Value),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrmMutationPreview {
    pub draft_id: String,
    pub platform: EnterprisePlatform,
    pub action: String, // CREATE, UPDATE, DELETE
    pub entity_type: CrmEntityType,
    pub target_id: Option<String>,
    pub diff_summary: String,
    pub staged_payload: serde_json::Value,
}
```

---

```
===================================================================================================================
SECTION 3.6: liva-financial-advisor (AES-256-GCM Encrypted Ledger & Double-Entry Math)
===================================================================================================================
```

### 3.6 `liva-financial-advisor`

#### 3.6.1 Purpose & Scope
Delivers sovereign, zero-knowledge corporate financial statement auditing, balance sheet analysis, cash flow forecasting, and personal investment allocation. All financial records are stored using authenticated AES-256-GCM encryption.

#### 3.6.2 Double-Entry Bookkeeping & Math Invariants
1. **Mathematical Invariant**: Every financial transaction must satisfy the fundamental equation:
   $$\sum \text{Debits} \equiv \sum \text{Credits}$$
   $$\text{Assets} \equiv \text{Liabilities} + \text{Equity}$$
2. **Cash Flow & Burn Rate Forecasting**: Calculates trailing 3-month and 6-month burn rates, net cash burn, and runway survival periods:
   $$\text{Runway (Months)} = \frac{\text{Total Liquid Reserves}}{\text{Average Monthly Net Burn Rate}}$$
3. **Encrypted Persistence**: Ledgers, tax records, and bank statement parses are serialized to JSON, encrypted with AES-256-GCM (96-bit random nonce + 128-bit authentication tag), and stored in `financial_ledgers`.

```
[ Raw Statement / Transaction ] ---> ( Parser & Extractor )
                                              |
                                              v
                              { Double-Entry Invariant Guard }
                              /                              \
               (\sum Debits == \sum Credits)        (\sum Debits != \sum Credits)
                              /                                \
                 [ Compute Financial Metrics ]            [ REJECT TRANSACTION ]
                              |
                    +---------+---------+
                    v                   v
           [ Runway & Ratios ]   [ AES-256-GCM Encryptor ]
                    |                   |
                    v                   v
           [ Markdown Report ]   [ Save to SQLite WAL ]
```

#### 3.6.3 Rust Structs & Data Contracts
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AccountCategory {
    Asset,
    Liability,
    Equity,
    Revenue,
    Expense,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntryItem {
    pub account_name: String,
    pub category: AccountCategory,
    pub debit_cents: u64,
    pub credit_cents: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalTransaction {
    pub transaction_id: String,
    pub timestamp: i64,
    pub description: String,
    pub entries: Vec<JournalEntryItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinancialRatioReport {
    pub period: String,
    pub gross_margin_percentage: f64,
    pub net_profit_margin_percentage: f64,
    pub current_liquidity_ratio: f64,
    pub debt_to_equity_ratio: f64,
    pub average_monthly_burn_rate_cents: u64,
    pub total_cash_reserves_cents: u64,
    pub estimated_runway_months: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedFinancialLedger {
    pub ledger_id: String,
    pub owner_id: String,
    pub nonce_hex: String,      // 12 bytes (24 hex chars)
    pub ciphertext_hex: String, // Encrypted Journal Transactions
    pub auth_tag_hex: String,   // 16 bytes (32 hex chars)
    pub updated_at: i64,
}
```

---

## 4. Rust FFI Bindings & Tauri Command Plane

### 4.1 Command Principal & Authorization Mapping
All Phase 2 skills are integrated into `liva-native-core/src/authorization.rs` and dispatched via `liva-native-core/src/commands/`.

| Command Name | Module | Policy | Allowed Principals | Description |
|---|---|---|---|---|
| `devops:diagnose_container` | `commands::devops` | `ExecPolicy::Auto` | `LocalCli`, `TauriDashboard` | Streams container logs and computes health status |
| `devops:propose_patch` | `commands::devops` | `ExecPolicy::ProposeOnly` | `LocalCli`, `TauriDashboard` | Proposes unified diff patch for container configs |
| `security:pdg_taint_trace` | `commands::security` | `ExecPolicy::Auto` | `LocalCli`, `TauriDashboard` | Traces source-to-sink taint flows via GitNexus PDG |
| `security:blast_radius` | `commands::security` | `ExecPolicy::Auto` | `LocalCli`, `TauriDashboard` | Calculates upstream callers and affected flows |
| `workflow:dag_dispatch` | `commands::orchestrator`| `ExecPolicy::Auto` | `LocalCli`, `TauriDashboard` | Validates DAG and dispatches wave execution |
| `workflow:consensus_vote` | `commands::orchestrator`| `ExecPolicy::Auto` | `LocalCli`, `TauriDashboard` | Collects role votes and computes supermajority |
| `bi:text_to_sql` | `commands::bi` | `ExecPolicy::Auto` | `LocalCli`, `TauriDashboard` | Verifies AST and executes read-only SQL query |
| `bi:render_chart` | `commands::bi` | `ExecPolicy::Auto` | `LocalCli`, `TauriDashboard` | Generates Vega-Lite / Mermaid chart specs |
| `crm:sync_entities` | `commands::crm` | `ExecPolicy::Auto` | `LocalCli`, `TauriDashboard` | Syncs CRM records using idempotency keys |
| `crm:draft_mutation` | `commands::crm` | `ExecPolicy::ProposeOnly` | `LocalCli`, `TauriDashboard` | Previews CRM record creation or modification |
| `finance:analyze_statements` | `commands::finance` | `ExecPolicy::Auto` | `LocalCli`, `TauriDashboard` | Validates double-entry and computes ratios |
| `finance:encrypt_ledger` | `commands::finance` | `ExecPolicy::ProposeOnly` | `LocalCli`, `TauriDashboard` | Encrypts financial ledger with AES-256-GCM |

### 4.2 Native Tauri Dispatch Implementation Blueprint
```rust
// liva-native-core/src/commands/mod.rs (Extract)
pub mod bi;
pub mod crm;
pub mod devops;
pub mod finance;
pub mod orchestrator;
pub mod security;

use crate::authorization::{CommandPrincipal, ExecPolicy, authorize_command};
use crate::db::DatabasePool;
use serde_json::Value;

pub async fn handle_phase2_command(
    principal: CommandPrincipal,
    command_name: &str,
    payload: Value,
    db: &DatabasePool,
) -> Result<Value, String> {
    authorize_command(principal, command_name)?;

    match command_name {
        "devops:diagnose_container" => devops::diagnose_container(payload).await,
        "devops:propose_patch" => devops::propose_patch(payload, db).await,
        "security:pdg_taint_trace" => security::pdg_taint_trace(payload).await,
        "security:blast_radius" => security::calculate_blast_radius(payload).await,
        "workflow:dag_dispatch" => orchestrator::dispatch_dag(payload, db).await,
        "workflow:consensus_vote" => orchestrator::record_vote(payload, db).await,
        "bi:text_to_sql" => bi::execute_text_to_sql(payload, db).await,
        "bi:render_chart" => bi::render_chart_spec(payload).await,
        "crm:sync_entities" => crm::sync_entities(payload, db).await,
        "crm:draft_mutation" => crm::stage_mutation_preview(payload, db).await,
        "finance:analyze_statements" => finance::analyze_statements(payload).await,
        "finance:encrypt_ledger" => finance::encrypt_and_save_ledger(payload, db).await,
        _ => Err(format!("Unknown Phase 2 command: {}", command_name)),
    }
}
```

---

## 5. SQLite WAL Schema Extensions (Schema Migration v8)

To support the Phase 2 Skill Pack without breaking existing data or Tauri IPC contracts, the database schema version is bumped from `SCHEMA_VERSION = 7` to `SCHEMA_VERSION = 8` in `liva-native-core/src/db.rs`.

### 5.1 Database Migration DDL (Version 8)
```sql
-- ============================================================================
-- Migration Version 8: LIVA Phase 2 Full Skill Pack Schema Extensions
-- ============================================================================

-- 1. Multi-Agent Workflow Orchestrator Tables
CREATE TABLE IF NOT EXISTS orchestrator_workflows (
    workflow_id     TEXT PRIMARY KEY,
    title           TEXT NOT NULL,
    status          TEXT NOT NULL CHECK(status IN ('pending', 'running', 'completed', 'failed', 'cancelled')),
    total_tasks     INTEGER NOT NULL,
    completed_tasks INTEGER NOT NULL DEFAULT 0,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS orchestrator_tasks (
    task_id         TEXT PRIMARY KEY,
    workflow_id     TEXT NOT NULL REFERENCES orchestrator_workflows(workflow_id) ON DELETE CASCADE,
    title           TEXT NOT NULL,
    assigned_role   TEXT NOT NULL,
    dependencies    TEXT NOT NULL DEFAULT '[]', -- JSON Array of task_ids
    input_context   TEXT NOT NULL DEFAULT '{}', -- JSON Object
    output_result   TEXT,                       -- JSON Object
    status          TEXT NOT NULL CHECK(status IN ('pending', 'running', 'completed', 'failed', 'skipped')),
    retry_count     INTEGER NOT NULL DEFAULT 0,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_orchestrator_tasks_wf 
    ON orchestrator_tasks(workflow_id, status);

-- 2. Business Intelligence Query Audit Table
CREATE TABLE IF NOT EXISTS bi_query_audit (
    audit_id          TEXT PRIMARY KEY,
    natural_prompt    TEXT NOT NULL,
    executed_sql      TEXT NOT NULL,
    execution_time_ms INTEGER NOT NULL,
    row_count         INTEGER NOT NULL,
    verdict           TEXT NOT NULL,
    created_at        INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_bi_query_audit_time 
    ON bi_query_audit(created_at);

-- 3. CRM & ERP Bi-directional Sync Ledger Table
CREATE TABLE IF NOT EXISTS crm_erp_sync_events (
    sync_id          TEXT PRIMARY KEY,
    platform         TEXT NOT NULL CHECK(platform IN ('salesforce', 'hubspot', 'odoo')),
    entity_type      TEXT NOT NULL,
    external_id      TEXT NOT NULL,
    idempotency_key  TEXT NOT NULL UNIQUE,
    payload_hash     TEXT NOT NULL,
    payload_json     TEXT NOT NULL,
    sync_status      TEXT NOT NULL CHECK(sync_status IN ('synced', 'conflict', 'pending_approval')),
    last_synced_at   INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_crm_sync_platform_entity 
    ON crm_erp_sync_events(platform, entity_type, external_id);

-- 4. AES-256-GCM Encrypted Financial Ledgers Table
CREATE TABLE IF NOT EXISTS financial_ledgers (
    ledger_id       TEXT PRIMARY KEY,
    owner_id        TEXT NOT NULL,
    title           TEXT NOT NULL,
    nonce_hex       TEXT NOT NULL,      -- 12 bytes (24 hex characters)
    ciphertext_hex  TEXT NOT NULL,      -- Encrypted ledger payload
    auth_tag_hex    TEXT NOT NULL,      -- 16 bytes (32 hex characters)
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_financial_ledgers_owner 
    ON financial_ledgers(owner_id);
```

---

## 6. Security, Privacy & Zero-Knowledge Invariants

### 6.1 Regulatory Compliance Matrix

| Requirement / Standard | Implementation in `liva-native-core` | Verification Mechanism |
|---|---|---|
| **Vietnam Decree 13/2023/NĐ-CP** (Personal Data Protection) | Native Regex + Named Entity Tokenizer masks Vietnamese CCCD (12 digits), Personal Tax Codes (10 digits), and bank accounts prior to prompt building. | Automated test suite in `liva-compliance-sanitizer` |
| **EU GDPR Art. 17** (Right to Erasure) | `deletion_audit` table logs cryptographic scope hashes with zero plaintext retention; cascades across all task and ledger tables. | `db::delete_conversation` and `db::delete_subject` unit tests |
| **Zero-Knowledge Encryption** | Financial ledgers encrypted at rest using AES-256-GCM; master keys held strictly in local OS Keychain, never transmitted off-box. | `crypto::EncryptionEngine` test harness |
| **SQL Injection Prevention** | AST parsing via `sqlparser-rs` rejects all multi-statements and mutation keywords; executed on read-only SQLite/Postgres connection pools. | `bi::test_ast_safety_guard` test cases |
| **Command Injection Guard** | DevOps patching uses structured file IO with path normalization; blocks shell execution strings (`sh -c`, `cmd.exe /c`). | `devops::test_path_sanitization` test cases |

---

## 7. Concurrency, Connection Pools & Failure Recovery

### 7.1 Multi-Agent Read/Write Concurrency
LIVA's database layer uses an `r2d2` connection pool configured for SQLite WAL mode:
- **Dedicated Writer Pool (Size = 1)**: Serializes all state mutations (task checkpoints, sync ledgers, encrypted balances) to eliminate `SQLITE_BUSY` lock contention.
- **Dedicated Reader Pool (Size = 4)**: Executes concurrent read-only queries (BI queries, log lookups, memory searches, vector KNN) in parallel with sub-millisecond response times.
- **WAL Auto-Checkpointing**: Auto-checkpoint threshold set to 500 pages (`PRAGMA wal_autocheckpoint = 500`) to maintain compact database file size.

### 7.2 Dead Letter Queue (DLQ) Recovery Flow
```
[ Failed Orchestrator Task ]
             |
             v
   { Retry Count < 3? }
   /                  \
 (Yes)                (No)
  /                     \
[ Exponential Backoff ]   [ Route to `dlq_consolidation` ]
                                |
                                v
                     [ Isolate Sub-Graph ]
                                |
                                v
                     [ Emit Alert Event to Tauri UI ]
                                |
                                v
                     [ Operator Manual Re-run / Patch ]
```

---

## 8. Skill Governance & Verification Strategy

### 8.1 Governance Rules & Quality Gates
In accordance with LIVA Skill Governance standards:
1. **Frontmatter Dialect Separation**:
   - `SKILL.md`: Contains strictly `name` and `description`.
   - `agents/openai.yaml`: Houses machine metadata, tool dependencies, and policy flags.
   - Obsidian Vault Notes: Use `title`, `tags`, `author`, `last_update`.
2. **Zero Drift Parity**:
   - Byte-identical symmetry is maintained between `.claude/skills/<skill-name>/` and `.agents/skills/<skill-name>/`.
3. **Zero Placeholder Policy**:
   - Zero occurrences of `[TODO]`, `{{placeholder}}`, or template stubs in production files.
4. **Automated Verification Harness**:
   - `npm run test:skills`: Must pass 100% (25/25 test cases).
   - `npm run skills:audit`: Must complete with **0 errors and 0 warnings**.

### 8.2 End-to-End Test Matrix for Phase 2 Skills

| Test Target | Test Command | Success Criteria |
|---|---|---|
| **Skills Audit Engine** | `npm run test:skills` | 25/25 tests pass (100%) |
| **Workspace Integrity** | `npm run skills:audit` | 0 errors, 0 warnings across all 42 `SKILL.md` and 52 vault notes |
| **Rust Native Core Unit Tests** | `cargo test --manifest-path liva-native-core/Cargo.toml` | 100% test pass across `authorization`, `db`, `crypto`, `skills` |
| **AST SQL Safety Gate** | `cargo test --manifest-path liva-native-core/Cargo.toml -- bi_safety` | Rejects 100% of mutation vectors (`DROP`, `INSERT`, `UPDATE`, `--`) |
| **Double-Entry Balance Math** | `cargo test --manifest-path liva-native-core/Cargo.toml -- double_entry` | Invariant verified for unbalanced debits/credits |
| **Kahn DAG Topological Sort** | `cargo test --manifest-path liva-native-core/Cargo.toml -- dag_sort` | Detects cycles and produces correct wave partitioning |

---

## 9. Conclusion & Implementation Roadmap

The LIVA Phase 2 Technical Architecture establishes a robust, highly resilient foundation for deploying advanced developer, security, data, and enterprise capabilities. By integrating directly into `liva-native-core`, leveraging SQLite WAL connection pools, and enforcing CapBAC and Zero-Knowledge security at every boundary, LIVA ensures unmatched performance, zero data leakage, and sovereign enterprise autonomy.
