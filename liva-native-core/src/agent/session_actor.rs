//! Session Actor & Queue Modes Implementation (Features F07 & F08).
//!
//! Provides:
//! - Dedicated per-session actor (`SessionActor`) with bounded `mpsc::Receiver` mailbox (capacity 64).
//! - Strict Zero Open Network Ports guarantee (pure in-process IPC and Windows NamedPipe; binds NO TCP/UDP sockets).
//! - 4 queue execution modes: `Steer`, `Followup`, `Collect`, and `Interrupt`.
//! - Session-isolated cooperative cancellation via `Arc<AtomicBool>`.
//! - State checkpointing and memory eviction on idle timeout.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, mpsc, oneshot};
use tracing::{debug, error, info, warn};

pub use liva_core_types::{
    CommandPrincipal, SessionError, SessionQueueMode, SessionTurnOutput, SessionTurnRequest,
};

/// Type alias for backward compatibility and prompt dispatch consistency.
pub type QueueMode = SessionQueueMode;
/// Type alias for SessionMessage.
pub type SessionEnvelope = SessionMessage;

use crate::agent::memory::SqliteCheckpointer;
use crate::agent::state::AgentState;

pub const DEFAULT_MAILBOX_CAPACITY: usize = 64;
pub const MAX_FOLLOWUP_QUEUE_DEPTH: usize = 16;
pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(300); // 5 minutes
pub const DEFAULT_COLLECT_DEBOUNCE: Duration = Duration::from_millis(350);

/// Messages handled by the SessionActor mailbox.
pub enum SessionMessage {
    /// Execute a turn with queue mode semantics.
    ExecuteTurn {
        request: SessionTurnRequest,
        stream_tx: Option<mpsc::Sender<String>>,
        responder: oneshot::Sender<Result<SessionTurnOutput, SessionError>>,
    },
    /// Direct steering injection for the active turn.
    Steer {
        instruction: String,
        responder: oneshot::Sender<Result<(), SessionError>>,
    },
    /// Preemptive interruption signal.
    Interrupt {
        reason: String,
        responder: oneshot::Sender<Result<(), SessionError>>,
    },
    /// State inspection snapshot.
    GetSnapshot {
        responder: oneshot::Sender<SessionSnapshot>,
    },
    /// Graceful shutdown of actor.
    Shutdown { responder: oneshot::Sender<()> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub session_id: String,
    pub status: String,
    pub queued_followups: usize,
    pub message_count: usize,
    pub is_idle: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    Idle,
    Active,
    Closed,
}

pub struct QueuedTurn {
    pub request: SessionTurnRequest,
    pub stream_tx: Option<mpsc::Sender<String>>,
    pub responder: oneshot::Sender<Result<SessionTurnOutput, SessionError>>,
}

pub struct ActiveTurn {
    pub turn_id: String,
    pub cancel_token: Arc<AtomicBool>,
    pub steering_tx: mpsc::Sender<String>,
    pub start_time: Instant,
}

struct CollectBuffer {
    requests: Vec<SessionTurnRequest>,
    responders: Vec<oneshot::Sender<Result<SessionTurnOutput, SessionError>>>,
    stream_tx: Option<mpsc::Sender<String>>,
    deadline: Instant,
}

/// The Actor executing conversational turns sequentially for a single session.
pub struct SessionActor {
    pub session_id: String,
    receiver: mpsc::Receiver<SessionMessage>,
    status: SessionStatus,
    state: AgentState,
    followup_queue: VecDeque<QueuedTurn>,
    active_turn: Option<ActiveTurn>,
    collect_buffer: Option<CollectBuffer>,
    checkpointer: Option<Arc<SqliteCheckpointer>>,
    idle_timeout: Duration,
    collect_debounce: Duration,
    last_activity: Instant,
    turn_completion_tx: mpsc::Sender<TurnCompletionEvent>,
    turn_completion_rx: mpsc::Receiver<TurnCompletionEvent>,
}

struct TurnCompletionEvent {
    turn_id: String,
    _output: Result<SessionTurnOutput, SessionError>,
    new_state: Option<AgentState>,
}

impl SessionActor {
    pub fn new(
        session_id: String,
        receiver: mpsc::Receiver<SessionMessage>,
        initial_state: Option<AgentState>,
        checkpointer: Option<Arc<SqliteCheckpointer>>,
    ) -> Self {
        let (turn_completion_tx, turn_completion_rx) = mpsc::channel(8);
        Self {
            session_id,
            receiver,
            status: SessionStatus::Idle,
            state: initial_state.unwrap_or_default(),
            followup_queue: VecDeque::new(),
            active_turn: None,
            collect_buffer: None,
            checkpointer,
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
            collect_debounce: DEFAULT_COLLECT_DEBOUNCE,
            last_activity: Instant::now(),
            turn_completion_tx,
            turn_completion_rx,
        }
    }

    pub fn with_idle_timeout(mut self, timeout: Duration) -> Self {
        self.idle_timeout = timeout;
        self
    }

    pub fn with_collect_debounce(mut self, debounce: Duration) -> Self {
        self.collect_debounce = debounce;
        self
    }

    /// Primary event loop running in a dedicated Tokio task.
    pub async fn run(mut self) {
        info!("[SessionActor:{}] Started actor loop", self.session_id);

        loop {
            // Determine debounce sleep duration if collect buffer is active
            let debounce_sleep = if let Some(ref buf) = self.collect_buffer {
                let now = Instant::now();
                if buf.deadline > now {
                    buf.deadline - now
                } else {
                    Duration::from_millis(0)
                }
            } else {
                Duration::from_secs(86400) // indefinite sleep
            };

            let idle_sleep = self.idle_timeout;

            tokio::select! {
                // Branch 1: Incoming message from mailbox
                msg = self.receiver.recv() => {
                    match msg {
                        Some(message) => {
                            self.last_activity = Instant::now();
                            if !self.handle_message(message).await {
                                break;
                            }
                        }
                        None => {
                            info!("[SessionActor:{}] Mailbox sender dropped, closing actor", self.session_id);
                            break;
                        }
                    }
                }

                // Branch 2: Completed background turn execution
                Some(completion) = self.turn_completion_rx.recv() => {
                    self.last_activity = Instant::now();
                    self.handle_turn_completion(completion).await;
                }

                // Branch 3: Collect debounce timer expired
                _ = tokio::time::sleep(debounce_sleep), if self.collect_buffer.is_some() => {
                    self.flush_collect_buffer().await;
                }

                // Branch 4: Idle timeout reached while in Idle status
                _ = tokio::time::sleep(idle_sleep), if self.status == SessionStatus::Idle && self.followup_queue.is_empty() && self.collect_buffer.is_none() => {
                    if self.last_activity.elapsed() >= self.idle_timeout {
                        info!("[SessionActor:{}] Idle timeout reached ({:?}), checkpointing and exiting task", self.session_id, self.idle_timeout);
                        self.checkpoint_state().await;
                        break;
                    }
                }
            }
        }

        // Teardown: flush state and abort pending queues
        self.status = SessionStatus::Closed;
        self.drain_and_reject_queues(SessionError::MailboxClosed);
        info!(
            "[SessionActor:{}] Actor task terminated cleanly",
            self.session_id
        );
    }

    async fn handle_message(&mut self, message: SessionMessage) -> bool {
        match message {
            SessionMessage::ExecuteTurn {
                request,
                stream_tx,
                responder,
            } => {
                match request.queue_mode {
                    SessionQueueMode::Followup => {
                        self.handle_followup_mode(request, stream_tx, responder)
                            .await;
                    }
                    SessionQueueMode::Interrupt => {
                        self.handle_interrupt_mode(request, stream_tx, responder)
                            .await;
                    }
                    SessionQueueMode::Steer => {
                        self.handle_steer_mode(request, stream_tx, responder).await;
                    }
                    SessionQueueMode::Collect => {
                        self.handle_collect_mode(request, stream_tx, responder)
                            .await;
                    }
                }
                true
            }
            SessionMessage::Steer {
                instruction,
                responder,
            } => {
                if let Some(ref active) = self.active_turn {
                    let _ = active.steering_tx.try_send(instruction);
                    let _ = responder.send(Ok(()));
                } else {
                    let _ = responder.send(Err(SessionError::StateError(
                        "No active turn to steer".into(),
                    )));
                }
                true
            }
            SessionMessage::Interrupt { reason, responder } => {
                self.trigger_interrupt(&reason);
                let _ = responder.send(Ok(()));
                true
            }
            SessionMessage::GetSnapshot { responder } => {
                let snap = SessionSnapshot {
                    session_id: self.session_id.clone(),
                    status: format!("{:?}", self.status),
                    queued_followups: self.followup_queue.len(),
                    message_count: self.state.messages.len(),
                    is_idle: self.status == SessionStatus::Idle,
                };
                let _ = responder.send(snap);
                true
            }
            SessionMessage::Shutdown { responder } => {
                info!(
                    "[SessionActor:{}] Graceful shutdown requested",
                    self.session_id
                );
                self.checkpoint_state().await;
                let _ = responder.send(());
                false // Break loop
            }
        }
    }

    async fn handle_followup_mode(
        &mut self,
        request: SessionTurnRequest,
        stream_tx: Option<mpsc::Sender<String>>,
        responder: oneshot::Sender<Result<SessionTurnOutput, SessionError>>,
    ) {
        if self.status == SessionStatus::Idle {
            self.start_turn(request, stream_tx, responder).await;
        } else {
            if self.followup_queue.len() >= MAX_FOLLOWUP_QUEUE_DEPTH {
                warn!(
                    "[SessionActor:{}] Followup queue full ({})",
                    self.session_id, MAX_FOLLOWUP_QUEUE_DEPTH
                );
                let _ = responder.send(Err(SessionError::QueueFull));
                return;
            }
            self.followup_queue.push_back(QueuedTurn {
                request,
                stream_tx,
                responder,
            });
        }
    }

    async fn handle_interrupt_mode(
        &mut self,
        request: SessionTurnRequest,
        stream_tx: Option<mpsc::Sender<String>>,
        responder: oneshot::Sender<Result<SessionTurnOutput, SessionError>>,
    ) {
        if self.status == SessionStatus::Active {
            self.trigger_interrupt("Preempted by Interrupt queue mode");
            // Clear pending followups since conversation context has been preempted
            self.drain_and_reject_queues(SessionError::Interrupted);
            // Push interrupt request to front of queue to run next
            self.followup_queue.push_front(QueuedTurn {
                request,
                stream_tx,
                responder,
            });
        } else {
            self.start_turn(request, stream_tx, responder).await;
        }
    }

    async fn handle_steer_mode(
        &mut self,
        request: SessionTurnRequest,
        stream_tx: Option<mpsc::Sender<String>>,
        responder: oneshot::Sender<Result<SessionTurnOutput, SessionError>>,
    ) {
        if let Some(ref active) = self.active_turn {
            // Forward steering instruction to active turn
            let _ = active.steering_tx.try_send(request.input_text.clone());
            let _ = responder.send(Ok(SessionTurnOutput {
                turn_id: request.turn_id,
                response_text: "Steer instruction accepted".into(),
                is_interrupted: false,
                is_steered: true,
                execution_duration_ms: 0,
            }));
        } else {
            // Degrade to regular turn if actor is idle
            self.start_turn(request, stream_tx, responder).await;
        }
    }

    async fn handle_collect_mode(
        &mut self,
        request: SessionTurnRequest,
        stream_tx: Option<mpsc::Sender<String>>,
        responder: oneshot::Sender<Result<SessionTurnOutput, SessionError>>,
    ) {
        if self.status == SessionStatus::Active {
            // Coalesce with existing pending Collect turn if at tail of followup queue
            if let Some(last) = self.followup_queue.back_mut() {
                if last.request.queue_mode == SessionQueueMode::Collect {
                    last.request.input_text.push('\n');
                    last.request.input_text.push_str(&request.input_text);
                    let _ = responder.send(Ok(SessionTurnOutput {
                        turn_id: request.turn_id,
                        response_text: "Merged with pending queued turn".into(),
                        is_interrupted: false,
                        is_steered: false,
                        execution_duration_ms: 0,
                    }));
                    return;
                }
            }
            if self.followup_queue.len() < MAX_FOLLOWUP_QUEUE_DEPTH {
                self.followup_queue.push_back(QueuedTurn {
                    request,
                    stream_tx,
                    responder,
                });
            } else {
                let _ = responder.send(Err(SessionError::QueueFull));
            }
        } else {
            // Idle: Accumulate into collect buffer and arm debounce window
            let deadline = Instant::now() + self.collect_debounce;
            if let Some(ref mut buf) = self.collect_buffer {
                buf.requests.push(request);
                buf.responders.push(responder);
                buf.deadline = deadline;
            } else {
                self.collect_buffer = Some(CollectBuffer {
                    requests: vec![request],
                    responders: vec![responder],
                    stream_tx,
                    deadline,
                });
            }
        }
    }

    async fn flush_collect_buffer(&mut self) {
        if let Some(buf) = self.collect_buffer.take() {
            if buf.requests.is_empty() {
                return;
            }
            let first_req = buf.requests[0].clone();
            let mut combined_text = String::new();
            for (idx, r) in buf.requests.iter().enumerate() {
                if idx > 0 {
                    combined_text.push('\n');
                }
                combined_text.push_str(&r.input_text);
            }

            let consolidated_req = SessionTurnRequest {
                session_id: first_req.session_id,
                turn_id: first_req.turn_id,
                input_text: combined_text,
                queue_mode: SessionQueueMode::Collect,
                principal: first_req.principal,
                metadata: first_req.metadata,
                session_mode: first_req.session_mode,
                turn_principal: first_req.turn_principal,
            };

            // Multiplex output to all waiting responders
            let (turn_tx, turn_rx): (
                oneshot::Sender<Result<SessionTurnOutput, SessionError>>,
                oneshot::Receiver<Result<SessionTurnOutput, SessionError>>,
            ) = oneshot::channel();
            tokio::spawn(async move {
                if let Ok(res) = turn_rx.await {
                    for resp in buf.responders {
                        let _ = resp.send(res.clone());
                    }
                }
            });

            self.start_turn(consolidated_req, buf.stream_tx, turn_tx)
                .await;
        }
    }

    fn trigger_interrupt(&mut self, reason: &str) {
        if let Some(ref active) = self.active_turn {
            info!(
                "[SessionActor:{}] Interrupting active turn {} (reason: {})",
                self.session_id, active.turn_id, reason
            );
            active.cancel_token.store(true, Ordering::SeqCst);
        }
    }

    fn drain_and_reject_queues(&mut self, err: SessionError) {
        while let Some(queued) = self.followup_queue.pop_front() {
            let _ = queued.responder.send(Err(err.clone()));
        }
        if let Some(buf) = self.collect_buffer.take() {
            for resp in buf.responders {
                let _ = resp.send(Err(err.clone()));
            }
        }
    }

    async fn start_turn(
        &mut self,
        request: SessionTurnRequest,
        stream_tx: Option<mpsc::Sender<String>>,
        responder: oneshot::Sender<Result<SessionTurnOutput, SessionError>>,
    ) {
        self.status = SessionStatus::Active;
        let cancel_token = Arc::new(AtomicBool::new(false));
        let (steering_tx, steering_rx) = mpsc::channel(8);

        self.active_turn = Some(ActiveTurn {
            turn_id: request.turn_id.clone(),
            cancel_token: Arc::clone(&cancel_token),
            steering_tx,
            start_time: Instant::now(),
        });

        let completion_tx = self.turn_completion_tx.clone();
        let turn_id = request.turn_id.clone();
        let mut current_state = self.state.clone();

        // Spawn isolated turn execution task
        tokio::spawn(async move {
            let start = Instant::now();
            let exec_result = Self::execute_turn_logic(
                request,
                stream_tx,
                cancel_token,
                steering_rx,
                &mut current_state,
            )
            .await;

            let duration_ms = start.elapsed().as_millis() as u64;
            let (turn_output, new_state) = match exec_result {
                Ok((text, was_interrupted, was_steered)) => {
                    let out = Ok(SessionTurnOutput {
                        turn_id: turn_id.clone(),
                        response_text: text,
                        is_interrupted: was_interrupted,
                        is_steered: was_steered,
                        execution_duration_ms: duration_ms,
                    });
                    (out, Some(current_state))
                }
                Err(err) => (Err(err), None),
            };

            let _ = responder.send(turn_output.clone());
            let _ = completion_tx
                .send(TurnCompletionEvent {
                    turn_id,
                    _output: turn_output,
                    new_state,
                })
                .await;
        });
    }

    async fn execute_turn_logic(
        request: SessionTurnRequest,
        stream_tx: Option<mpsc::Sender<String>>,
        cancel_token: Arc<AtomicBool>,
        mut steering_rx: mpsc::Receiver<String>,
        state: &mut AgentState,
    ) -> Result<(String, bool, bool), SessionError> {
        // Append user utterance to state
        state.messages.push(serde_json::json!({
            "role": "user",
            "content": request.input_text,
        }));
        state.context.insert(
            "session_mode".into(),
            serde_json::to_value(request.session_mode).unwrap_or_default(),
        );
        state.context.insert(
            "turn_principal".into(),
            serde_json::to_value(&request.turn_principal).unwrap_or_default(),
        );
        state.trim_history();

        // Check cancellation before generation starts
        if cancel_token.load(Ordering::Relaxed) {
            return Ok(("Turn interrupted before execution".into(), true, false));
        }

        // Check for early steering instructions
        let mut steered = false;
        if let Ok(steer) = steering_rx.try_recv() {
            state
                .context
                .insert("steering".into(), serde_json::Value::String(steer));
            steered = true;
        }

        let mut accumulated_output = String::new();
        // Generate simulated response chunks with cooperative cancellation checks
        let simulated_chunks = vec!["Xin ", "chào, ", "tôi ", "là ", "LIVA."];

        for chunk in simulated_chunks {
            if cancel_token.load(Ordering::Relaxed) {
                return Ok((accumulated_output, true, steered));
            }
            // Check steering mid-stream
            if let Ok(steer) = steering_rx.try_recv() {
                debug!("Received mid-turn steer: {steer}");
                steered = true;
            }

            accumulated_output.push_str(chunk);
            if let Some(ref tx) = stream_tx {
                let _ = tx.send(chunk.to_string()).await;
            }
            tokio::time::sleep(Duration::from_millis(15)).await;
        }

        state.messages.push(serde_json::json!({
            "role": "assistant",
            "content": accumulated_output,
        }));
        state.trim_history();

        Ok((accumulated_output, false, steered))
    }

    async fn handle_turn_completion(&mut self, completion: TurnCompletionEvent) {
        if let Some(ref active) = self.active_turn {
            if active.turn_id == completion.turn_id {
                self.active_turn = None;
            }
        }

        if let Some(new_state) = completion.new_state {
            self.state = new_state;
            self.checkpoint_state().await;
        }

        // Process next turn in followup queue
        if let Some(next_turn) = self.followup_queue.pop_front() {
            self.start_turn(next_turn.request, next_turn.stream_tx, next_turn.responder)
                .await;
        } else {
            self.status = SessionStatus::Idle;
        }
    }

    async fn checkpoint_state(&self) {
        if let Some(ref cp) = self.checkpointer {
            if let Err(e) = cp.save_checkpoint(&self.session_id, &self.state).await {
                error!(
                    "[SessionActor:{}] Failed to persist checkpoint: {}",
                    self.session_id, e
                );
            } else {
                debug!(
                    "[SessionActor:{}] Checkpoint persisted successfully",
                    self.session_id
                );
            }
        }
    }
}

/// Handle cloneable by callers to dispatch commands to the actor.
#[derive(Clone)]
pub struct SessionActorHandle {
    pub session_id: String,
    sender: mpsc::Sender<SessionMessage>,
}

impl SessionActorHandle {
    pub fn new(session_id: String, sender: mpsc::Sender<SessionMessage>) -> Self {
        Self { session_id, sender }
    }

    pub fn is_closed(&self) -> bool {
        self.sender.is_closed()
    }

    pub async fn execute_turn(
        &self,
        request: SessionTurnRequest,
        stream_tx: Option<mpsc::Sender<String>>,
    ) -> Result<SessionTurnOutput, SessionError> {
        let (responder, rx) = oneshot::channel();
        self.sender
            .send(SessionMessage::ExecuteTurn {
                request,
                stream_tx,
                responder,
            })
            .await
            .map_err(|_| SessionError::MailboxClosed)?;
        rx.await.map_err(|_| SessionError::MailboxClosed)?
    }

    pub async fn interrupt(&self, reason: String) -> Result<(), SessionError> {
        let (responder, rx) = oneshot::channel();
        self.sender
            .send(SessionMessage::Interrupt { reason, responder })
            .await
            .map_err(|_| SessionError::MailboxClosed)?;
        rx.await.map_err(|_| SessionError::MailboxClosed)?
    }

    pub async fn steer(&self, instruction: String) -> Result<(), SessionError> {
        let (responder, rx) = oneshot::channel();
        self.sender
            .send(SessionMessage::Steer {
                instruction,
                responder,
            })
            .await
            .map_err(|_| SessionError::MailboxClosed)?;
        rx.await.map_err(|_| SessionError::MailboxClosed)?
    }

    pub async fn get_snapshot(&self) -> Result<SessionSnapshot, SessionError> {
        let (responder, rx) = oneshot::channel();
        self.sender
            .send(SessionMessage::GetSnapshot { responder })
            .await
            .map_err(|_| SessionError::MailboxClosed)?;
        rx.await.map_err(|_| SessionError::MailboxClosed)
    }

    pub async fn shutdown(&self) -> Result<(), SessionError> {
        let (responder, rx) = oneshot::channel();
        self.sender
            .send(SessionMessage::Shutdown { responder })
            .await
            .map_err(|_| SessionError::MailboxClosed)?;
        rx.await.map_err(|_| SessionError::MailboxClosed)
    }
}

/// Thread-safe registry and supervisor of all active session actors.
#[derive(Clone)]
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, SessionActorHandle>>>,
    checkpointer: Option<Arc<SqliteCheckpointer>>,
    mailbox_capacity: usize,
    idle_timeout: Duration,
}

impl SessionManager {
    pub fn new(checkpointer: Option<Arc<SqliteCheckpointer>>) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            checkpointer,
            mailbox_capacity: DEFAULT_MAILBOX_CAPACITY,
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
        }
    }

    pub fn with_idle_timeout(mut self, timeout: Duration) -> Self {
        self.idle_timeout = timeout;
        self
    }

    pub fn with_mailbox_capacity(mut self, capacity: usize) -> Self {
        self.mailbox_capacity = capacity;
        self
    }

    /// Retrieve or spawn a dedicated actor for the requested session ID.
    pub async fn get_or_create(
        &self,
        session_id: &str,
    ) -> Result<SessionActorHandle, SessionError> {
        let mut map = self.sessions.write().await;
        if let Some(handle) = map.get(session_id) {
            if !handle.is_closed() {
                return Ok(handle.clone());
            }
        }

        // Rehydrate initial state from checkpointer if available
        let initial_state = if let Some(ref cp) = self.checkpointer {
            cp.load_checkpoint(session_id).await.unwrap_or(None)
        } else {
            None
        };

        let (tx, rx) = mpsc::channel(self.mailbox_capacity);
        let actor = SessionActor::new(
            session_id.to_string(),
            rx,
            initial_state,
            self.checkpointer.clone(),
        )
        .with_idle_timeout(self.idle_timeout);

        tokio::spawn(actor.run());

        let handle = SessionActorHandle::new(session_id.to_string(), tx);
        map.insert(session_id.to_string(), handle.clone());
        Ok(handle)
    }

    pub async fn active_session_count(&self) -> usize {
        let map = self.sessions.read().await;
        map.len()
    }

    pub async fn prune_closed_sessions(&self) {
        let mut map = self.sessions.write().await;
        map.retain(|_, handle| !handle.is_closed());
    }
}
