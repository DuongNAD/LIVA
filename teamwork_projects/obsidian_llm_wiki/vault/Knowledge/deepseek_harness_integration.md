---
title: "deepseek_harness_integration"
tags:
  - liva/knowledge
  - liva/architecture
  - liva/deepseek
  - liva/cordis
author: "LIVA Core Architecture Team"
last_update: "2026-08-16T11:45:00+07:00"
confidence: "high"
sources:
  - "https://github.com/deepseek-ai/deepseek-harness"
  - "docs/05-chat-luong/threat-model.md"
  - "docs/03-he-thong-con/memory.md"
  - "docs/03-he-thong-con/persistence.md"
---

# Knowledge: DeepSeek Harness & Cordis Microkernel Architectural Integration

## Executive Summary

This document establishes the comprehensive architectural deconstruction and integration blueprint for adapting **DeepSeek Harness** (`dsh`) and its foundational meta-framework **Cordis** into LIVA's Unified Native Engine in Rust (`liva-native-core`) and Tauri IPC / Vue 3 desktop interface. 

DeepSeek Harness introduces key agentic design paradigms: **Spatiotemporal Composability**, **Fiber-based plugin lifecycles with LIFO effect rollback**, **Append-Only Session Event Streams**, **Dynamic Token-Budgeted Prompt Compilation**, and a standardized **Evaluation Benchmark Harness** (`DSBench` / `TerminalBench`). 

This specification provides a 1-to-1 architectural comparison against LIVA Native Core, catalogues critical transferable patterns vs. anti-patterns, and details four production-grade RFC specifications:
1. **Scoped Tool Registry & Guarded Execution Pipeline** (`ScopedToolRegistry`, `ToolScope`, `CommandPrincipal` authorization).
2. **Real-Time Dual-Channel Streaming CoT Separation** (`ReasoningStreamSplitter`, `<think>` token isolation, Tauri IPC `isThought` streaming).
3. **Modular Dynamic Prompt Assembly Subsystem** (`DynamicPromptAssembler`, priority-ranked prompt slicing, $N_{\text{ctx}}$ budget enforcement).
4. **Automated Evaluation & Benchmark Harness** (`LIVA-Eval`, TTFT raw/visible, TPS, tool selection accuracy, JSON schema validation).

---

## Cross-References & Canonical Vault Graph

- [[liva_architecture]] — Unified Native Rust runtime boundary, Tauri IPC architecture, and SQLite WAL invariants.
- [[memory_architecture]] — 4-Tier Memory Architecture (L0 RAM to L3 KV Facts), transactional SQLite vector indexing, and KV cache reuse.
- [[anti_patterns]] — Compile-time and runtime constraints (VRAM thrashing, event-loop blocking, unbounded caches, dirty rollbacks).
- [[coding_standards]] — Rust/Tauri non-blocking invariants, error propagation, and Vue 3 `shallowRef` streaming protocols.
- [[tech_stack]] — Official technology bounds (Rust stable, Tokio, `rusqlite`, `sqlite-vec`, `llama-cpp-2`, Vue 3, TypeScript).
- [[commands_reference]] — IPC command catalog, capability boundaries, and session tokens.
- [[testing_guidelines]] — Automated testing tiers, mock boundaries, and regression safety.
- [[LIVA Security PDG]] — Program Dependence Graph taint tracking and source-to-sink verification.
- [[LIVA Workflow Orchestrator]] — DAG task graph decomposition and multi-agent consensus gating.
- `docs/05-chat-luong/threat-model.md` — Formal threat model, trust boundaries, and fail-closed authorization.

---

## 1. DeepSeek Harness & Cordis Microkernel Architectural Deconstruction

```
┌─────────────────────────────────────────────────────────────────────────┐
│                          DEEPSEEK HARNESS (dsh)                         │
├─────────────────────────────────────────────────────────────────────────┤
│  Presets: Standard | Minimal | Code | Creator                           │
│  Interfaces: Web UI (127.0.0.1:3080) / Headless Scriptable Daemon       │
├─────────────────────────────────────────────────────────────────────────┤
│                           CORDIS MICROKERNEL                            │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │                    Spatiotemporal Composability                   │  │
│  │  Spatial: ctx.provide() / ctx.inject() Service Mesh & Scoping     │  │
│  │  Temporal: ctx.plugin() -> Fiber -> LIFO Disposer / Effect Undo   │  │
│  └───────────────────────────────────────────────────────────────────┘  │
│  ┌────────────────────┬────────────────────┬─────────────────────────┐  │
│  │ Model Adapters     │ Tool Registry &    │ Session Event Stream    │  │
│  │ (DeepSeek V4, etc) │ Sandboxed Dispatch │ (Append-Only Log)       │  │
│  ├────────────────────┼────────────────────┼─────────────────────────┤  │
│  │ Dynamic Prompt     │ Agent Turn Loop    │ Evaluation Benchmark    │  │
│  │ Budget Assembly    │ State Orchestrator │ (TerminalBench/DSBench) │  │
│  └────────────────────┴────────────────────┴─────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
```

### 1.1 The "Everything is a Plugin" Microkernel Paradigm

Traditional agent frameworks often devolve into monolithic, tightly coupled systems where central state managers coordinate models, tools, memory, vector indices, and network listeners. This creates hidden side effects, uncollectable memory leaks, and high maintenance overhead.

DeepSeek Harness replaces monolithic orchestration with the **Cordis Microkernel**:
- **Zero In-Kernel Business Logic**: The kernel provides strictly three primitives: an Inversion-of-Control (IoC) dependency container, an event bus, and a hierarchical lifecycle coordinator.
- **Process-Internal Service Mesh**: Model providers, tool catalogs, memory drivers, prompt assemblers, and UI listeners exist as decentralized plugins attached to a shared or scoped `Context`.

### 1.2 Spatiotemporal Composability Model

The theoretical foundation of Cordis is **Spatiotemporal Composability**:

#### A. Spatial Composability (Topology & Scoping)
- **Hierarchical Context Trees**: A `Context` forms a hierarchical tree. Subordinate contexts (e.g., for a subagent or sandboxed execution) inherit services from parent contexts while selectively shadowing or isolating specific services.
- **Service Registration & Injection**:
  - Services register their availability: `ctx.provide('service_name')`.
  - Plugins declare prerequisite dependencies: `ctx.inject = ['service_a', 'service_b']`.
  - Cordis handles lazy dependency resolution: plugin activation is deferred until all declared service dependencies are satisfied in the current context.

#### B. Temporal Composability (Fibers & LIFO Effect Rollback)
- **Fiber / Fork Lifecycle**: Loading a plugin (`ctx.plugin(PluginDef)`) spawns a discrete execution handle called a `Fiber`. The fiber tracks all side effects created by that plugin instance:
  - Event listeners (`ctx.on(...)`)
  - Periodic timers and intervals (`ctx.setInterval(...)`)
  - Tool and schema endpoints (`ctx.tool(...)`)
  - Subprocess handles and file descriptors (`ctx.effect(...)`)
- **LIFO Disposal Invariant**: When a fiber is terminated (`fiber.dispose()`), Cordis walks the tracked side-effect stack in strict **Last-In, First-Out (LIFO)** order and executes corresponding cleanup handlers. This guarantees:
  - Zero zombie event listeners or callbacks.
  - Zero dangling network connections or orphaned subprocesses.
  - Zero stale tool schemas remaining in the LLM's tool catalog upon plugin unload or upgrade.

### 1.3 Append-Only Session Event Stream

DeepSeek Harness enforces an **Append-Only Event Stream** as the single authoritative ledger of session history:

```
[Session Event Stream: Append-Only]
├── 001: Event::SystemPromptCompiled { prompt_hash, token_budget, active_plugins }
├── 002: Event::UserUtterance { text, timestamp, input_provenance }
├── 003: Event::ModelReasoningChunk { cot_tokens, channel: "analysis" }
├── 004: Event::ToolCallProposed { tool: "bash", call_id, arguments }
├── 005: Event::ToolExecutionFinished { call_id, exit_code, stdout, duration_ms }
├── 006: Event::ModelCompletionChunk { visible_tokens, channel: "final" }
└── 007: Event::SessionCheckpoint { state_digest, kv_cache_seq }
```

- **Immutability**: Once recorded, events are never modified in place.
- **Deterministic Replay & Post-Mortem Auditing**: Any execution trajectory can be replayed step-by-step offline to isolate hallucinations, reasoning regressions, or tool execution failures.
- **Time-Travel Forking**: Developers and benchmark suites can branch a session at turn $N$ to evaluate alternative prompts or model parameters without re-running turns $0 \dots N-1$.
- **Crash Resilience**: Abrupt terminations recover cleanly by replaying the event log from the last verified checkpoint.

### 1.4 Scoped Tool Dispatch & Context Injection

Tools are decoupled from ambient global state:
- **Declarative Schema Contract**: Every tool exports a strict JSON Schema defining parameter types, mandatory fields, and semantic descriptions.
- **Explicit Context Injection**: Tools receive an explicit `ToolContext` handle containing session ID, caller identity, working directory, event sink, and remaining token budget. Tools cannot access unconstrained global resources.
- **Dynamic Scoping**: Tools are conditionally visible based on the active agent mode, user privilege level, and current task phase.

### 1.5 Dynamic Prompt Assembly & Token Budget Economics

Prompt compilation in DeepSeek Harness is a budget-constrained pipeline rather than static string templating:

$$\text{Prompt Budget} = N_{\text{ctx}} - \text{RESERVE}_{\text{completion}} - \text{TOKEN}_{\text{user\_input}}$$

1. **Priority Slicing**:
   - **P0 (Critical)**: Core System Persona & Safety Invariants (never evicted).
   - **P1 (Operational)**: Active Tool Schemas (minified JSON representation).
   - **P2 (Immediate)**: Current User Turn & Direct Instructions.
   - **P3 (Episodic/Semantic)**: Retrieved RAG Knowledge & Memory Facts (ranked by relevance).
   - **P4 (Historical)**: Multi-turn Conversation History (sliding window with LIFO eviction).
2. **Real-Time Token Tracking**: Monitors prompt tokens, reasoning tokens, completion tokens, and KV-cache prefix hits to guarantee the request never exceeds the hardware or model context window ($N_{\text{ctx}}$).

### 1.6 Automated Evaluation Harness (DSBench & Benchmark Runners)

DeepSeek Harness includes an automated evaluation harness measuring:
- **Latency Metrics**:
  - **Raw TTFT** ($T_{\text{first\_token}}$): Timestamp of the first token callback (including internal reasoning tokens).
  - **Visible TTFT** ($T_{\text{visible}}$): Timestamp when the first user-facing response token emerges.
  - **Throughput (TPS)**: $\text{Tokens Per Second} = \frac{N_{\text{visible\_tokens}} - 1}{\Delta T_{\text{generation}}}$.
- **Tool Calling Metrics**:
  - **Tool Selection Precision & Recall**: Accuracy of tool selection against golden test corpora.
  - **Schema Validation Rate**: Percentage of tool call arguments that strictly satisfy the target JSON Schema without type coercion errors or missing parameters.
- **Reasoning Fidelity**: Step-by-step verification of Chain-of-Thought (CoT) trajectories across multi-turn tasks.

---

## 2. 1-to-1 Architectural Comparison: DeepSeek Harness vs. LIVA Native Core

| Architectural Dimension | DeepSeek Harness (`dsh`) / Cordis | LIVA Native Core (`liva-native-core` Rust) | Integration Analysis & Enhancement Strategy for LIVA |
|---|---|---|---|
| **Core Runtime Engine** | TypeScript Microkernel (Cordis) | Unified Native Engine in Rust (`AppState`), Tokio Async Runtime, direct Tauri IPC | LIVA possesses zero Node/TS overhead, zero garbage collection pauses, and memory safety. LIVA adapts Cordis's lifecycle model into idiomatic Rust RAII guards. |
| **Plugin & Module Lifecycle** | Hierarchical Contexts, Fibers, LIFO Effect Tracking, Dynamic Loading | Rust compile-time modules, `AppState` Arc, fixed MCP Server/Client registrations | LIVA implements a **Scoped Tool Registry** where tools can be dynamically registered, scoped to sessions or tasks, and automatically unmounted with RAII scope drop guards. |
| **State & Event Logging** | In-Memory / File-based Append-Only Event Stream for replay & branching | SQLite WAL (20-table schema v5) with `conversation_turn`, `facts`, `checkpoint` | LIVA has superior SQLite WAL ACID guarantees, AES-256-GCM encryption, and DPAPI key management. LIVA introduces a structured append-only `SessionEventStream` channel. |
| **Tool Calling & Selection** | Cordis Scoped Tool Registry, declarative JSON Schema, context injection | `llm::tool_calling` (embedding top-k retrieval, 2-line prompt contract, `ExecPolicy`), `NativeMcpServer` | LIVA has efficient top-k vector retrieval for 2B-4B local models. LIVA enhances tool dispatch with structured `ToolExecutionContext` injection and fail-closed privilege verification. |
| **Reasoning / CoT Streaming** | Stream parser separates `<think>` / analysis channels into distinct events | `VisibleOutputFilter` previously stripped thought tags, discarding CoT | **Seam Upgrade**: LIVA implements `ReasoningStreamSplitter` to separate `<think>` tokens in real-time, emitting typed `isThought: true` IPC events to Vue 3 for collapsible UI rendering. |
| **Prompt Assembly & Budgeting** | Dynamic prompt assembler with token budgeting and schema minification | `llm::compile_prompt` (ChatML / Gemma-4 template detection), hardcoded completion reserve | LIVA implements `DynamicPromptAssembler` with explicit priority slicing (P0-P4) to dynamically balance persona, skills, memories, and history under strict $N_{\text{ctx}}$ limits. |
| **Security & Authorization** | Process sandboxing, capability checks | `CommandPrincipal` (`TauriWidget`, `TauriDashboard`, `WebSocketLoopback`, `WebSocketRemote`), `authorize_command`, Stronghold | LIVA binds tool execution policies (`ToolExecPolicy::Auto` vs `ProposeOnly`) to `CommandPrincipal` levels, enforcing fail-closed authorization before tool dispatch. |
| **Evaluation & Benchmarking** | DSBench, TerminalBench 2.1, Toolathlon runner | `bin/ttft_bench.rs` (p50/p95 TTFT, TPS), `bin/tool_calling_probe.rs` (13-case corpus) | LIVA unifies microbenchmarks into `LIVA-Eval` (`bin/liva_eval.rs`), supporting automated multi-tier evaluation of local GGUF models and Cloud APIs. |

---

## 3. Transferable Design Patterns & Anti-Patterns for LIVA Core

### 3.1 Transferable Design Patterns to Adopt

1. **Scoped Tool Registry with RAII Scope Guards**:
   - Implement `NativeScopedToolRegistry` in Rust allowing tool registration tied to an explicit `ToolScope` (`Global`, `Session(String)`, `Task(String)`).
   - Leverage Rust's `Drop` semantics so tools scoped to ephemeral tasks or subagents are automatically unregistered when the scope guard leaves scope.
2. **Dual-Channel CoT Stream Classifier (`ReasoningStreamSplitter`)**:
   - Upgrade `output_filter.rs` to parse reasoning tokens (`<think>`, `<thought>`, `<analysis>`) in real-time without discarding them.
   - Stream separated tokens over Tauri IPC (`ai_stream_chunk` with `isThought: boolean`), enabling Vue 3 to render interactive thought accordions without blocking main-thread rendering.
3. **Deterministic Append-Only Session Event Stream**:
   - Maintain an append-only event stream in Tokio channels and SQLite WAL logging user utterances, reasoning tokens, tool invocations, and memory recalls with microsecond timestamps.
   - Enables instant time-travel debugging, trajectory visualization, and crash recovery.
4. **Token-Budgeted Dynamic Prompt Assembler**:
   - Construct a `DynamicPromptAssembler` enforcing mathematical budget constraints ($N_{\text{ctx}} - \text{RESERVE} - \text{INPUT}$), pruning lower-priority prompt slices before LLM tokenization.
5. **Unified Automated Evaluation Harness (`LIVA-Eval`)**:
   - Build a unified benchmark runner (`liva_eval`) testing both Local GGUF models (`llama-cpp-2`) and Cloud APIs against standardized test suites for TTFT, TPS, and tool calling precision.

### 3.2 Critical Anti-Patterns to Avoid

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        ANTI-PATTERNS TO AVOID                           │
├─────────────────────────────────────────────────────────────────────────┤
│ ❌ JS Monkey-Patching in Rust       │ ❌ In-Memory-Only Event Logs       │
│    Unsafe pointers & unbounded      │    Losing session history on      │
│    Box<dyn Any> maps                │    process crash or power loss    │
├─────────────────────────────────────┼───────────────────────────────────┤
│ ❌ Unbounded Tool Execution         │ ❌ Tokio Thread Blocking          │
│    Executing tools without checking │    Running sync I/O or GGUF       │
│    CommandPrincipal privilege       │    compute on Tokio worker threads│
├─────────────────────────────────────┼───────────────────────────────────┤
│ ❌ Per-Token DOM Re-rendering       │ ❌ Discarding Reasoning Chains    │
│    Triggering deep Vue reactivity   │    Completely stripping <think>   │
│    on every sub-word token          │    preventing user transparency   │
└─────────────────────────────────────┴───────────────────────────────────┘
```

1. **JavaScript-Style Dynamic Monkey-Patching in Rust**:
   - *Anti-Pattern*: Attempting to emulate JS dynamic prototype modifications using `unsafe` pointer casts or unbounded `Box<dyn Any>` maps.
   - *LIVA Remedy*: Use idiomatic Rust trait objects (`Arc<dyn ScopedTool>`), explicit enum typing, and thread-safe channels (`tokio::sync::mpsc`).
2. **In-Memory-Only Event Logging**:
   - *Anti-Pattern*: Buffering execution event streams solely in volatile RAM, causing unrecoverable data loss upon unexpected application shutdown.
   - *LIVA Remedy*: Persist all session events to the SQLite WAL pool with `synchronous=NORMAL` and indexed queries.
3. **Unbounded Tool Execution without Principal Verification**:
   - *Anti-Pattern*: Allowing external MCP tools or dynamic plugins to execute shell or filesystem operations without verifying caller privilege.
   - *LIVA Remedy*: Enforce `CommandPrincipal` authorization and `ToolExecPolicy` checks at every entry point.
4. **Blocking Tokio Control Loops with Heavy Tool Invocations**:
   - *Anti-Pattern*: Executing synchronous filesystem operations or external processes directly within Tokio worker threads.
   - *LIVA Remedy*: Offload all blocking tool calls to `tokio::task::spawn_blocking` or dedicated async subprocesses.
5. **Per-Token Reactive DOM Re-rendering in Vue 3**:
   - *Anti-Pattern*: Emitting fine-grained Vue reactive updates on every single streamed sub-word token, causing high CPU usage and UI stuttering.
   - *LIVA Remedy*: Use `shallowRef` + `triggerRef` batching and windowed history rendering (`messages.slice(-15)`).

---

## 4. Production-Grade RFC Specifications for LIVA Native Core

### RFC 1: Native Scoped Tool Registry & Guarded Execution Pipeline

#### 1.1 Objective
Provide a thread-safe, memory-safe, and dynamically scoped tool registry in `liva-native-core::llm::tool_registry` that integrates seamlessly with `CommandPrincipal`, `ToolExecPolicy`, and MCP protocol definitions.

#### 1.2 Architecture & Data Structures

```rust
// liva-native-core/src/llm/tool_registry.rs

use crate::authorization::CommandPrincipal;
use crate::mcp::protocol::{CallToolRequest, CallToolResult, Tool};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Lifecycle scope for registered tools.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolScope {
    /// Available across all sessions (e.g. native vault tools).
    Global,
    /// Scoped to a specific conversation or session ID.
    Session(String),
    /// Ephemeral scope for a single multi-step task or subagent run.
    Task(String),
}

/// Execution policy defining auto-execution permissions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolExecPolicy {
    /// Safe, read-only or reversible operation; can execute automatically.
    Auto,
    /// Dangerous or state-mutating operation; requires user confirmation.
    ProposeOnly,
    /// Completely disabled in the current scope.
    Forbidden,
}

/// Execution context injected into every tool invocation.
#[derive(Clone)]
pub struct ToolExecutionContext {
    pub session_id: String,
    pub caller_principal: CommandPrincipal,
    pub working_dir: std::path::PathBuf,
    pub event_tx: Option<tokio::sync::mpsc::Sender<String>>,
    pub token_budget_remaining: usize,
}

/// Trait implemented by all native and bridged tools.
#[async_trait::async_trait]
pub trait ScopedTool: Send + Sync {
    /// Tool definition and schema.
    fn definition(&self) -> Tool;
    
    /// Default execution policy for this tool.
    fn default_policy(&self) -> ToolExecPolicy;
    
    /// Minimum principal level required to invoke this tool.
    fn required_principal(&self) -> CommandPrincipal;
    
    /// Execute the tool with sandboxed context.
    async fn execute(
        &self,
        arguments: Value,
        context: ToolExecutionContext,
    ) -> Result<CallToolResult, String>;
}

/// Thread-safe scoped tool registry.
#[derive(Default)]
pub struct NativeScopedToolRegistry {
    tools: RwLock<HashMap<String, (ToolScope, Arc<dyn ScopedTool>)>>,
}

impl NativeScopedToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: RwLock::new(HashMap::new()),
        }
    }

    /// Register a tool under a specific scope.
    pub async fn register(
        &self,
        scope: ToolScope,
        tool: Arc<dyn ScopedTool>,
    ) -> Result<(), String> {
        let name = tool.definition().name;
        let mut guard = self.tools.write().await;
        guard.insert(name, (scope, tool));
        Ok(())
    }

    /// Unregister all tools belonging to a scope (e.g., when a session terminates).
    pub async fn unregister_scope(&self, scope: &ToolScope) {
        let mut guard = self.tools.write().await;
        guard.retain(|_, (s, _)| s != scope);
    }

    /// List all tools visible to a given session and principal.
    pub async fn list_visible_tools(
        &self,
        session_id: &str,
        principal: CommandPrincipal,
    ) -> Vec<Tool> {
        let guard = self.tools.read().await;
        guard
            .values()
            .filter(|(scope, tool)| {
                let scope_match = match scope {
                    ToolScope::Global => true,
                    ToolScope::Session(sid) => sid == session_id,
                    ToolScope::Task(tid) => tid == session_id,
                };
                let auth_match = principal >= tool.required_principal();
                scope_match && auth_match
            })
            .map(|(_, tool)| tool.definition())
            .collect()
    }

    /// Dispatches a tool call with fail-closed policy enforcement.
    pub async fn dispatch(
        &self,
        name: &str,
        arguments: Value,
        context: ToolExecutionContext,
    ) -> Result<CallToolResult, String> {
        let tool = {
            let guard = self.tools.read().await;
            let (scope, tool) = guard
                .get(name)
                .ok_or_else(|| format!("Tool '{}' not found in registry", name))?;
            
            match scope {
                ToolScope::Global => {}
                ToolScope::Session(sid) if sid == &context.session_id => {}
                ToolScope::Task(tid) if tid == &context.session_id => {}
                _ => return Err(format!("Tool '{}' is not accessible in session '{}'", name, context.session_id)),
            }
            
            if context.caller_principal < tool.required_principal() {
                return Err(format!(
                    "Principal '{:?}' denied permission for tool '{}' (requires '{:?}')",
                    context.caller_principal, name, tool.required_principal()
                ));
            }

            Arc::clone(tool)
        };

        let schema = serde_json::to_value(tool.definition().input_schema)
            .map_err(|e| format!("Invalid schema: {e}"))?;
        crate::llm::tool_calling::validate_arguments(&schema, &arguments)?;

        tool.execute(arguments, context).await
    }
}
```

---

### RFC 2: Real-time Streaming CoT / Reasoning Token Seam

#### 2.1 Objective
Upgrade the streaming generation pipeline in `liva-native-core::llm` to isolate reasoning tokens (`<think>`, `<thought>`, `<analysis>`) in real time, emitting typed IPC chunks so the Vue 3 frontend can render interactive reasoning thought accordions alongside final responses.

#### 2.2 Streaming State Machine & IPC Pipeline

```
[Raw Token Stream from llama.cpp / Cloud API]
                      │
                      ▼
        ┌───────────────────────────┐
        │  ReasoningStreamSplitter  │
        └─────────────┬─────────────┘
                      │
        ┌─────────────┴─────────────┐
        ▼                           ▼
[Reasoning Channel]         [Visible Content Channel]
<think>                     Final markdown text
Step-by-step logic          User response
</think>
        │                           │
        ▼                           ▼
IPC: "ai_stream_chunk"      IPC: "ai_stream_chunk"
{ isThought: true }         { isThought: false }
        │                           │
        └─────────────┬─────────────┘
                      ▼
             [Vue 3 Desktop UI]
  ┌───────────────────────────────────────┐
  │ ▼ Reasoning Process (1.2s, 142 tokens) │
  │   - Analyzing intent                  │
  │   - Querying memory facts             │
  │ ───────────────────────────────────── │
  │ Here is the answer to your request... │
  └───────────────────────────────────────┘
```

#### 2.3 Concrete Implementation

```rust
// liva-native-core/src/llm/stream_splitter.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "text", rename_all = "snake_case")]
pub enum StreamPiece {
    /// Internal reasoning token piece (from inside <think>...</think>).
    Reasoning(String),
    /// User-visible content piece.
    Content(String),
    /// Control signal / heartbeat.
    Heartbeat,
}

pub struct ReasoningStreamSplitter {
    in_reasoning: bool,
    pending: String,
}

impl ReasoningStreamSplitter {
    pub fn new(starts_in_reasoning: bool) -> Self {
        Self {
            in_reasoning: starts_in_reasoning,
            pending: String::new(),
        }
    }

    /// Process an incoming raw token piece and produce typed stream pieces.
    pub fn process_chunk(&mut self, chunk: &str) -> Vec<StreamPiece> {
        self.pending.push_str(chunk);
        let mut pieces = Vec::new();

        const OPEN_TAGS: &[&str] = &["<think>", "<thought>", "<analysis>", "<|channel|>analysis<|message|>"];
        const CLOSE_TAGS: &[&str] = &["</think>", "</thought>", "</analysis>", "<|channel|>final<|message|>"];

        loop {
            if self.in_reasoning {
                if let Some((pos, tag_len)) = find_earliest_tag(&self.pending, CLOSE_TAGS) {
                    let reasoning_text = self.pending[..pos].to_string();
                    if !reasoning_text.is_empty() {
                        pieces.push(StreamPiece::Reasoning(reasoning_text));
                    }
                    self.pending.drain(..pos + tag_len);
                    self.in_reasoning = false;
                    continue;
                }
                
                let safe_len = safe_emit_len(&self.pending, CLOSE_TAGS);
                if safe_len > 0 {
                    let emit = self.pending[..safe_len].to_string();
                    pieces.push(StreamPiece::Reasoning(emit));
                    self.pending.drain(..safe_len);
                }
                break;
            } else {
                if let Some((pos, tag_len)) = find_earliest_tag(&self.pending, OPEN_TAGS) {
                    let content_text = self.pending[..pos].to_string();
                    if !content_text.is_empty() {
                        pieces.push(StreamPiece::Content(content_text));
                    }
                    self.pending.drain(..pos + tag_len);
                    self.in_reasoning = true;
                    continue;
                }

                let safe_len = safe_emit_len(&self.pending, OPEN_TAGS);
                if safe_len > 0 {
                    let emit = self.pending[..safe_len].to_string();
                    pieces.push(StreamPiece::Content(emit));
                    self.pending.drain(..safe_len);
                }
                break;
            }
        }

        pieces
    }

    /// Flush remaining buffer at end of generation.
    pub fn finish(&mut self) -> Vec<StreamPiece> {
        let mut pieces = Vec::new();
        if !self.pending.is_empty() {
            if self.in_reasoning {
                pieces.push(StreamPiece::Reasoning(std::mem::take(&mut self.pending)));
            } else {
                pieces.push(StreamPiece::Content(std::mem::take(&mut self.pending)));
            }
        }
        pieces
    }
}

fn find_earliest_tag(text: &str, tags: &[&str]) -> Option<(usize, usize)> {
    let lower = text.to_ascii_lowercase();
    tags.iter()
        .filter_map(|&tag| lower.find(tag).map(|idx| (idx, tag.len())))
        .min_by_key(|(idx, _)| *idx)
}

fn safe_emit_len(text: &str, tags: &[&str]) -> usize {
    let lower = text.to_ascii_lowercase();
    let max_tag_len = tags.iter().map(|t| t.len()).max().unwrap_or(0);
    let check_suffix_len = lower.len().min(max_tag_len.saturating_sub(1));
    
    for i in (1..=check_suffix_len).rev() {
        let suffix = &lower[lower.len() - i..];
        if tags.iter().any(|t| t.starts_with(suffix)) {
            return lower.len() - i;
        }
    }
    lower.len()
}
```

---

### RFC 3: Modular Dynamic Prompt Assembly & Token Budget Subsystem

#### 3.1 Objective
Replace static prompt formatting with a priority-ranked **Dynamic Prompt Assembly Pipeline** in `liva-native-core::llm::prompt_assembly` that guarantees strict compliance with context window budgets ($N_{\text{ctx}}$).

#### 3.2 Priority Ranking & Implementation

```rust
// liva-native-core/src/llm/prompt_assembly.rs

use crate::llm::ChatMessage;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SlicePriority {
    /// Highest: System Persona & Core Invariants (never dropped).
    P0_CorePersona = 0,
    /// Active Tools & Schema Definitions (compact format).
    P1_ToolSchemas = 1,
    /// Immediate User Turn & Direct Instructions.
    P2_CurrentTurn = 2,
    /// Relevant L2/L3 Memory Facts retrieved via vector search.
    P3_RecalledMemories = 3,
    /// Short-term conversation history (LIFO eviction).
    P4_ConversationHistory = 4,
}

pub struct PromptSlice {
    pub priority: SlicePriority,
    pub message: ChatMessage,
    pub estimated_tokens: usize,
}

pub struct DynamicPromptAssembler {
    n_ctx: usize,
    reserve_completion: usize,
    slices: Vec<PromptSlice>,
}

impl DynamicPromptAssembler {
    pub fn new(n_ctx: usize, reserve_completion: usize) -> Self {
        Self {
            n_ctx,
            reserve_completion,
            slices: Vec::new(),
        }
    }

    pub fn add_slice(&mut self, priority: SlicePriority, message: ChatMessage) {
        let estimated_tokens = (message.content.len() / 3) + 4;
        self.slices.push(PromptSlice {
            priority,
            message,
            estimated_tokens,
        });
    }

    /// Assemble messages under strict budget constraints.
    pub fn assemble(mut self) -> Result<Vec<ChatMessage>, String> {
        let max_prompt_budget = self.n_ctx.saturating_sub(self.reserve_completion);
        if max_prompt_budget < 256 {
            return Err(format!("Context budget {} too small for completion", self.n_ctx));
        }

        self.slices.sort_by_key(|s| s.priority);

        let mut accumulated_tokens = 0;
        let mut accepted_slices = Vec::new();

        for slice in self.slices {
            if accumulated_tokens + slice.estimated_tokens <= max_prompt_budget {
                accumulated_tokens += slice.estimated_tokens;
                accepted_slices.push(slice);
            } else if slice.priority == SlicePriority::P0_CorePersona {
                return Err("Core persona exceeds total context window budget!".to_string());
            } else {
                tracing::warn!(
                    "Prompt slice with priority {:?} dropped due to token budget ({} / {})",
                    slice.priority, accumulated_tokens + slice.estimated_tokens, max_prompt_budget
                );
            }
        }

        let mut final_messages = Vec::with_capacity(accepted_slices.len());
        for slice in accepted_slices {
            final_messages.push(slice.message);
        }

        Ok(final_messages)
    }
}
```

---

### RFC 4: Automated Benchmark & Evaluation Harness (`LIVA-Eval`)

#### 4.1 Objective
Create a unified benchmark harness `liva-native-core/src/bin/liva_eval.rs` inspired by DeepSeek Harness benchmark runners (`DSBench`, `TerminalBench`) to evaluate model quality, TTFT, TPS, and tool calling reliability across local GGUF models and Cloud APIs.

#### 4.2 Mathematical Metrics

$$\text{TTFT}_{\text{raw}} = t(\text{first token received}) - t(\text{request dispatched})$$

$$\text{TTFT}_{\text{visible}} = t(\text{first non-reasoning token}) - t(\text{request dispatched})$$

$$\text{TPS} = \frac{N_{\text{visible tokens}} - 1}{t(\text{completion}) - t(\text{first visible token})}$$

$$\text{Tool Call Accuracy} = \frac{\text{Correct Tool Selection \& Valid Arguments}}{\text{Total Test Invocations}}$$

$$\text{Schema Compliance Rate} = \frac{\text{Calls matching JSON Schema}}{\text{Total Tool Calls}}$$

#### 4.3 Benchmark Test Corpus Structure

```json
{
  "benchmark_suite": "liva_core_v1",
  "test_cases": [
    {
      "id": "tc_001_smarthome_light_on",
      "category": "tool_calling",
      "input": "bật đèn phòng khách giúp mình",
      "expected": {
        "intent": "tool_call",
        "tool_name": "control_smarthome",
        "arguments": {
          "device": "light",
          "command": "on"
        }
      }
    },
    {
      "id": "tc_002_vault_search",
      "category": "vault_rag",
      "input": "tìm trong ghi chú xem hôm qua mình đã họp với ai",
      "expected": {
        "intent": "tool_call",
        "tool_name": "search_vault",
        "arguments": {
          "query": "họp"
        }
      }
    },
    {
      "id": "tc_003_pure_conversation",
      "category": "chat_general",
      "input": "hôm nay thời tiết đẹp quá nhỉ",
      "expected": {
        "intent": "no_tool"
      }
    }
  ]
}
```

---

## 5. Implementation Roadmap & Verification Gates

1. **M1 (Knowledge Base & Spec)**: Complete architectural note in Obsidian Vault with 100% `validate-vault.ts` compliance.
2. **M2 (Scoped Tool Registry & Event Stream)**: Implement `ScopedToolRegistry`, `SessionEventStream`, and `ToolScope` in `liva-native-core`.
3. **M3 (Streaming CoT & Prompt Assembly)**: Implement `ReasoningStreamSplitter`, `DynamicPromptAssembler`, and Tauri IPC `isThought` streaming to Vue 3.
4. **M4 (Automated Evaluation Harness)**: Deploy `liva_eval` binary and run baseline evaluations across local GGUF models and Cloud APIs.
5. **M5 (E2E Integration & Verification)**: Run full integration test suite, cargo clippy zero-warning gate, and adversarial stress tests.
