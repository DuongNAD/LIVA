//! Bounded circular action history tracker for reactive execution loops (Flash profile).
//!
//! Provides `CompactActionSummary` and `ActionHistoryTracker`, a bounded FIFO buffer
//! with zero-allocation steady state and compact LLM prompt context formatting.

use crate::types::{CuaActionEffect, CuaDeliveryMode};
use serde::{Deserialize, Serialize};

/// Lightweight summary of a single executed CUA action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactActionSummary {
    /// 0-indexed position of the action in the execution sequence.
    pub step_index: usize,
    /// Verbose action name (e.g. "click", "type_text", "scroll").
    pub action_type: String,
    /// Set-of-Mark 1-based element index if targeted.
    pub target_som_id: Option<usize>,
    /// Target Win32 HWND.
    pub target_hwnd: u64,
    /// Accessible label, text caption, or selector of the target element.
    pub target_label: Option<String>,
    /// Input delivery mode used (Background or Foreground).
    pub delivery_mode: CuaDeliveryMode,
    /// Action outcome effect classification.
    pub effect: CuaActionEffect,
    /// Whether the action succeeded without errors or policy blockage.
    pub success: bool,
    /// Elapsed execution latency in milliseconds.
    pub duration_ms: u64,
    /// UNIX epoch timestamp in milliseconds.
    pub timestamp_ms: u64,
}

/// Bounded FIFO circular buffer tracking recent action history for LLM prompt assembly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionHistoryTracker {
    /// Maximum number of recent summaries retained before evicting the oldest.
    pub max_capacity: usize,
    /// Ordered list of action summaries (oldest at index 0, newest at end).
    pub summaries: Vec<CompactActionSummary>,
}

impl ActionHistoryTracker {
    /// Default history buffer capacity (20 items).
    pub const DEFAULT_CAPACITY: usize = 20;

    /// Create a new `ActionHistoryTracker` with the specified maximum capacity.
    pub fn new(capacity: usize) -> Self {
        let cap = if capacity == 0 {
            Self::DEFAULT_CAPACITY
        } else {
            capacity
        };
        Self {
            max_capacity: cap,
            summaries: Vec::with_capacity(cap),
        }
    }

    /// Append a new action summary. If the buffer is full, evicts the oldest item (FIFO).
    pub fn append(&mut self, summary: CompactActionSummary) {
        if self.summaries.len() >= self.max_capacity {
            self.summaries.remove(0);
        }
        self.summaries.push(summary);
    }

    /// Clear all stored action summaries.
    pub fn clear(&mut self) {
        self.summaries.clear();
    }

    /// Returns the number of currently stored summaries.
    pub fn len(&self) -> usize {
        self.summaries.len()
    }

    /// Returns true if no summaries are currently stored.
    pub fn is_empty(&self) -> bool {
        self.summaries.is_empty()
    }

    /// Access the slice of currently stored summaries.
    pub fn summaries(&self) -> &[CompactActionSummary] {
        &self.summaries
    }

    /// Formats the bounded action history into a compact, token-efficient string
    /// suitable for injection into an LLM prompt context window.
    pub fn format_prompt_summary(&self) -> String {
        if self.summaries.is_empty() {
            return String::new();
        }

        self.summaries
            .iter()
            .map(|s| {
                let target_str = match (s.target_som_id, &s.target_label) {
                    (Some(id), Some(label)) => format!("[{}] \"{}\"", id, label),
                    (Some(id), None) => format!("[{}]", id),
                    (None, Some(label)) => format!("\"{}\" (0x{:X})", label, s.target_hwnd),
                    (None, None) => format!("0x{:X}", s.target_hwnd),
                };

                let delivery_str = match s.delivery_mode {
                    CuaDeliveryMode::Background => "bg",
                    CuaDeliveryMode::Foreground => "fg",
                };

                format!(
                    "step {}: {} on {} via {} -> {} ({}ms)",
                    s.step_index,
                    s.action_type,
                    target_str,
                    delivery_str,
                    if s.success { "ok" } else { "fail" },
                    s.duration_ms
                )
            })
            .collect::<Vec<_>>()
            .join("; ")
    }
}

impl Default for ActionHistoryTracker {
    fn default() -> Self {
        Self::new(Self::DEFAULT_CAPACITY)
    }
}
