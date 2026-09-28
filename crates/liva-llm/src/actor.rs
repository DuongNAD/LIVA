use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, info, warn};

/// 3-level Priority queue enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Low = 0,
    Normal = 1,
    High = 2,
}

/// Commands that the LLM actor can process
#[derive(Debug)]
pub enum LlmCommand {
    /// Command to generate text using the LLM model with optional streaming channel
    GenerateText {
        prompt: String,
        priority: Priority,
        token_tx: Option<mpsc::Sender<String>>,
        responder: oneshot::Sender<anyhow::Result<String>>,
        cancel_token: Option<Arc<AtomicBool>>,
    },
    /// Command to shut down the actor gracefully
    Shutdown,
}

impl LlmCommand {
    pub fn priority(&self) -> Priority {
        match self {
            Self::GenerateText { priority, .. } => *priority,
            Self::Shutdown => Priority::High,
        }
    }
}

/// Internal queue holding commands by priority
struct PriorityQueue {
    high: VecDeque<LlmCommand>,
    normal: VecDeque<LlmCommand>,
    low: VecDeque<LlmCommand>,
}

impl PriorityQueue {
    fn new() -> Self {
        Self {
            high: VecDeque::new(),
            normal: VecDeque::new(),
            low: VecDeque::new(),
        }
    }

    fn push(&mut self, cmd: LlmCommand) {
        match cmd.priority() {
            Priority::High => self.high.push_back(cmd),
            Priority::Normal => self.normal.push_back(cmd),
            Priority::Low => self.low.push_back(cmd),
        }
    }

    fn pop(&mut self) -> Option<LlmCommand> {
        if let Some(cmd) = self.high.pop_front() {
            return Some(cmd);
        }
        if let Some(cmd) = self.normal.pop_front() {
            return Some(cmd);
        }
        if let Some(cmd) = self.low.pop_front() {
            return Some(cmd);
        }
        None
    }

    #[allow(dead_code)]
    fn is_empty(&self) -> bool {
        self.high.is_empty() && self.normal.is_empty() && self.low.is_empty()
    }
}

/// Legacy non-streaming backend closure type for LLM inference
pub type LlmBackendFn = Box<dyn FnMut(&str) -> anyhow::Result<String> + Send>;

/// Real-time streaming backend closure type for LLM inference
pub type LlmStreamingBackendFn =
    Box<dyn FnMut(&str, Option<mpsc::Sender<String>>) -> anyhow::Result<String> + Send>;

/// The Actor that processes LLM requests sequentially
pub struct LlmActor {
    receiver: mpsc::Receiver<LlmCommand>,
    queue: PriorityQueue,
    backend: Option<Arc<std::sync::Mutex<LlmStreamingBackendFn>>>,
}

impl LlmActor {
    pub fn new(receiver: mpsc::Receiver<LlmCommand>) -> Self {
        Self {
            receiver,
            queue: PriorityQueue::new(),
            backend: None,
        }
    }

    /// Attaches a custom non-streaming inference backend to the LLM actor (adapts 1-arg closure)
    pub fn with_backend(mut self, mut backend: LlmBackendFn) -> Self {
        self.backend = Some(Arc::new(std::sync::Mutex::new(Box::new(
            move |prompt: &str, _token_tx: Option<mpsc::Sender<String>>| backend(prompt),
        ))));
        self
    }

    /// Attaches a streaming-capable inference backend to the LLM actor
    pub fn with_streaming_backend(mut self, backend: LlmStreamingBackendFn) -> Self {
        self.backend = Some(Arc::new(std::sync::Mutex::new(backend)));
        self
    }

    /// The run loop for the LLM actor. Should be spawned on a dedicated worker thread
    /// or tokio task to prevent UI head-of-line blocking.
    pub async fn run(mut self) {
        info!("LlmActor started.");
        loop {
            // Drain all pending messages from channel into priority queue before popping next command.
            // This guarantees high-priority preemption even if lower priority messages arrived first.
            while let Ok(cmd) = self.receiver.try_recv() {
                self.queue.push(cmd);
            }

            if let Some(cmd) = self.queue.pop() {
                if !self.handle_command(cmd).await {
                    info!("LlmActor shutting down.");
                    break;
                }
            } else {
                // If the queue is empty, await the next command
                match self.receiver.recv().await {
                    Some(cmd) => {
                        self.queue.push(cmd);
                        // Eagerly drain any further messages that arrived concurrently
                        while let Ok(cmd) = self.receiver.try_recv() {
                            self.queue.push(cmd);
                        }
                    }
                    None => {
                        info!("LlmActor command channel closed, shutting down.");
                        break;
                    }
                }
            }
        }
    }

    async fn handle_command(&mut self, cmd: LlmCommand) -> bool {
        match cmd {
            LlmCommand::GenerateText {
                prompt,
                priority: current_priority,
                token_tx,
                responder,
                cancel_token,
            } => {
                debug!("LlmActor processing GenerateText command via spawn_blocking.");
                let backend_opt = self.backend.clone();
                let cancel = cancel_token.unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
                let cancel_for_worker = cancel.clone();

                let mut worker_task = tokio::task::spawn_blocking(move || {
                    if let Some(backend_arc) = backend_opt {
                        let mut backend = match backend_arc.lock() {
                            Ok(guard) => guard,
                            Err(poisoned) => {
                                warn!(
                                    "LlmActor backend mutex was poisoned; recovering inner guard."
                                );
                                poisoned.into_inner()
                            }
                        };
                        backend(&prompt, token_tx)
                    } else {
                        if let Some(ref tx) = token_tx {
                            let pieces = ["Simulated ", "response ", "to: ", &prompt];
                            for piece in pieces {
                                if cancel_for_worker.load(Ordering::Relaxed) {
                                    return Err(anyhow::anyhow!(
                                        "Operation cancelled by preemption"
                                    ));
                                }
                                let _ = tx.try_send(piece.to_string());
                            }
                        }
                        if cancel_for_worker.load(Ordering::Relaxed) {
                            return Err(anyhow::anyhow!("Operation cancelled by preemption"));
                        }
                        Ok(format!("Simulated response to: {}", prompt))
                    }
                });

                // While worker_task is running, monitor self.receiver for higher priority commands or shutdown
                let final_result = loop {
                    tokio::select! {
                        join_res = &mut worker_task => {
                            break match join_res {
                                Ok(inner) => inner,
                                Err(join_err) => Err(anyhow::anyhow!(
                                    "LLM blocking inference task failed: {join_err}"
                                )),
                            };
                        }
                        Some(new_cmd) = self.receiver.recv() => {
                            let new_priority = new_cmd.priority();
                            let is_shutdown = matches!(new_cmd, LlmCommand::Shutdown);
                            self.queue.push(new_cmd);

                            if is_shutdown || new_priority > current_priority {
                                info!(
                                    "LlmActor: Preempting active {:?} generation with {:?} command",
                                    current_priority, new_priority
                                );
                                cancel.store(true, Ordering::SeqCst);
                            }
                        }
                    }
                };

                let _ = responder.send(final_result);
                true
            }
            LlmCommand::Shutdown => {
                debug!("LlmActor processing Shutdown command.");
                false
            }
        }
    }
}

/// A handle to interact with the LLM Actor
#[derive(Clone)]
pub struct LlmActorHandle {
    sender: mpsc::Sender<LlmCommand>,
}

impl LlmActorHandle {
    pub fn new(sender: mpsc::Sender<LlmCommand>) -> Self {
        Self { sender }
    }

    /// Sends a prompt to be processed by the LLM actor (non-streaming delegate).
    pub async fn generate_text(
        &self,
        prompt: impl Into<String>,
        priority: Priority,
    ) -> anyhow::Result<String> {
        self.generate_text_stream(prompt, priority, None).await
    }

    /// Sends a prompt to be processed by the LLM actor with optional incremental token streaming.
    pub async fn generate_text_stream(
        &self,
        prompt: impl Into<String>,
        priority: Priority,
        token_tx: Option<mpsc::Sender<String>>,
    ) -> anyhow::Result<String> {
        self.generate_text_stream_cancellable(prompt, priority, token_tx, None)
            .await
    }

    /// Sends a prompt to be processed by the LLM actor with optional token streaming and cancellation token.
    pub async fn generate_text_stream_cancellable(
        &self,
        prompt: impl Into<String>,
        priority: Priority,
        token_tx: Option<mpsc::Sender<String>>,
        cancel_token: Option<Arc<AtomicBool>>,
    ) -> anyhow::Result<String> {
        let (tx, rx) = oneshot::channel();
        let cmd = LlmCommand::GenerateText {
            prompt: prompt.into(),
            priority,
            token_tx,
            responder: tx,
            cancel_token,
        };

        self.sender
            .send(cmd)
            .await
            .map_err(|_| anyhow::anyhow!("Failed to send GenerateText command to LlmActor"))?;

        rx.await
            .map_err(|_| anyhow::anyhow!("Failed to receive response from LlmActor"))?
    }

    /// Requests the actor to gracefully shut down.
    pub async fn shutdown(&self) -> anyhow::Result<()> {
        let cmd = LlmCommand::Shutdown;
        self.sender
            .send(cmd)
            .await
            .map_err(|_| anyhow::anyhow!("Failed to send Shutdown command to LlmActor"))
    }
}
