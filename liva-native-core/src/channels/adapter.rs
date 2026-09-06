//! Multi-Channel Base Trait & Common Infrastructure (Feature 5)
//!
//! Provides the core `ChannelAdapter` trait, capabilities matrix, lifecycle management,
//! status reporting, error types, backoff strategies, and ingress stream wrapper.

use async_trait::async_trait;
use futures_util::Stream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::sync::mpsc::Receiver;

use crate::messaging::normalized::{
    ChannelId, DeliveryReceipt, IncomingMessage, OutgoingMessage,
};

/// Capabilities matrix defining what features an adapter natively supports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelCapabilities {
    pub streaming_text: bool,
    pub binary_attachments: bool,
    pub voice_notes: bool,
    pub interactive_buttons: bool,
    pub typing_indicator: bool,
    pub thread_replies: bool,
}

impl ChannelCapabilities {
    /// Full capabilities enabled.
    pub fn all() -> Self {
        Self {
            streaming_text: true,
            binary_attachments: true,
            voice_notes: true,
            interactive_buttons: true,
            typing_indicator: true,
            thread_replies: true,
        }
    }

    /// Minimal text-only capabilities.
    pub fn text_only() -> Self {
        Self {
            streaming_text: false,
            binary_attachments: false,
            voice_notes: false,
            interactive_buttons: false,
            typing_indicator: false,
            thread_replies: false,
        }
    }
}

/// Operational connection status of a channel adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ChannelStatus {
    Connected,
    Disconnected,
    Reconnecting {
        attempt: u32,
        next_retry_ms: u64,
    },
    Failed {
        error: String,
    },
}

impl fmt::Display for ChannelStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connected => write!(f, "connected"),
            Self::Disconnected => write!(f, "disconnected"),
            Self::Reconnecting {
                attempt,
                next_retry_ms,
            } => write!(
                f,
                "reconnecting (attempt {}, next in {}ms)",
                attempt, next_retry_ms
            ),
            Self::Failed { error } => write!(f, "failed: {}", error),
        }
    }
}

/// Specific errors encountered during channel adapter lifecycle and transport operations.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Authentication error: {0}")]
    AuthError(String),

    #[error("Message delivery failed: {0}")]
    DeliveryFailed(String),

    #[error("Webhook signature invalid: {0}")]
    InvalidSignature(String),

    #[error("Channel rate limited; retry after {0}s")]
    RateLimited(u64),

    #[error("Unsupported payload type: {0}")]
    UnsupportedPayload(String),

    #[error("Internal channel error: {0}")]
    Internal(String),

    #[error("Adapter is currently disconnected")]
    Disconnected,
}

/// Streaming options for debounced live text streaming.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamOptions {
    pub edit_in_place: bool,
    pub debounce_ms: u64,
}

impl Default for StreamOptions {
    fn default() -> Self {
        Self {
            edit_in_place: true,
            debounce_ms: 1000,
        }
    }
}

/// Exponential backoff calculator with jitter for automatic connection recovery.
#[derive(Debug, Clone)]
pub struct ExponentialBackoff {
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
    pub factor: f64,
    pub jitter: f64,
    current_attempt: Arc<AtomicU32>,
}

impl ExponentialBackoff {
    /// Create a new exponential backoff configuration.
    pub fn new(initial_delay_ms: u64, max_delay_ms: u64) -> Self {
        Self {
            initial_delay_ms,
            max_delay_ms,
            factor: 2.0,
            jitter: 0.1,
            current_attempt: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Configure the multiplication factor (default: 2.0).
    pub fn with_factor(mut self, factor: f64) -> Self {
        self.factor = factor;
        self
    }

    /// Configure the jitter percentage (default: 0.1, i.e., 10%).
    pub fn with_jitter(mut self, jitter: f64) -> Self {
        self.jitter = jitter;
        self
    }

    /// Calculate the delay for a specific attempt number.
    ///
    /// Formula: `min(initial_delay * factor^attempt, max_delay) * (1 + jitter)`
    pub fn calculate_delay(&self, attempt: u32) -> u64 {
        let base = (self.initial_delay_ms as f64) * self.factor.powi(attempt as i32);
        let capped = base.min(self.max_delay_ms as f64);
        let jitter_amount = capped * self.jitter;
        (capped + jitter_amount).round() as u64
    }

    /// Advance attempt count and get the next delay as Duration.
    pub fn next_delay(&self) -> Duration {
        let attempt = self.current_attempt.fetch_add(1, Ordering::SeqCst);
        let delay_ms = self.calculate_delay(attempt);
        Duration::from_millis(delay_ms)
    }

    /// Reset attempt count to 0 upon successful connection.
    pub fn reset(&self) {
        self.current_attempt.store(0, Ordering::SeqCst);
    }

    /// Current attempt count.
    pub fn current_attempt(&self) -> u32 {
        self.current_attempt.load(Ordering::SeqCst)
    }
}

/// BackoffCalculator alias for backwards compatibility and test suites.
pub type BackoffCalculator = ExponentialBackoff;

/// Ingress stream wrapper around a Tokio MPSC Receiver.
pub struct IngressReceiverStream<T> {
    receiver: Receiver<T>,
}

impl<T> IngressReceiverStream<T> {
    /// Wrap a tokio Receiver into a Stream.
    pub fn new(receiver: Receiver<T>) -> Self {
        Self { receiver }
    }
}

impl<T> Stream for IngressReceiverStream<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.receiver.poll_recv(cx)
    }
}

/// Common asynchronous trait implemented by all LIVA channel adapters.
#[async_trait]
pub trait ChannelAdapter: Send + Sync + 'static {
    /// Return the unique identifier for this channel.
    fn channel_id(&self) -> ChannelId;

    /// Return the capabilities supported by this adapter.
    fn capabilities(&self) -> ChannelCapabilities;

    /// Initialize connection to the remote platform / service.
    async fn connect(&mut self) -> Result<(), ChannelError>;

    /// Terminate connection and release active stream resources.
    async fn disconnect(&mut self) -> Result<(), ChannelError>;

    /// Poll or subscribe to an asynchronous stream of normalized incoming messages.
    async fn poll_stream(
        &mut self,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<IncomingMessage, ChannelError>> + Send>>, ChannelError>;

    /// Deliver a normalized outgoing message to the platform.
    async fn send_message(&self, msg: OutgoingMessage) -> Result<DeliveryReceipt, ChannelError>;

    /// Ingest and parse an incoming HTTP webhook request payload.
    async fn handle_webhook(
        &self,
        headers: &HashMap<String, String>,
        body: &[u8],
    ) -> Result<Option<IncomingMessage>, ChannelError>;

    /// Return the current connection status.
    async fn status(&self) -> ChannelStatus;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exponential_backoff_progression() {
        let backoff = ExponentialBackoff::new(100, 5000);

        // Attempt 0: 100 * 2^0 + 10% = 110ms
        assert_eq!(backoff.calculate_delay(0), 110);

        // Attempt 1: 100 * 2^1 + 10% = 220ms
        assert_eq!(backoff.calculate_delay(1), 220);

        // Attempt 2: 100 * 2^2 + 10% = 440ms
        assert_eq!(backoff.calculate_delay(2), 440);

        // Attempt 3: 100 * 2^3 + 10% = 880ms
        assert_eq!(backoff.calculate_delay(3), 880);

        // High attempt capped at 5000 + 10% = 5500ms
        assert_eq!(backoff.calculate_delay(10), 5500);

        // Test stateful next_delay
        let d0 = backoff.next_delay();
        assert_eq!(d0.as_millis(), 110);
        let d1 = backoff.next_delay();
        assert_eq!(d1.as_millis(), 220);
        assert_eq!(backoff.current_attempt(), 2);

        backoff.reset();
        assert_eq!(backoff.current_attempt(), 0);
    }

    #[test]
    fn test_channel_status_display() {
        assert_eq!(ChannelStatus::Connected.to_string(), "connected");
        assert_eq!(ChannelStatus::Disconnected.to_string(), "disconnected");
        assert_eq!(
            ChannelStatus::Reconnecting {
                attempt: 2,
                next_retry_ms: 440,
            }
            .to_string(),
            "reconnecting (attempt 2, next in 440ms)"
        );
        assert_eq!(
            ChannelStatus::Failed {
                error: "timeout".into()
            }
            .to_string(),
            "failed: timeout"
        );
    }
}
