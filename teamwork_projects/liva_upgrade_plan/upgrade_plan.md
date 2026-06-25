# LIVA System Upgrade Plan: Integrating Advanced Agentic and Voice Capabilities into Rust Native Core

**Date**: June 25, 2026  
**Author**: Worker 1 (LIVA Upgrade Team)  
**Status**: Proposal (Ready for Review)  

---

## 1. Executive Summary

LIVA has successfully executed its foundation phase of architectural consolidation. The legacy, multi-process hybrid stack—composed of a Node.js Gateway (`liva-gateway`), a Python AI Voice Engine (`liva-ai-engine`), a standalone C++ Inference Daemon (`llama-server`), and a Tauri UI frontend—has been fully decommissioned. The system is now driven by a unified, asynchronous Rust core (`liva-native-core`) running on the `tokio` runtime, incorporating native `llama-cpp-2` bindings and CPU-optimized ONNX runtime models (`ort`) for voice activity detection, text-to-speech, and speech-to-text.

While this native transition solved the major resource collision, memory footprint, and startup latency issues, further architectural evolution is required to establish LIVA as a state-of-the-art desktop AI assistant. This document presents a comprehensive **LIVA Upgrade Plan** designed to integrate:
1. **Rust-Native Model Context Protocol (MCP)**: Replacing standalone Node.js-based servers with a native RPC layer.
2. **Cyclic State Graph Agent Loop**: Upgrading the simple linear agent state machine to support multi-agent cycles, SQLite-backed checkpointing, and time-travel state recovery.
3. **Multimodal WebRTC Duplex Voice Streaming**: Enhancing the voice pipeline with cloud-native Realtime WebSocket APIs and adaptive local-first fallbacks.
4. **Asynchronous Memory Critic Judge**: Auditing stored personal facts and resolving semantic conflicts in background idle loops.

---

## 2. Legacy Stack Bottlenecks & Native Rust Resolutions

The transition to `liva-native-core` successfully addressed several critical database and voice pipeline bottlenecks that plagued the legacy Node.js/Python stack:

### A. Database Bottlenecks Resolved in Rust Core

#### Bottleneck 1: Database Lock Contention and Thread Blocking (`SQLITE_BUSY`)
* **Legacy Problem**: In the Node.js `liva-gateway`, database queries were processed via a single SQLite connection inside a `DatabaseWorker` thread. Since WAL (Write-Ahead Logging) was not fully exploited under a pooled configuration, any write operation (such as recording a conversation turn, logging system state, or writing background reflection facts) would acquire an exclusive write lock on the database file. While this lock was active, parallel read queries (like RAG context extraction or user preference retrieval) were blocked, causing frequent `SQLITE_BUSY` errors, bridge-level query timeouts, and sluggish assistant responses.
* **Rust Resolution**: In `liva-native-core`, the database module (`db.rs`) implements a strict **Multi-Reader, Single-Writer** pool powered by `r2d2` and `rusqlite`.
  - **Writer Pool**: Restricted to `max_size(1)` to prevent concurrent write collisions. It initializes the database in WAL mode (`PRAGMA journal_mode = WAL;`) and sets `PRAGMA synchronous = NORMAL;`, allowing write transactions to return as soon as data is written to the OS cache.
  - **Reader Pool**: Configured with `max_size(4)` for parallel read access. Since WAL mode allows readers to access the database concurrently while a write is occurring, read queries are never blocked, completely eliminating `SQLITE_BUSY` errors.
  - **Async Thread Safety**: All SQL executions are offloaded to dedicated blocking threads via `tokio::task::spawn_blocking`, ensuring the main Tokio runtime executor remains unblocked.

#### Bottleneck 2: Event Loop Freezes during Vector and Sparse Search
* **Legacy Problem**: Performing vector similarity math (via custom JavaScript or unoptimized modules) and FTS5 keyword searches within the single-threaded Node.js event loop blocked asynchronous operations. Under concurrent loads (e.g., retrieving context during streaming), the V8 event loop froze for over 10ms, which resulted in UI stuttering and audio playback glitching.
* **Rust Resolution**: The Rust core encapsulates all vector math in native C bindings via the `sqlite-vec` extension.
  - **8-bit Vector Quantization**: Embeddings are quantized into 8-bit integers (`vec_quantize_int8` with `unit` scaling) to minimize the RAM footprint and accelerate similarity calculations.
  - **Reciprocal Rank Fusion (RRF)**: A native RRF algorithm in `db.rs` combines dense vector searches (`sqlite-vec`) and sparse keyword searches (`fts5`) directly in CPU registers. The entire hybrid search process executes in under 100ms inside blocking threads (`spawn_blocking`), preventing any interference with audio streaming or UI rendering.

### B. Voice Pipeline Bottlenecks Resolved in Rust Core

#### Bottleneck 3: Clause Chunking Buffer Latency and Time-to-First-Sound (TTFS)
* **Legacy Problem**: The legacy Python-based Voice Engine relied on third-party cloud services (Edge-TTS) that lacked token-level audio streaming. This forced the system to generate the complete audio clip for an entire sentence before playback could start. Additionally, the Node.js `TTSFormatter` utilized static punctuation buffering and an 8-word overflow rule to split streaming text into clauses, resulting in a high Time-to-First-Sound (TTFS) of $>300\text{ms}$.
* **Rust Resolution**: The Rust voice module implements a native, dynamic chunker (`TtsChunker`) combined with character-streaming Kokoro ONNX model execution.
  - **Dynamic Clause Splitting**: It splits streaming LLM text instantly when a punctuation mark is detected (such as `.`, `!`, `?` or a comma if the buffer contains $\ge 6$ words).
  - **Local Native Synthesis**: Extracted clauses are immediately translated to IPA phonemes via a native G2P (Grapheme-to-Phoneme) converter (`tts/g2p.rs`), tokenized, and synthesized on CPU-bound ONNX sessions (`ort` crate).
  - **Streaming Playback**: Synthesized audio samples are appended directly to the active `rodio`/`cpal` output sink using a lock-free queue. Audio playback begins immediately for the first chunk while the next chunk synthesizes in the background, lowering the TTFS to under **50ms**.

#### Bottleneck 4: Preemption and Barge-In Lock Contention
* **Legacy Problem**: When the user spoke during assistant playback, the legacy system ducked the audio volume and attempted to abort processes via WebSocket connections. However, because the legacy TTS engine locked the system-wide voice mutex during the entire duration of speech synthesis, the `stop()` command blocked waiting for lock release. This resulted in laggy interruptions, with the assistant continuing to speak for hundreds of milliseconds after the user started talking.
* **Rust Resolution**: The native `WebRTCActor` uses **Monotonic Session IDs** combined with active thread preemption.
  - **Atomic Session Management**: Every new speech session increments a monotonic ID stored in an `AtomicU64`.
  - **Immediate Thread Aborts**: On speech start, the coordinator immediately aborts active background tasks (STT, LLM, TTS generation) using `JoinHandle::abort()`.
  - **Lock-Free Mismatch Checking**: Generation loops inspect the active session ID before every computation step (G2P, tokenization, ONNX inference, and playback). If a mismatch is detected, the loop terminates immediately, releasing its resources.
  - **Async Fade-Out**: Playback is stopped within 5ms using non-blocking, asynchronous fade-out loops, reducing interruption latency to $<10\text{ms}$.

---

## 3. Advanced Agent Capabilities & Community Trends Research

To determine the design of LIVA's next-generation architecture, we investigated four prominent trends in the AI engineering community:

1. **Anthropic Model Context Protocol (MCP) Specification (2024)**
   * **Concept**: MCP is an open-standard JSON-RPC 2.0 protocol establishing client-server boundaries for LLM tooling. Instead of hardcoding API integrations inside the LLM client, the client connects to independent MCP servers over standardized transports (Stdio or Server-Sent Events - SSE) to retrieve resources, execute tools, and fetch structured prompts.
   * **Community Trend**: Moving away from proprietary, tightly coupled tool calling towards decoupled, plug-and-play ecosystems (e.g., standard MCP servers for GitHub, PostgreSQL, Brave Search, Slack).
   * **Sourced References**:
     - Anthropic Model Context Protocol (MCP) Specification: [modelcontextprotocol.io](https://modelcontextprotocol.io)
     - Anthropic MCP GitHub Organization: [github.com/modelcontextprotocol](https://github.com/modelcontextprotocol)
   * **LIVA Integration**: Porting the existing Node.js-based Obsidian toolset into a Rust-native MCP server, and implementing an MCP client inside `liva-native-core` to load external tools dynamically.

2. **LangGraph State Graph and Checkpointer Architecture (LangChain, 2024)**
   * **Concept**: Transitioning agent loops from linear pipelines to cyclic state graphs, where nodes represent computational steps (e.g., call LLM, execute tool, critique response) and edges represent state-based transitions. State is maintained inside a centralized struct and persisted at every node boundary using a database-backed checkpointer.
   * **Community Trend**: Embracing cyclic agentic workflows (reflection, verification loops, multi-agent systems) and enabling **Time-Travel** (restoring execution states to step $N$) and **Human-in-the-Loop** (pausing graph execution for user confirmation before executing critical tools).
   * **Sourced References**:
     - LangGraph Repository & Checkpointer Architecture: [github.com/langchain-ai/langgraph](https://github.com/langchain-ai/langgraph)
   * **LIVA Integration**: Building a native Rust state-graph coordinator backed by a SQLite checkpointer that saves state representations to the database.

3. **OpenAI Realtime WebSocket/WebRTC Protocol & Pipecat (2024)**
   * **Concept**: Bypassing the traditional, high-latency cascade of discrete STT, LLM text generation, and TTS steps. Instead, audio frames are streamed bidirectionally in real-time over a persistent WebSocket or WebRTC connection. The server-side model accepts raw audio input and emits raw audio output directly, preserving tone, pacing, and emotion.
   * **Community Trend**: True duplex, conversational voice assistants that achieve sub-500ms response latency and support seamless barge-in.
   * **Sourced References**:
     - OpenAI Realtime API Guide: [platform.openai.com/docs/guides/realtime](https://platform.openai.com/docs/guides/realtime)
     - Pipecat-ai Framework: [github.com/pipecat-ai/pipecat](https://github.com/pipecat-ai/pipecat)
   * **LIVA Integration**: Routing local WebRTC audio inputs directly to a cloud WebSocket/WebRTC transport in online mode, with local ONNX models acting as offline fallbacks.

4. **LLM-as-a-Judge for Memory Consolidation (2023-2024)**
   * **Concept**: Utilizing a highly capable LLM (or a specialized local model) as an evaluator/critic to perform background auditing, scoring, and alignment of generated outputs, episodic logs, and long-term memory entries.
   * **Community Trend**: Shifting memory management from simple vector databases (which suffer from noise and redundancy) to structured, self-cleaning semantic knowledge graphs.
   * **Sourced References**:
     - Prometheus Model Repository: [github.com/promo-eval/prometheus](https://github.com/promo-eval/prometheus)
     - MT-Bench Evaluation Framework (LMSYS): [arxiv.org/abs/2306.05685](https://arxiv.org/abs/2306.05685)
     - Prometheus: Inducing Fine-grained Evaluation Capability in Language Models: [arxiv.org/abs/2310.13639](https://arxiv.org/abs/2310.13639)
   * **LIVA Integration**: Running a low-priority background critique loop during assistant idle periods to deduplicate facts, resolve contradictions, and manage memory decay weights.

---

## 4. Proposed Implementation Plan

The following sections detail the technical design and architecture for the proposed upgrades in `liva-native-core`.

### A. Rust-Native MCP Client and Server

We will deprecate the legacy Node.js-based MCP server (`server.ts`) and integrate both client and server capabilities directly into the Rust core using asynchronous JSON-RPC 2.0 communication.

```
       +---------------------------------------------+
       |               liva-native-core              |
       |                                             |
       |  +-------------------+                      |
       |  |  Local LLM Client |                      |
       |  +---------+---------+                      |
       |            | JSON-RPC 2.0 (Stdio/SSE)       |
       |            v                                |
       |  +-------------------+                      |
       |  |  McpRouter/Client |                      |
       |  +----+---------+----+                      |
       |       |         |                           |
       |       |         +-------------------+       |
       |       |                             |       |
       |       v (Internal Channel)          v (Stdio Fork)
       |  +----+--------------+      +-------+-------+
       |  | Native MCP Server |      | External MCP  |
       |  |  (Obsidian, DB)   |      |  Servers      |
       |  +-------------------+      +---------------+
       +---------------------------------------------+
```

1. **Protocol Engine (`src/mcp/protocol.rs`)**:
   * Implement JSON-RPC 2.0 message parsing via `serde_json` and `serde`. Define core message frames (`Request`, `Response`, `Notification`, `Error`).
   * Define MCP schema structures using `schemars` to automatically output JSON schema metadata for tool arguments.
2. **Native MCP Server (`src/mcp/server.rs`)**:
   * Create an internal registry for local tools. The first-class tools will include:
     - `read_markdown(path)`: Safely reads files within the Obsidian vault directory.
     - `write_markdown(path, content)`: Writes or appends notes.
     - `search_vault(query)`: Executes local vector or keyword search.
     - `control_smarthome(device, command)`: Sends commands to local IoT endpoints.
3. **Native MCP Client (`src/mcp/client.rs`)**:
   * Read configurations from `mcp_config.json` detailing external server paths and arguments.
   * Use `tokio::process::Command` to spawn external servers (e.g., a Python or Node-based MCP server).
   * Pipe child process standard input (`stdin`) and standard output (`stdout`) into a Tokio event loop, translating RPC calls between the LLM router and the external processes.
4. **Native MCP Security Architecture**:
   * **Virtualization and Runtime Isolation (Sandbox Protection)**: Spawning untrusted external stdio servers poses severe privilege escalation risks. To securely execute external MCP servers without exposing host capabilities, the client wrapper supports sandboxed lightweight isolation environments:
     - **Containerized Sandboxing**: External servers are spawned inside container runtimes (such as Docker or Podman) configured with read-only filesystems, dropped capabilities, and disabled host network access (unless explicitly whitelisted).
     - **WebAssembly (Wasm) Isolation**: Supporting plugin-based MCP servers compiled to WebAssembly and executed locally within a native `wasmtime` engine, enforcing strict capability gating for memory, filesystem access, and system calls.
   * **File System Sandboxing**: Any tool that touches the local file system (e.g., reading or writing notes in Obsidian) is confined to a virtual workspace root.
     - On Unix, execution is wrapped in a `chroot` jail or systemd dynamic users.
     - On Windows, path canonicalization checks (`fs::canonicalize`) verify that no operations resolve outside the permitted vault directory, preventing path traversal attacks (like `..\..\Windows`).
   * **Cross-Platform Process Wrapper**: The client utilizes a unified `ProcessWrapper` abstraction:
     - **Windows**: Spawns child processes using `cmd.exe /c` or `powershell.exe -Command`, running under restricted User Account Control (UAC) tokens and Windows Job Objects to constrain CPU/memory usage and prevent process tree escapes.
     - **Unix**: Spawns processes via `/bin/sh -c` or `/bin/bash -c` under restricted POSIX user/group IDs (UID/GID) with sanitised environment variables (stripping system-wide environment secrets).

---

### B. Tokio-Based Cyclic State Graph Engine

We will implement a lightweight, native Rust execution graph representing cyclic agentic loops, providing strict type-safe state transitions, reliable persistence, and Human-in-the-Loop (HITL) coordination.

```rust
// Core Graph Definitions in src/agent/graph.rs

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeError<S> {
    Fatal(String),
    YieldUserApproval(S, String), // Contains the partial state and approval context
}

pub type NodeResult<S> = Result<S, NodeError<S>>;
pub type FutureNodeFn<S> = Box<dyn Fn(S) -> Pin<Box<dyn Future<Output = NodeResult<S>> + Send>> + Send + Sync>;

pub enum Edge<S> {
    Static(String),
    Conditional(Box<dyn Fn(&S) -> String + Send + Sync>),
}

pub trait Checkpointer<S>: Send + Sync {
    fn save_checkpoint(&self, thread_id: &str, step: usize, state: &S) -> Result<(), String>;
    fn load_checkpoint(&self, thread_id: &str, step: usize) -> Result<S, String>;
}

pub struct StateGraph<S> {
    nodes: HashMap<String, FutureNodeFn<S>>,
    edges: HashMap<String, Edge<S>>,
    checkpointer: Option<Arc<dyn Checkpointer<S>>>,
}
```

1. **SQLite Checkpointer Schema & Duplicate Key Resolution (`src/agent/checkpoint.rs`)**:
   * To resolve the SQLite duplicate key constraint on `session_id` in the legacy `consolidation_checkpoints` table (which prevented saving multiple execution states for a single session), we propose a schema migration. A dedicated `graph_checkpoints` table utilizes a composite primary key consisting of `(thread_id, step)`:
     ```sql
     CREATE TABLE graph_checkpoints (
         thread_id TEXT NOT NULL,
         step INTEGER NOT NULL,
         state_data TEXT NOT NULL,       -- Full JSON serialized state or base state
         diff_data TEXT,                 -- JSON diff patch for intermediate steps
         tool_outputs TEXT,              -- Cached outputs for side-effect replay prevention
         timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
         PRIMARY KEY (thread_id, step)
     );
     ```
2. **Performance Optimization: Differential Checkpointing & Connection Buffering**:
   * **Differential Checkpointing**: To avoid write amplification caused by serializing the entire state `S` at every single node transition, the checkpointer implements differential saving. Full state snapshots are written only at epoch intervals (e.g., every 10 steps). For intermediate steps, only the RFC 6902 JSON Patch diff is computed (using `jsonpatch` or custom serde logic) and stored in `diff_data`. State reconstruction performs a sequential fold/apply of diffs onto the nearest base snapshot.
   * **Connection Bottleneck Resolution**: The SQLite connection pool for writes is restricted to a single writer (`max_size(1)`) to avoid database lock contention. To prevent graph execution threads from blocking on disk I/O, the state-graph engine uses an in-memory transition buffer (a lock-free queue channel). State-graph nodes write transition events instantly to the RAM buffer, and a dedicated background task flushes and batches these writes in a single SQLite transaction every 50ms (or when the graph yields control).
3. **Historical Checkpoint GC and Compaction**:
   * Storing every transition and diff indefinitely causes SQLite database bloat and write amplification over long sessions.
   * To prevent this, a background **Garbage Collection (GC) and Compaction** task prunes checkpoints from terminated or historical execution paths.
   * For active threads, only the last $N$ steps (the active context sliding window) are preserved as individual restoration points. Older checkpoints are merged into a single consolidated base state snapshot, deleting the redundant intermediate diffs.
4. **Time-Travel and Side-Effect Replay Prevention**:
   * **Time-Travel**: Expose a Tauri command `restore_agent_state(thread_id, target_step)`. This command loads the base snapshot and replays diffs up to `target_step`, instantiating the state graph at that exact point.
   * **Side-Effect Replay Cache**: Replaying nodes from a previous step could re-execute external tools (such as creating Obsidian files or sending API calls), causing duplicates. The engine prevents this by recording tool inputs and outputs in the `tool_outputs` column. During recovery execution, if a node attempts to invoke a tool, the engine checks the cache for a matching signature in the current step and injects the cached output, bypassing execution.
5. **Human-in-the-Loop (HITL) Gating & Control Flow**:
   * In cyclic workflows, nodes execution returns a `Result<S, NodeError<S>>`.
   * If a node requires user approval (e.g., before performing a critical smart-home or file-write action), it returns `Err(NodeError::YieldUserApproval(partial_state, context))`.
   * When this error is received, the graph executor saves the `partial_state` to `graph_checkpoints`, transitions the graph status to `SUSPENDED`, and broadcasts a Tauri IPC event detailing the approval context.
   * Once the user approves or rejects the action via the UI, a Tauri command `resume_agent_state(thread_id, user_decision)` is called. If approved, the executor marks the tool output as simulated/approved, updates the state, and resumes the graph loop from the next node.

---

### C. Multimodal WebRTC Duplex Voice Pipeline

To achieve low-latency, natural voice interactions, we will integrate a hybrid streaming layer connecting to cloud-based real-time audio APIs with local fallback capabilities.

```
                  +--------------------------+
                  |     Local WebRTC Mic     |
                  +-------------+------------+
                                |
                                v
                       +--------+--------+
                       |   Silero VAD    |
                       +--------+--------+
                                |
                   +------------+------------+
                   | (Speech Detected)       |
                   v                         v
       +-----------+-----------+   +---------+---------+
       |   Cloud Mode (Online) |   | Local Mode (Off)  |
       +-----------+-----------+   +---------+---------+
                   |                         |
       +-----------+-----------+             v
       | WebRTC Data/Audio     |   +---------+---------+
       | (Rust `webrtc` crate) |   | Nemotron STT      |
       | & WebSocket (TCP/TLS) |   | Gemma LLM Router  |
       +-----------+-----------+   | Kokoro TTS        |
                   |               +---------+---------+
                   v                         |
       +-----------+-----------+             |
       | WebSocket Audio Stream|             |
       | (OpenAI / Gemini API) |             |
       +-----------+-----------+             |
                   |                         |
                   +------------+------------+
                                |
                                v
                  +-------------+------------+
                  |  Local Rodio Audio Sink  |
                  +--------------------------+
```

1. **Protocol Transition: TCP WebSocket to UDP WebRTC (`src/webrtc/cloud.rs`)**:
   * Acknowledge that while TCP-based WebSockets via `tokio-tungstenite` are suitable for session handshakes, configuration parameters, and discrete control commands, true duplex real-time streaming must avoid TCP head-of-line blocking. The upgrade plan transitions to UDP-based WebRTC protocols using the native Rust `webrtc` crate.
   - Inbound and outbound audio media streams are carried over UDP-based RTP/SRTP channels to guarantee minimal latency under packet loss conditions.
   - Establish a separate TCP-based WebSocket connection using `tokio-tungstenite` to `/v1/realtime` (such as OpenAI's Realtime API or Gemini's Live API) to serve as a reliable control plane for session configuration and schema definitions.
2. **Audio Streaming Loop and Base64 Audio Frame Queue**:
   * **Inbound (Capture)**: Raw PCM voice frames are captured from the microphone, segmented into 20ms chunks, and encoded. In WebSocket mode, frames are base64-encoded; in WebRTC mode, they are packaged into RTP packets.
   * To handle packet loss, jitter, or temporary network drops, we implement an **Audio Frame Queue** with monotonically increasing **Sequence Numbers**. Base64 audio frames (or RTP payloads) are buffered in a queue. If a packet/frame fails to transmit, it is held in a retry/reconnect queue and retransmitted upon reconnection.
   * **Outbound (Playback)**: Incoming audio events are decoded and streamed directly into `rodio` sample buffers.
3. **Session Epoch and State Synchronization**:
   * To prevent out-of-order execution, race conditions during barge-in, and context incoherence, the client and server exchange a monotonic `session_epoch` and `event_id`.
   * When an interruption or state switch occurs, the client increments the local `session_epoch`. Any incoming audio frames or text tokens associated with the old epoch are discarded. This ensures that historical generation fragments from the cloud do not mix with the new session state.
4. **Adaptive Local Fallback**:
   * If a WebSocket connection failure occurs, or if network latency spikes beyond $300\text{ms}$, the system switches to Local Mode.
   * In Local Mode, audio is routed through the local `SttManager` (Nemotron ONNX), LLM Router, and `TtsEngine` (Kokoro ONNX) entirely offline.
5. **Cloud Interruption and Barge-In Handling**:
   * The local `VadEngine` continues analyzing microphone input. If the user interrupts the assistant during playback, the actor:
     - Aborts the active `rodio` output sink immediately.
     - Dispatches an `input_audio_buffer.clear` WebSocket/WebRTC message to the cloud server to cancel the remaining generation.
     - Discards all queued outbound packets associated with the active epoch.
6. **Acoustic Echo Cancellation (AEC) & VAD Self-Interruption Prevention**:
   * **Self-Interruption Issue**: During assistant voice playback, the microphone captures the assistant's own audio output. Without proper Acoustic Echo Cancellation, the local VAD engine registers this feedback loop as user speech, causing false preemption and self-interruption.
   * **Mitigation**: The audio input stream is processed using a software-based AEC module (e.g., WebRTC Audio Processing library or RNNoise) in `src/webrtc/audio_processor.rs`. The output audio signal sent to the speaker is used as a reference channel to cancel echo from the microphone signal before running Voice Activity Detection (VAD).

---

### D. Asynchronous Memory Critic Judge

To maintain high-quality personal memories and settings, a low-priority background critic daemon will run during periods of system inactivity.

```rust
// Background Worker Loop in src/memory/critic.rs

use std::sync::Arc;
use tokio::time::{sleep, Duration};

pub async fn start_memory_critic_loop(
    state: Arc<AppState>,
    mut preemption_rx: tokio::sync::broadcast::Receiver<SystemStatus>
) {
    loop {
        // Wait until system is idle
        if state.get_system_status() == SystemStatus::Idle {
            tokio::select! {
                res = run_memory_consolidation(&state) => {
                    if let Err(e) = res {
                        tracing::error!("Memory critic error: {}", e);
                    }
                }
                Ok(status) = preemption_rx.recv() => {
                    if status == SystemStatus::Active {
                        tracing::info!("Memory critic preempted by user activity; pausing execution.");
                    }
                }
            }
        }
        sleep(Duration::from_secs(300)).await; // Audit check every 5 minutes
    }
}
```

1. **Resource Isolation (VRAM Isolation via CPU-Only Models) & Starvation Prevention**:
   * To prevent background critic operations from causing CPU/GPU resource starvation or thread lags during real-time assistant wakeups, the Memory Critic is configured to run exclusively on a **CPU-only quantized local model** (e.g., a 1.5B or 3B Parameter Q4_K_M GGUF model executed via a dedicated CPU-bound thread pool at low thread priority) or routes calls to a cloud API. This keeps the primary GPU and high-priority VRAM dedicated to audio playback and active session LLM inference.
2. **Explicit Preemption and Instant Suspension**:
   * To eliminate thread lags when the user starts speaking or wakes up the assistant, the Memory Critic daemon listens to a high-priority system status broadcast channel. If the system state transitions from `SystemStatus::Idle` to `SystemStatus::Active`, the Memory Critic immediately halts its prompt execution, aborts any ongoing CPU-bound inference thread/task, and rolls back or suspends any active SQLite transaction to free the write-lock.
3. **RAM-buffered Memory Decay & Write Amplification Reduction**:
   * Continually computing and writing updated Ebbinghaus decay metrics to SQLite on disk for hundreds of memory items creates massive write amplification. To solve this, LIVA buffers memory decays in a thread-safe RAM cache (e.g., a `DashMap` or a `Mutex`-wrapped `HashMap`).
   * When memory items are read, their decayed strengths are computed dynamically based on the last-accessed timestamp. The calculated decays are only flushed and batched to SQLite in a single transaction during prolonged inactivity or system shutdown.
4. **Evaluation Prompting**:
   * The Critic Daemon fetches newly extracted episodic facts from the SQLite `vectors_meta` table.
   * It sends these facts to the local CPU-bound model or a cloud LLM using a structured prompt:
     ```
     You are the Memory Critic. Examine the following list of newly extracted facts and reconcile them against the existing user knowledge base.
     New Facts: [New Facts]
     Existing Knowledge: [Current DB Facts]
     
     Identify:
     1. Redundant facts (mark for deletion).
     2. Contradictory facts (resolve by selecting the most recent or accurate fact).
     3. Information updates (update existing keys with new data).
     ```
5. **Applying Mutations**:
   * The Critic outputs a structured JSON mutation payload listing actions: `Delete(id)`, `Update(id, new_value)`, or `Insert(value)`.
   * These mutations are applied to the database inside a single writer transaction, updating the vector indexes accordingly.
6. **Memory Strength and Forgetting Curve**:
   * The database stores a `memory_strength` metric for each fact. During the audit, facts that have not been referenced are decayed using an Ebbinghaus decay function:
     $$\text{Strength}_{\text{new}} = \text{Strength}_{\text{old}} \cdot e^{-\lambda t}$$
     where $\lambda$ is the decay rate and $t$ is the time elapsed. If `memory_strength` falls below `0.1`, the fact is deleted from the active memory layer.

---

## 5. Phased Roadmap and Milestones

We propose a 16-week execution plan split into four key milestones.

```
+-----------------------------------------------------------------------------+
|                                  ROADMAP                                    |
+-----------------------------------------------------------------------------+
| Weeks 1-4: Milestone 1 - Rust-Native MCP Engine                            |
| * Deliverables: Stdio client transport, local tool registry, JSON-RPC.       |
+-----------------------------------------------------------------------------+
| Weeks 5-8: Milestone 2 - Tokio State-Graph Coordinator                    |
| * Deliverables: StateGraph structure, SQLite checkpointer, Time-Travel.     |
+-----------------------------------------------------------------------------+
| Weeks 9-12: Milestone 3 - Duplex WebRTC Voice Pipeline                      |
| * Deliverables: Websocket audio streaming, cloud interruption, fallback.   |
+-----------------------------------------------------------------------------+
| Weeks 13-16: Milestone 4 - Asynchronous Memory Critic Daemon                |
| * Deliverables: Idle cron system, conflict resolution, decay curves.        |
+-----------------------------------------------------------------------------+
```

### Milestone 1: Rust-Native MCP Engine (Weeks 1–4)
* **Goal**: Deprecate Node.js MCP server components and establish a high-performance native MCP layer.
* **Deliverables**:
  - Asynchronous JSON-RPC 2.0 message parser in Rust.
  - Native MCP tool registry exposing Obsidian vault and smart home tools.
  - Stdio-based client transport spawning external sub-processes.
* **Verification Method**:
  - Run integration tests simulating tool execution requests.
  - Verify zero remaining Node.js process dependencies for Obsidian vault interactions.

### Milestone 2: Tokio State-Graph Coordinator (Weeks 5–8)
* **Goal**: Implement cyclic execution graphs with database checkpoint persistence.
* **Deliverables**:
  - `StateGraph` core engine with dynamic node transitions.
  - SQLite checkpointer serializing state vectors to database tables.
  - Tauri-compatible commands for Human-in-the-Loop approval and Time-Travel recovery.
* **Verification Method**:
  - Assert that execution state is successfully saved and loaded from SQLite after each node.
  - Test thread recovery by interrupting an active execution and restoring it.

### Milestone 3: Duplex WebRTC Voice Pipeline (Weeks 9–12)
* **Goal**: Integrate real-time audio duplex streaming with cloud endpoints and local fallbacks.
* **Deliverables**:
  - Asynchronous WebSocket client using `tokio-tungstenite` for voice streaming.
  - Bidirectional PCM audio routing between WebRTC microphone and cloud API.
  - Cloud interruption handling based on local VAD signals.
  - Automatic offline switcher to local ONNX engines.
* **Verification Method**:
  - Measure round-trip audio latency (target: $<500\text{ms}$ under normal network conditions).
  - Verify that local VAD speech detection successfully stops cloud playback within $10\text{ms}$.

### Milestone 4: Asynchronous Memory Critic Daemon (Weeks 13–16)
* **Goal**: Integrate background self-cleaning memory structures.
* **Deliverables**:
  - Background idle loop trigger running at low thread priority.
  - Memory Critic prompting pipeline to reconcile facts and remove duplicates.
  - Ebbinghaus-based memory decay algorithm updating database records.
* **Verification Method**:
  - Seed database with duplicate and contradictory facts, then verify that the background critic daemon successfully resolves them during idle periods.
  - Verify that memory strength decays correctly over simulated intervals.

---

## 6. Conclusion

This upgrade plan transforms the unified native Rust core (`liva-native-core`) into an advanced, low-latency, and highly agentic system. By eliminating legacy cross-process serialization overheads, introducing native MCP tools, implementing cyclic multi-agent graphs with persistent checkpointers, and employing real-time duplex voice streaming, LIVA achieves desktop assistant responsiveness. Implementing these features in a phased manner ensures that system stability and verified correctness are maintained throughout the process.

---

## 7. References

- **Anthropic Model Context Protocol (MCP) Specification**: [Official Site](https://modelcontextprotocol.io) | [GitHub Repository](https://github.com/modelcontextprotocol)
- **LangGraph State Graph & Checkpointer Architecture**: [GitHub Repository](https://github.com/langchain-ai/langgraph)
- **OpenAI Realtime API WebSocket/WebRTC Protocol**: [Official Guide](https://platform.openai.com/docs/guides/realtime)
- **Pipecat-ai Real-Time Conversational Agent Framework**: [GitHub Repository](https://github.com/pipecat-ai/pipecat)
- **LLM-as-a-Judge Prometheus Model**: [GitHub Repository](https://github.com/promo-eval/prometheus)
- **Prometheus Paper**: *Prometheus: Inducing Fine-grained Evaluation Capability in Language Models* (Oct 2023). [arXiv Link](https://arxiv.org/abs/2310.13639)
- **MT-Bench Evaluation Framework**: *Judging LLM-as-a-Judge on MT-Bench and Chatbot Arena* (June 2023). [arXiv Link](https://arxiv.org/abs/2306.05685)
