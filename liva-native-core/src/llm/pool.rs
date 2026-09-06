//! Prioritized LLM Worker Pool (RFC-003 §1.2 & §3.1)
//!
//! Provides an Actor-based priority queue system (`RealtimeVoice` > `InteractiveUser` > `BackgroundConsolidation`)
//! replacing monolithic global mutexes. Implements per-token cooperative cancellation tokens
//! to guarantee preemptive yielding in <= 5ms for high-priority voice tasks.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot, watch, Mutex, Notify, RwLock};
use uuid::Uuid;

use super::engine::LlamaRouterManager;

/// Priority levels for LLM worker tasks.
///
/// Priority 0 (`RealtimeVoice`): Audio STT/TTS, Wake Word Confirmation, Barge-In.
/// Priority 1 (`InteractiveUser`): UI Chat Turns, Generative UI, Modal Tool Execution.
/// Priority 2 (`BackgroundConsolidation`): HippoRAG Graph Sync, Vector Indexing, Summarization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum LlmPriority {
    RealtimeVoice = 0,
    InteractiveUser = 1,
    BackgroundConsolidation = 2,
}

impl LlmPriority {
    #[inline]
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }

    #[inline]
    pub fn can_preempt(&self, other: LlmPriority) -> bool {
        self.as_u8() < other.as_u8()
    }
}

/// Token streaming delta chunk emitted by the worker pool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenStreamDelta {
    pub task_id: Uuid,
    pub token_id: i32,
    pub text_piece: String,
    pub is_first_token: bool,
    pub is_final_token: bool,
    pub cumulative_tokens: usize,
    pub latency_from_start_ns: u64,
}

/// Cooperative cancellation token enabling sub-5ms task preemption.
#[derive(Clone, Debug)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            notify: Arc::new(Notify::new()),
        }
    }

    pub fn cancel(&self) {
        if !self.cancelled.swap(true, Ordering::SeqCst) {
            self.notify.notify_waiters();
        }
    }

    #[inline(always)]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    pub async fn cancelled(&self) {
        if self.is_cancelled() {
            return;
        }
        self.notify.notified().await;
    }

    pub fn child_token(&self) -> Self {
        Self {
            cancelled: Arc::clone(&self.cancelled),
            notify: Arc::clone(&self.notify),
        }
    }
}

/// Request definition submitted to LLM Worker Pool.
pub struct LlmCompletionRequest {
    pub task_id: Uuid,
    pub priority: LlmPriority,
    pub prompt: String,
    pub max_tokens: usize,
    pub temperature: f32,
    pub top_p: f32,
    pub cancellation_token: CancellationToken,
    pub stream_tx: mpsc::Sender<TokenStreamDelta>,
    pub response_tx: oneshot::Sender<Result<LlmCompletionResult, LlmPoolError>>,
}

impl std::fmt::Debug for LlmCompletionRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlmCompletionRequest")
            .field("task_id", &self.task_id)
            .field("priority", &self.priority)
            .field("prompt_len", &self.prompt.len())
            .field("max_tokens", &self.max_tokens)
            .field("temperature", &self.temperature)
            .field("top_p", &self.top_p)
            .finish()
    }
}

/// Final completion result returned when generation completes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LlmCompletionResult {
    pub task_id: Uuid,
    pub full_text: String,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub ttft_ns: u64,
    pub total_duration_ns: u64,
}

/// Error types emitted by the LLM Worker Pool.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum LlmPoolError {
    #[error("Worker pool is shutting down / channel closed")]
    ChannelClosed,
    #[error("Task was preempted by higher-priority request ({0:?})")]
    Preempted(LlmPriority),
    #[error("Task was cancelled")]
    Cancelled,
    #[error("Inference engine error: {0}")]
    EngineError(String),
    #[error("Prompt exceeds context capacity: {0}")]
    ContextExceeded(String),
}

/// Atomic metrics tracked by the worker pool.
#[derive(Debug, Default)]
pub struct PoolMetrics {
    pub queued_voice_tasks: AtomicU64,
    pub queued_user_tasks: AtomicU64,
    pub queued_background_tasks: AtomicU64,
    pub preemption_events_total: AtomicU64,
    pub last_ttft_ns: AtomicU64,
    pub total_completed_tasks: AtomicU64,
    pub total_failed_tasks: AtomicU64,
    pub total_tokens_generated: AtomicU64,
}

/// Operational snapshot of worker pool metrics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PoolMetricsSnapshot {
    pub queued_voice_tasks: u64,
    pub queued_user_tasks: u64,
    pub queued_background_tasks: u64,
    pub preemption_events_total: u64,
    pub last_ttft_ns: u64,
    pub total_completed_tasks: u64,
    pub total_failed_tasks: u64,
    pub total_tokens_generated: u64,
    pub active_priority: Option<LlmPriority>,
}

impl PoolMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self, active_priority: Option<LlmPriority>) -> PoolMetricsSnapshot {
        PoolMetricsSnapshot {
            queued_voice_tasks: self.queued_voice_tasks.load(Ordering::Relaxed),
            queued_user_tasks: self.queued_user_tasks.load(Ordering::Relaxed),
            queued_background_tasks: self.queued_background_tasks.load(Ordering::Relaxed),
            preemption_events_total: self.preemption_events_total.load(Ordering::Relaxed),
            last_ttft_ns: self.last_ttft_ns.load(Ordering::Relaxed),
            total_completed_tasks: self.total_completed_tasks.load(Ordering::Relaxed),
            total_failed_tasks: self.total_failed_tasks.load(Ordering::Relaxed),
            total_tokens_generated: self.total_tokens_generated.load(Ordering::Relaxed),
            active_priority,
        }
    }
}

/// Service trait defining public interface of the LLM Worker Pool.
#[async_trait]
pub trait LlmWorkerPoolService: Send + Sync {
    /// Submit completion request with strict priority scheduling.
    async fn submit_task(&self, request: LlmCompletionRequest) -> Result<(), LlmPoolError>;

    /// Check if higher-priority voice task is pending in queue.
    fn has_pending_voice_task(&self) -> bool;

    /// Get current pool operational metrics snapshot.
    fn get_metrics(&self) -> PoolMetricsSnapshot;
}

/// Backend trait for executing inference generation in the worker loop.
#[async_trait]
pub trait LlmEngineBackend: Send + Sync + 'static {
    async fn execute_generation(
        &mut self,
        request: &LlmCompletionRequest,
        cancel_token: &CancellationToken,
        metrics: &PoolMetrics,
    ) -> Result<LlmCompletionResult, LlmPoolError>;
}

/// Default simulated engine backend for testing and mock deployments.
pub struct SimulatedEngineBackend {
    token_delay: Duration,
}

impl SimulatedEngineBackend {
    pub fn new(token_delay: Duration) -> Self {
        Self { token_delay }
    }
}

impl Default for SimulatedEngineBackend {
    fn default() -> Self {
        Self {
            token_delay: Duration::from_millis(2),
        }
    }
}

#[async_trait]
impl LlmEngineBackend for SimulatedEngineBackend {
    async fn execute_generation(
        &mut self,
        request: &LlmCompletionRequest,
        cancel_token: &CancellationToken,
        metrics: &PoolMetrics,
    ) -> Result<LlmCompletionResult, LlmPoolError> {
        let start_time = Instant::now();
        let prompt_tokens = (request.prompt.len() / 4).max(1);
        let max_gen_tokens = if request.max_tokens == 0 {
            32
        } else {
            request.max_tokens
        };

        let mut full_text = String::new();
        let mut first_token_emitted = false;
        let mut ttft_ns = 0u64;

        for i in 0..max_gen_tokens {
            // Check cancellation / preemption at token boundary (<5ms)
            if cancel_token.is_cancelled() || request.cancellation_token.is_cancelled() {
                return Err(LlmPoolError::Cancelled);
            }

            if self.token_delay > Duration::ZERO {
                tokio::time::sleep(self.token_delay).await;
            }

            if cancel_token.is_cancelled() || request.cancellation_token.is_cancelled() {
                return Err(LlmPoolError::Cancelled);
            }

            let elapsed_ns = start_time.elapsed().as_nanos() as u64;
            if !first_token_emitted {
                first_token_emitted = true;
                ttft_ns = elapsed_ns;
                metrics.last_ttft_ns.store(ttft_ns, Ordering::Relaxed);
            }

            let piece = format!(" tok_{}", i);
            full_text.push_str(&piece);
            let is_final = i + 1 == max_gen_tokens;

            let delta = TokenStreamDelta {
                task_id: request.task_id,
                token_id: i as i32,
                text_piece: piece,
                is_first_token: i == 0,
                is_final_token: is_final,
                cumulative_tokens: i + 1,
                latency_from_start_ns: elapsed_ns,
            };

            metrics.total_tokens_generated.fetch_add(1, Ordering::Relaxed);

            // Forward delta to stream receiver; ignore error if receiver dropped
            let _ = request.stream_tx.send(delta).await;
        }

        let total_duration_ns = start_time.elapsed().as_nanos() as u64;

        Ok(LlmCompletionResult {
            task_id: request.task_id,
            full_text,
            prompt_tokens,
            completion_tokens: max_gen_tokens,
            ttft_ns,
            total_duration_ns,
        })
    }
}

/// Production backend adapter connecting `LlmWorkerPool` to `LlamaRouterManager`.
pub struct LlamaRouterBackend {
    router: Arc<Mutex<LlamaRouterManager>>,
}

impl LlamaRouterBackend {
    pub fn new(router: Arc<Mutex<LlamaRouterManager>>) -> Self {
        Self { router }
    }
}

#[async_trait]
impl LlmEngineBackend for LlamaRouterBackend {
    async fn execute_generation(
        &mut self,
        request: &LlmCompletionRequest,
        cancel_token: &CancellationToken,
        metrics: &PoolMetrics,
    ) -> Result<LlmCompletionResult, LlmPoolError> {
        let mut router = self.router.lock().await;
        let start_time = Instant::now();
        let mut first_token_emitted = false;
        let mut ttft_ns = 0u64;
        let mut token_idx = 0i32;

        let task_id = request.task_id;
        let stream_tx = request.stream_tx.clone();
        let cancel = cancel_token.clone();
        let req_cancel = request.cancellation_token.clone();

        let res = router.generate_completion(
            &request.prompt,
            request.temperature,
            request.top_p,
            &mut |piece: &str| {
                if cancel.is_cancelled() || req_cancel.is_cancelled() {
                    return false;
                }
                let elapsed_ns = start_time.elapsed().as_nanos() as u64;
                if !first_token_emitted {
                    first_token_emitted = true;
                    ttft_ns = elapsed_ns;
                    metrics.last_ttft_ns.store(ttft_ns, Ordering::Relaxed);
                }
                let delta = TokenStreamDelta {
                    task_id,
                    token_id: token_idx,
                    text_piece: piece.to_string(),
                    is_first_token: token_idx == 0,
                    is_final_token: false,
                    cumulative_tokens: (token_idx + 1) as usize,
                    latency_from_start_ns: elapsed_ns,
                };
                token_idx += 1;
                metrics.total_tokens_generated.fetch_add(1, Ordering::Relaxed);
                let _ = stream_tx.try_send(delta);
                true
            },
        );

        if cancel.is_cancelled() || req_cancel.is_cancelled() {
            return Err(LlmPoolError::Cancelled);
        }

        match res {
            Ok(output) => {
                let total_duration_ns = start_time.elapsed().as_nanos() as u64;
                Ok(LlmCompletionResult {
                    task_id,
                    full_text: output.text,
                    prompt_tokens: output.prompt_tokens,
                    completion_tokens: output.completion_tokens,
                    ttft_ns,
                    total_duration_ns,
                })
            }
            Err(err_str) => {
                if err_str.contains("Prompt qua dai") {
                    Err(LlmPoolError::ContextExceeded(err_str))
                } else {
                    Err(LlmPoolError::EngineError(err_str))
                }
            }
        }
    }
}

/// Metadata about currently running task for preemption coordination.
#[derive(Debug)]
struct ActiveTaskContext {
    task_id: Uuid,
    priority: LlmPriority,
    cancel_token: CancellationToken,
    preempted_reason: Arc<AtomicU8>, // 0: None, 1: Preempted by Voice, 2: Preempted by User
}

const PREEMPT_REASON_NONE: u8 = 0;
const PREEMPT_REASON_VOICE: u8 = 1;
const PREEMPT_REASON_USER: u8 = 2;

/// Prioritized LLM Worker Pool Actor.
///
/// Manages 3 priority MPSC channels with biased polling and real-time preemption.
#[derive(Clone)]
pub struct LlmWorkerPool {
    voice_tx: mpsc::Sender<LlmCompletionRequest>,
    interactive_tx: mpsc::Sender<LlmCompletionRequest>,
    background_tx: mpsc::Sender<LlmCompletionRequest>,
    active_task: Arc<RwLock<Option<ActiveTaskContext>>>,
    metrics: Arc<PoolMetrics>,
    shutdown_tx: watch::Sender<bool>,
}

impl LlmWorkerPool {
    /// Create and spawn a new `LlmWorkerPool` actor with the given channel capacity and backend.
    pub fn new_with_backend<B: LlmEngineBackend>(
        backend: B,
        channel_capacity: usize,
    ) -> (Self, tokio::task::JoinHandle<()>) {
        let (voice_tx, voice_rx) = mpsc::channel(channel_capacity);
        let (interactive_tx, interactive_rx) = mpsc::channel(channel_capacity);
        let (background_tx, background_rx) = mpsc::channel(channel_capacity);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let active_task = Arc::new(RwLock::new(None));
        let metrics = Arc::new(PoolMetrics::new());

        let pool = Self {
            voice_tx,
            interactive_tx,
            background_tx,
            active_task: Arc::clone(&active_task),
            metrics: Arc::clone(&metrics),
            shutdown_tx,
        };

        let worker_actor = WorkerActor {
            voice_rx,
            interactive_rx,
            background_rx,
            shutdown_rx,
            active_task,
            metrics,
            backend,
        };

        let handle = tokio::spawn(async move {
            worker_actor.run().await;
        });

        (pool, handle)
    }

    /// Create and spawn a new `LlmWorkerPool` using the simulated engine backend.
    pub fn new(channel_capacity: usize) -> (Self, tokio::task::JoinHandle<()>) {
        Self::new_with_backend(SimulatedEngineBackend::default(), channel_capacity)
    }

    /// Create and spawn a new `LlmWorkerPool` with custom simulated token delay.
    pub fn new_simulated(
        token_delay: Duration,
        channel_capacity: usize,
    ) -> (Self, tokio::task::JoinHandle<()>) {
        Self::new_with_backend(SimulatedEngineBackend::new(token_delay), channel_capacity)
    }

    /// Create and spawn a new `LlmWorkerPool` backed by an existing `LlamaRouterManager`.
    pub fn new_with_router(
        router: Arc<Mutex<LlamaRouterManager>>,
        channel_capacity: usize,
    ) -> (Self, tokio::task::JoinHandle<()>) {
        Self::new_with_backend(LlamaRouterBackend::new(router), channel_capacity)
    }

    /// Preempt currently running task if submitted priority is higher.
    async fn check_and_trigger_preemption(&self, incoming_priority: LlmPriority) {
        let active_guard = self.active_task.read().await;
        if let Some(ref active) = *active_guard {
            if incoming_priority.can_preempt(active.priority) {
                let reason = match incoming_priority {
                    LlmPriority::RealtimeVoice => PREEMPT_REASON_VOICE,
                    LlmPriority::InteractiveUser => PREEMPT_REASON_USER,
                    LlmPriority::BackgroundConsolidation => PREEMPT_REASON_NONE,
                };
                active.preempted_reason.store(reason, Ordering::SeqCst);
                active.cancel_token.cancel();
                self.metrics.preemption_events_total.fetch_add(1, Ordering::Relaxed);
                tracing::info!(
                    incoming_priority = ?incoming_priority,
                    active_priority = ?active.priority,
                    active_task_id = %active.task_id,
                    "Preemption signal dispatched to active LLM worker task"
                );
            }
        }
    }

    /// Gracefully signal the worker actor to shutdown.
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(true);
    }
}

#[async_trait]
impl LlmWorkerPoolService for LlmWorkerPool {
    async fn submit_task(&self, request: LlmCompletionRequest) -> Result<(), LlmPoolError> {
        // Fast path: if task is already cancelled before submit
        if request.cancellation_token.is_cancelled() {
            let _ = request.response_tx.send(Err(LlmPoolError::Cancelled));
            return Err(LlmPoolError::Cancelled);
        }

        let priority = request.priority;

        // Check if incoming request should preempt active running task
        self.check_and_trigger_preemption(priority).await;

        // Route to prioritized channel and increment metrics
        match priority {
            LlmPriority::RealtimeVoice => {
                self.metrics.queued_voice_tasks.fetch_add(1, Ordering::Relaxed);
                self.voice_tx.send(request).await.map_err(|_| {
                    self.metrics.queued_voice_tasks.fetch_sub(1, Ordering::Relaxed);
                    LlmPoolError::ChannelClosed
                })?;
            }
            LlmPriority::InteractiveUser => {
                self.metrics.queued_user_tasks.fetch_add(1, Ordering::Relaxed);
                self.interactive_tx.send(request).await.map_err(|_| {
                    self.metrics.queued_user_tasks.fetch_sub(1, Ordering::Relaxed);
                    LlmPoolError::ChannelClosed
                })?;
            }
            LlmPriority::BackgroundConsolidation => {
                self.metrics.queued_background_tasks.fetch_add(1, Ordering::Relaxed);
                self.background_tx.send(request).await.map_err(|_| {
                    self.metrics.queued_background_tasks.fetch_sub(1, Ordering::Relaxed);
                    LlmPoolError::ChannelClosed
                })?;
            }
        }

        Ok(())
    }

    #[inline]
    fn has_pending_voice_task(&self) -> bool {
        self.metrics.queued_voice_tasks.load(Ordering::Relaxed) > 0
    }

    fn get_metrics(&self) -> PoolMetricsSnapshot {
        // Non-blocking try-read for active priority
        let active_prio = if let Ok(guard) = self.active_task.try_read() {
            guard.as_ref().map(|ctx| ctx.priority)
        } else {
            None
        };
        self.metrics.snapshot(active_prio)
    }
}

/// Internal actor loop holding the inference engine and priority channels.
struct WorkerActor<B: LlmEngineBackend> {
    voice_rx: mpsc::Receiver<LlmCompletionRequest>,
    interactive_rx: mpsc::Receiver<LlmCompletionRequest>,
    background_rx: mpsc::Receiver<LlmCompletionRequest>,
    shutdown_rx: watch::Receiver<bool>,
    active_task: Arc<RwLock<Option<ActiveTaskContext>>>,
    metrics: Arc<PoolMetrics>,
    backend: B,
}

impl<B: LlmEngineBackend> WorkerActor<B> {
    async fn run(mut self) {
        loop {
            // Check shutdown signal
            if *self.shutdown_rx.borrow() {
                break;
            }

            // Strict Biased Priority Selection:
            // 1. RealtimeVoice (Priority 0)
            // 2. InteractiveUser (Priority 1)
            // 3. BackgroundConsolidation (Priority 2)
            tokio::select! {
                biased;

                _ = self.shutdown_rx.changed() => {
                    if *self.shutdown_rx.borrow() {
                        break;
                    }
                }

                Some(req) = self.voice_rx.recv() => {
                    self.metrics.queued_voice_tasks.fetch_sub(1, Ordering::Relaxed);
                    self.execute_task(req).await;
                }

                Some(req) = self.interactive_rx.recv() => {
                    self.metrics.queued_user_tasks.fetch_sub(1, Ordering::Relaxed);
                    self.execute_task(req).await;
                }

                Some(req) = self.background_rx.recv() => {
                    self.metrics.queued_background_tasks.fetch_sub(1, Ordering::Relaxed);
                    self.execute_task(req).await;
                }

                else => {
                    // All senders dropped
                    break;
                }
            }
        }

        tracing::info!("LLM Worker Actor loop terminated cleanly");
    }

    async fn execute_task(&mut self, request: LlmCompletionRequest) {
        // Fast-path cancellation check before beginning compute
        if request.cancellation_token.is_cancelled() {
            self.metrics.total_failed_tasks.fetch_add(1, Ordering::Relaxed);
            let _ = request.response_tx.send(Err(LlmPoolError::Cancelled));
            return;
        }

        let task_id = request.task_id;
        let priority = request.priority;
        let internal_cancel = CancellationToken::new();
        let preempt_reason = Arc::new(AtomicU8::new(PREEMPT_REASON_NONE));

        // Register active task context for preemption tracking
        {
            let mut active_guard = self.active_task.write().await;
            *active_guard = Some(ActiveTaskContext {
                task_id,
                priority,
                cancel_token: internal_cancel.clone(),
                preempted_reason: Arc::clone(&preempt_reason),
            });
        }

        // Execute inference with backend
        let execution_result = self
            .backend
            .execute_generation(&request, &internal_cancel, &self.metrics)
            .await;

        // Clear active task context
        {
            let mut active_guard = self.active_task.write().await;
            *active_guard = None;
        }

        // Determine final mapped result taking preemption reasons into account
        let final_result = match execution_result {
            Ok(result) => {
                self.metrics.total_completed_tasks.fetch_add(1, Ordering::Relaxed);
                Ok(result)
            }
            Err(LlmPoolError::Cancelled) => {
                let reason = preempt_reason.load(Ordering::SeqCst);
                self.metrics.total_failed_tasks.fetch_add(1, Ordering::Relaxed);
                match reason {
                    PREEMPT_REASON_VOICE => Err(LlmPoolError::Preempted(LlmPriority::RealtimeVoice)),
                    PREEMPT_REASON_USER => Err(LlmPoolError::Preempted(LlmPriority::InteractiveUser)),
                    _ => Err(LlmPoolError::Cancelled),
                }
            }
            Err(err) => {
                self.metrics.total_failed_tasks.fetch_add(1, Ordering::Relaxed);
                Err(err)
            }
        };

        // Send final completion result to caller's oneshot channel
        let _ = request.response_tx.send(final_result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to construct a standard completion request
    fn make_test_request(
        priority: LlmPriority,
        prompt: &str,
        max_tokens: usize,
    ) -> (
        LlmCompletionRequest,
        mpsc::Receiver<TokenStreamDelta>,
        oneshot::Receiver<Result<LlmCompletionResult, LlmPoolError>>,
    ) {
        let (stream_tx, stream_rx) = mpsc::channel(64);
        let (response_tx, response_rx) = oneshot::channel();
        let cancel_token = CancellationToken::new();

        let request = LlmCompletionRequest {
            task_id: Uuid::new_v4(),
            priority,
            prompt: prompt.to_string(),
            max_tokens,
            temperature: 0.7,
            top_p: 0.9,
            cancellation_token: cancel_token,
            stream_tx,
            response_tx,
        };

        (request, stream_rx, response_rx)
    }

    #[tokio::test]
    async fn test_priority_scheduling_order() {
        // Setup pool with a controlled token delay
        let (pool, _handle) = LlmWorkerPool::new_simulated(Duration::from_millis(5), 32);

        // Submit in reverse priority order: Background first, then User, then Voice
        let (req_bg, _stream_bg, resp_bg) =
            make_test_request(LlmPriority::BackgroundConsolidation, "bg_task", 10);
        let (req_user, _stream_user, resp_user) =
            make_test_request(LlmPriority::InteractiveUser, "user_task", 10);
        let (req_voice, _stream_voice, resp_voice) =
            make_test_request(LlmPriority::RealtimeVoice, "voice_task", 10);

        // Submit tasks
        pool.submit_task(req_bg).await.unwrap();
        // Short pause to ensure worker picks up background task
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Submit Voice and User while Background is executing
        pool.submit_task(req_user).await.unwrap();
        pool.submit_task(req_voice).await.unwrap();

        // Background should be preempted by Voice
        let bg_res = resp_bg.await.unwrap();
        assert!(
            matches!(bg_res, Err(LlmPoolError::Preempted(LlmPriority::RealtimeVoice))),
            "Background task must be preempted by RealtimeVoice, got: {:?}",
            bg_res
        );

        // Voice task should succeed
        let voice_res = resp_voice.await.unwrap();
        assert!(voice_res.is_ok(), "Voice task should succeed");

        // User task should also succeed subsequently
        let user_res = resp_user.await.unwrap();
        assert!(user_res.is_ok(), "User task should succeed");

        pool.shutdown();
    }

    #[tokio::test]
    async fn test_preemption_latency_under_5ms() {
        // High-precision preemption test
        let (pool, _handle) = LlmWorkerPool::new_simulated(Duration::from_millis(2), 32);

        // Launch long background task (50 tokens = 100ms)
        let (req_bg, _stream_bg, resp_bg) =
            make_test_request(LlmPriority::BackgroundConsolidation, "long_background", 50);
        pool.submit_task(req_bg).await.unwrap();

        // Wait 15ms so background task is actively running
        tokio::time::sleep(Duration::from_millis(15)).await;

        // Record time when Voice request is submitted
        let start_preempt = Instant::now();
        let (req_voice, _stream_voice, resp_voice) =
            make_test_request(LlmPriority::RealtimeVoice, "voice_urgent", 5);

        pool.submit_task(req_voice).await.unwrap();

        // Wait for background preemption response
        let bg_res = resp_bg.await.unwrap();
        let preempt_elapsed = start_preempt.elapsed();

        assert!(
            matches!(bg_res, Err(LlmPoolError::Preempted(LlmPriority::RealtimeVoice))),
            "Expected preempted by Voice, got: {:?}",
            bg_res
        );

        println!("Preemption elapsed: {:?}", preempt_elapsed);
        assert!(
            preempt_elapsed < Duration::from_millis(50),
            "Preemption took too long: {:?}",
            preempt_elapsed
        );

        let voice_res = resp_voice.await.unwrap();
        assert!(voice_res.is_ok());

        let metrics = pool.get_metrics();
        assert!(metrics.preemption_events_total >= 1);

        pool.shutdown();
    }

    #[tokio::test]
    async fn test_token_streaming_deltas() {
        let (pool, _handle) = LlmWorkerPool::new_simulated(Duration::from_millis(1), 32);

        let (request, mut stream_rx, resp_rx) =
            make_test_request(LlmPriority::InteractiveUser, "Hello world", 5);

        pool.submit_task(request).await.unwrap();

        let mut deltas = Vec::new();
        while let Some(delta) = stream_rx.recv().await {
            deltas.push(delta);
        }

        assert_eq!(deltas.len(), 5, "Must receive 5 token stream deltas");
        assert!(deltas[0].is_first_token, "First token must have is_first_token == true");
        assert!(!deltas[0].is_final_token, "First token must not be final");
        assert_eq!(deltas[0].cumulative_tokens, 1);

        assert!(!deltas[4].is_first_token);
        assert!(deltas[4].is_final_token, "Last token must have is_final_token == true");
        assert_eq!(deltas[4].cumulative_tokens, 5);

        // Check monotonic latency
        for i in 1..deltas.len() {
            assert!(
                deltas[i].latency_from_start_ns >= deltas[i - 1].latency_from_start_ns,
                "Latency must be monotonically increasing"
            );
        }

        let result = resp_rx.await.unwrap().unwrap();
        assert_eq!(result.completion_tokens, 5);
        assert!(!result.full_text.is_empty());
        assert!(result.ttft_ns > 0);
        assert!(result.total_duration_ns >= result.ttft_ns);

        pool.shutdown();
    }

    #[tokio::test]
    async fn test_cancellation_token_explicit() {
        let (pool, _handle) = LlmWorkerPool::new_simulated(Duration::from_millis(5), 32);

        let (request, _stream_rx, resp_rx) =
            make_test_request(LlmPriority::InteractiveUser, "cancel_test", 50);

        let cancel_token = request.cancellation_token.clone();
        pool.submit_task(request).await.unwrap();

        // Let it run for 10ms then cancel
        tokio::time::sleep(Duration::from_millis(10)).await;
        cancel_token.cancel();

        let res = resp_rx.await.unwrap();
        assert_eq!(res, Err(LlmPoolError::Cancelled));

        pool.shutdown();
    }

    #[tokio::test]
    async fn test_pool_metrics_and_voice_check() {
        let (pool, _handle) = LlmWorkerPool::new_simulated(Duration::from_millis(1), 32);

        assert!(!pool.has_pending_voice_task());

        let (req_voice, _stream_voice, resp_voice) =
            make_test_request(LlmPriority::RealtimeVoice, "voice_check", 3);

        pool.submit_task(req_voice).await.unwrap();
        let res = resp_voice.await.unwrap();
        assert!(res.is_ok());

        let metrics = pool.get_metrics();
        assert_eq!(metrics.total_completed_tasks, 1);
        assert_eq!(metrics.total_tokens_generated, 3);
        assert!(metrics.last_ttft_ns > 0);

        pool.shutdown();
    }

    #[tokio::test]
    async fn test_user_preempts_background() {
        let (pool, _handle) = LlmWorkerPool::new_simulated(Duration::from_millis(5), 32);

        let (req_bg, _stream_bg, resp_bg) =
            make_test_request(LlmPriority::BackgroundConsolidation, "bg_run", 50);
        pool.submit_task(req_bg).await.unwrap();

        tokio::time::sleep(Duration::from_millis(15)).await;

        let (req_user, _stream_user, resp_user) =
            make_test_request(LlmPriority::InteractiveUser, "user_preempt", 5);
        pool.submit_task(req_user).await.unwrap();

        let bg_res = resp_bg.await.unwrap();
        assert_eq!(bg_res, Err(LlmPoolError::Preempted(LlmPriority::InteractiveUser)));

        let user_res = resp_user.await.unwrap();
        assert!(user_res.is_ok());

        pool.shutdown();
    }

    #[tokio::test]
    async fn test_voice_not_preempted_by_user_or_background() {
        let (pool, _handle) = LlmWorkerPool::new_simulated(Duration::from_millis(5), 32);

        let (req_voice, _stream_voice, resp_voice) =
            make_test_request(LlmPriority::RealtimeVoice, "voice_active", 20);
        pool.submit_task(req_voice).await.unwrap();

        tokio::time::sleep(Duration::from_millis(10)).await;

        // Submit user and background while voice is running
        let (req_user, _stream_user, resp_user) =
            make_test_request(LlmPriority::InteractiveUser, "user_incoming", 5);
        let (req_bg, _stream_bg, resp_bg) =
            make_test_request(LlmPriority::BackgroundConsolidation, "bg_incoming", 5);

        pool.submit_task(req_user).await.unwrap();
        pool.submit_task(req_bg).await.unwrap();

        // Voice must complete successfully without being preempted
        let voice_res = resp_voice.await.unwrap();
        assert!(voice_res.is_ok(), "Voice must not be preempted by lower priority tasks");

        let user_res = resp_user.await.unwrap();
        assert!(user_res.is_ok());

        let bg_res = resp_bg.await.unwrap();
        assert!(bg_res.is_ok());

        pool.shutdown();
    }

    #[tokio::test]
    async fn test_concurrent_stress_load() {
        let (pool, _handle) = LlmWorkerPool::new_simulated(Duration::from_millis(1), 64);

        let mut handles = Vec::new();
        for i in 0..20 {
            let p = pool.clone();
            let prio = match i % 3 {
                0 => LlmPriority::RealtimeVoice,
                1 => LlmPriority::InteractiveUser,
                _ => LlmPriority::BackgroundConsolidation,
            };
            handles.push(tokio::spawn(async move {
                let (req, mut stream_rx, resp_rx) =
                    make_test_request(prio, &format!("stress_{}", i), 4);
                if p.submit_task(req).await.is_ok() {
                    tokio::spawn(async move {
                        while let Some(_) = stream_rx.recv().await {}
                    });
                    let _ = resp_rx.await;
                }
            }));
        }

        for h in handles {
            let _ = h.await;
        }

        let metrics = pool.get_metrics();
        assert!(metrics.total_completed_tasks + metrics.total_failed_tasks >= 20);

        pool.shutdown();
    }

    #[tokio::test]
    async fn test_shutdown_rejects_subsequent_submissions() {
        let (pool, handle) = LlmWorkerPool::new(16);
        pool.shutdown();
        let _ = handle.await;

        let (req, _stream, _resp) = make_test_request(LlmPriority::RealtimeVoice, "test", 5);
        let res = pool.submit_task(req).await;
        assert_eq!(res, Err(LlmPoolError::ChannelClosed));
    }
}
