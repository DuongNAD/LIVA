//! 3-Tier Priority Mailbox for Lock-Free Actor Messaging
//!
//! Implements strict priority scheduling with Tokio biased select:
//! Messages with `MessagePriority::High` (Sentinel Vetoes, Emergency Signals) are ALWAYS
//! dequeued before `Normal` (Tasks, Diff Hunks, Reviews) and `Low` (Heartbeats, Telemetry).

use super::types::{ActorError, MessagePriority, SwarmMessage};
use tokio::sync::mpsc;

/// Default channel buffer capacities for priority mailbox tiers.
pub const DEFAULT_HIGH_CAPACITY: usize = 128;
pub const DEFAULT_NORMAL_CAPACITY: usize = 512;
pub const DEFAULT_LOW_CAPACITY: usize = 1024;

/// Cloneable multi-producer sender handle for an actor's priority mailbox.
#[derive(Clone, Debug)]
pub struct PriorityMailboxSender {
    high_tx: mpsc::Sender<SwarmMessage>,
    normal_tx: mpsc::Sender<SwarmMessage>,
    low_tx: mpsc::Sender<SwarmMessage>,
}

/// Single-consumer receiver handle for an actor's priority mailbox.
pub struct PriorityMailboxReceiver {
    high_rx: mpsc::Receiver<SwarmMessage>,
    normal_rx: mpsc::Receiver<SwarmMessage>,
    low_rx: mpsc::Receiver<SwarmMessage>,
}

/// Creates a new paired Priority Mailbox (Sender, Receiver) with configured capacities.
pub fn create_priority_mailbox(
    high_cap: usize,
    normal_cap: usize,
    low_cap: usize,
) -> (PriorityMailboxSender, PriorityMailboxReceiver) {
    let (h_tx, h_rx) = mpsc::channel(high_cap.max(1));
    let (n_tx, n_rx) = mpsc::channel(normal_cap.max(1));
    let (l_tx, l_rx) = mpsc::channel(low_cap.max(1));

    (
        PriorityMailboxSender {
            high_tx: h_tx,
            normal_tx: n_tx,
            low_tx: l_tx,
        },
        PriorityMailboxReceiver {
            high_rx: h_rx,
            normal_rx: n_rx,
            low_rx: l_rx,
        },
    )
}

/// Creates a Priority Mailbox with standard default capacities.
pub fn default_priority_mailbox() -> (PriorityMailboxSender, PriorityMailboxReceiver) {
    create_priority_mailbox(
        DEFAULT_HIGH_CAPACITY,
        DEFAULT_NORMAL_CAPACITY,
        DEFAULT_LOW_CAPACITY,
    )
}

impl PriorityMailboxSender {
    /// Asynchronously sends a message into the appropriate priority channel.
    pub async fn send(&self, msg: SwarmMessage) -> Result<(), ActorError> {
        match msg.priority {
            MessagePriority::High => self
                .high_tx
                .send(msg)
                .await
                .map_err(|e| ActorError::MailboxSendError(e.to_string())),
            MessagePriority::Normal => self
                .normal_tx
                .send(msg)
                .await
                .map_err(|e| ActorError::MailboxSendError(e.to_string())),
            MessagePriority::Low => self
                .low_tx
                .send(msg)
                .await
                .map_err(|e| ActorError::MailboxSendError(e.to_string())),
        }
    }

    /// Non-blocking attempt to send a message into the appropriate priority channel.
    pub fn try_send(&self, msg: SwarmMessage) -> Result<(), ActorError> {
        match msg.priority {
            MessagePriority::High => self
                .high_tx
                .try_send(msg)
                .map_err(|e| ActorError::MailboxSendError(e.to_string())),
            MessagePriority::Normal => self
                .normal_tx
                .try_send(msg)
                .map_err(|e| ActorError::MailboxSendError(e.to_string())),
            MessagePriority::Low => self
                .low_tx
                .try_send(msg)
                .map_err(|e| ActorError::MailboxSendError(e.to_string())),
        }
    }

    /// Returns whether any receiver is still connected.
    pub fn is_closed(&self) -> bool {
        self.high_tx.is_closed() && self.normal_tx.is_closed() && self.low_tx.is_closed()
    }
}

impl PriorityMailboxReceiver {
    /// Asynchronously dequeues the next message with deterministic priority:
    /// High-priority queue is ALWAYS drained first, followed by Normal, then Low.
    pub async fn recv(&mut self) -> Option<SwarmMessage> {
        tokio::select! {
            biased;
            Some(msg) = self.high_rx.recv() => Some(msg),
            Some(msg) = self.normal_rx.recv() => Some(msg),
            Some(msg) = self.low_rx.recv() => Some(msg),
            else => None,
        }
    }

    /// Non-blocking poll for the next available message in strict priority order.
    pub fn try_recv(&mut self) -> Option<SwarmMessage> {
        if let Ok(msg) = self.high_rx.try_recv() {
            return Some(msg);
        }
        if let Ok(msg) = self.normal_rx.try_recv() {
            return Some(msg);
        }
        if let Ok(msg) = self.low_rx.try_recv() {
            return Some(msg);
        }
        None
    }

    /// Closes all underlying channels.
    pub fn close(&mut self) {
        self.high_rx.close();
        self.normal_rx.close();
        self.low_rx.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::swarm::types::{SwarmPayload, SwarmRole};
    use crate::agent::swarm::vector_clock::VectorClock;

    fn make_msg(priority: MessagePriority, id: &str) -> SwarmMessage {
        let mut msg = SwarmMessage::new(
            SwarmRole::Coder,
            Some(SwarmRole::Reviewer),
            priority,
            SwarmPayload::TaskProgress {
                task_id: id.to_string(),
                step_index: 0,
                status: "test".to_string(),
                output: None,
            },
            VectorClock::new(),
        );
        msg.message_id = id.to_string();
        msg
    }

    #[tokio::test]
    async fn test_priority_scheduling_biased_order() {
        let (tx, mut rx) = create_priority_mailbox(10, 10, 10);

        // Send Low first, then Normal, then High
        tx.send(make_msg(MessagePriority::Low, "low_1")).await.unwrap();
        tx.send(make_msg(MessagePriority::Normal, "normal_1")).await.unwrap();
        tx.send(make_msg(MessagePriority::High, "high_1")).await.unwrap();
        tx.send(make_msg(MessagePriority::Low, "low_2")).await.unwrap();
        tx.send(make_msg(MessagePriority::Normal, "normal_2")).await.unwrap();
        tx.send(make_msg(MessagePriority::High, "high_2")).await.unwrap();

        // High priority messages must arrive first!
        let m1 = rx.recv().await.unwrap();
        assert_eq!(m1.priority, MessagePriority::High);
        assert_eq!(m1.message_id, "high_1");

        let m2 = rx.recv().await.unwrap();
        assert_eq!(m2.priority, MessagePriority::High);
        assert_eq!(m2.message_id, "high_2");

        // Normal priority next
        let m3 = rx.recv().await.unwrap();
        assert_eq!(m3.priority, MessagePriority::Normal);
        assert_eq!(m3.message_id, "normal_1");

        let m4 = rx.recv().await.unwrap();
        assert_eq!(m4.priority, MessagePriority::Normal);
        assert_eq!(m4.message_id, "normal_2");

        // Low priority last
        let m5 = rx.recv().await.unwrap();
        assert_eq!(m5.priority, MessagePriority::Low);
        assert_eq!(m5.message_id, "low_1");

        let m6 = rx.recv().await.unwrap();
        assert_eq!(m6.priority, MessagePriority::Low);
        assert_eq!(m6.message_id, "low_2");
    }

    #[test]
    fn test_try_send_and_try_recv() {
        let (tx, mut rx) = create_priority_mailbox(2, 2, 2);

        tx.try_send(make_msg(MessagePriority::Low, "l1")).unwrap();
        tx.try_send(make_msg(MessagePriority::High, "h1")).unwrap();

        // High should be picked first by try_recv
        let r1 = rx.try_recv().unwrap();
        assert_eq!(r1.message_id, "h1");

        let r2 = rx.try_recv().unwrap();
        assert_eq!(r2.message_id, "l1");

        assert!(rx.try_recv().is_none());
    }
}
