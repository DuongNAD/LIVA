use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::sync::broadcast;

/// Sensitivity classification of an incoming perception or session event.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "snake_case")]
pub enum EventSensitivity {
    #[default]
    Public,
    Internal,
    SensitivePii,
    Secret,
}

/// Strongly-typed payload containing specific sensory or system event details.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum PerceptionPayload {
    /// Audio speech recognition transcription event.
    VoiceUtterance {
        transcript: String,
        is_final: bool,
        confidence: f32,
        language: String,
        audio_duration_ms: u32,
    },
    /// Direct user interaction with the application UI or peripherals.
    UserAction {
        action_name: String,
        target_component: String,
        payload: serde_json::Value,
    },
    /// OS-level active foreground window or focus transition.
    ForegroundAppChanged {
        window_title: String,
        process_name: String,
        pid: u32,
        is_fullscreen: bool,
    },
    /// Scheduled task or reminder deadline triggered.
    TaskDue {
        task_id: String,
        title: String,
        due_timestamp_ms: i64,
        priority: String,
    },
    /// System hardware load (CPU, GPU, RAM) or governor mode transition.
    SystemPressureChanged {
        cpu_percent: u8,
        gpu_percent: Option<u8>,
        memory_used_mb: u64,
        game_mode_active: bool,
    },
    /// Screen visual perception or OCR capture event.
    ScreenObservation {
        ocr_text: Option<String>,
        active_region: Option<String>,
        visual_summary: Option<String>,
    },
    /// External IoT or smart home device status change.
    DeviceStateChanged {
        device_id: String,
        endpoint: String,
        new_state: serde_json::Value,
    },
}

/// Unified envelope representing any sensory input or external stimulus
/// entering the LIVA Cognitive Runtime.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PerceptionEvent {
    /// Unique event identifier (UUID v4).
    pub event_id: String,
    /// Epoch millisecond when the event occurred.
    pub timestamp_ms: i64,
    /// Calibrated confidence score (0.0 to 1.0).
    pub confidence_score: f32,
    /// Provenance / originating subsystem (e.g., "webrtc_voice", "desktop_ui", "governor").
    pub source_provenance: String,
    /// Privacy ownership domain (e.g., "memory_owner:local", "memory_owner:telegram:12345").
    pub owner_domain: String,
    /// Optional conversation or session context ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    /// Privacy / sensitivity classification.
    pub sensitivity: EventSensitivity,
    /// Typed event payload.
    pub payload: PerceptionPayload,
}

impl PerceptionEvent {
    /// Creates a generic PerceptionEvent with auto-generated UUID and current timestamp.
    pub fn new(source_provenance: impl Into<String>, payload: PerceptionPayload) -> Self {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        Self {
            event_id: uuid::Uuid::new_v4().to_string(),
            timestamp_ms: now_ms,
            confidence_score: 1.0,
            source_provenance: source_provenance.into(),
            owner_domain: "memory_owner:local".to_string(),
            conversation_id: None,
            sensitivity: EventSensitivity::Public,
            payload,
        }
    }

    /// Helper constructor for voice transcription events.
    pub fn voice_utterance(
        transcript: impl Into<String>,
        is_final: bool,
        confidence: f32,
        language: impl Into<String>,
        audio_duration_ms: u32,
    ) -> Self {
        let payload = PerceptionPayload::VoiceUtterance {
            transcript: transcript.into(),
            is_final,
            confidence,
            language: language.into(),
            audio_duration_ms,
        };
        let mut evt = Self::new("webrtc_voice", payload);
        evt.confidence_score = confidence;
        evt
    }

    /// Helper constructor for user action events.
    pub fn user_action(
        action_name: impl Into<String>,
        target_component: impl Into<String>,
        payload: serde_json::Value,
    ) -> Self {
        Self::new(
            "desktop_ui",
            PerceptionPayload::UserAction {
                action_name: action_name.into(),
                target_component: target_component.into(),
                payload,
            },
        )
    }

    /// Helper constructor for OS foreground application changes.
    pub fn foreground_app_changed(
        window_title: impl Into<String>,
        process_name: impl Into<String>,
        pid: u32,
        is_fullscreen: bool,
    ) -> Self {
        Self::new(
            "governor",
            PerceptionPayload::ForegroundAppChanged {
                window_title: window_title.into(),
                process_name: process_name.into(),
                pid,
                is_fullscreen,
            },
        )
    }

    /// Helper constructor for task due deadline notifications.
    pub fn task_due(
        task_id: impl Into<String>,
        title: impl Into<String>,
        due_timestamp_ms: i64,
        priority: impl Into<String>,
    ) -> Self {
        Self::new(
            "task_scheduler",
            PerceptionPayload::TaskDue {
                task_id: task_id.into(),
                title: title.into(),
                due_timestamp_ms,
                priority: priority.into(),
            },
        )
    }

    /// Helper constructor for system resource pressure transitions.
    pub fn system_pressure_changed(
        cpu_percent: u8,
        gpu_percent: Option<u8>,
        memory_used_mb: u64,
        game_mode_active: bool,
    ) -> Self {
        Self::new(
            "governor",
            PerceptionPayload::SystemPressureChanged {
                cpu_percent,
                gpu_percent,
                memory_used_mb,
                game_mode_active,
            },
        )
    }

    /// Fluent builder method to set confidence score.
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence_score = confidence.clamp(0.0, 1.0);
        self
    }

    /// Fluent builder method to set ownership domain.
    pub fn with_owner_domain(mut self, domain: impl Into<String>) -> Self {
        self.owner_domain = domain.into();
        self
    }

    /// Fluent builder method to set conversation ID.
    pub fn with_conversation_id(mut self, conversation_id: impl Into<String>) -> Self {
        self.conversation_id = Some(conversation_id.into());
        self
    }

    /// Fluent builder method to set sensitivity.
    pub fn with_sensitivity(mut self, sensitivity: EventSensitivity) -> Self {
        self.sensitivity = sensitivity;
        self
    }
}

// ── Append-Only Session Event Stream (Feature F6) ───────────────────────────

/// Append-only deterministic session event stream representation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "event", content = "payload", rename_all = "snake_case")]
pub enum SessionEvent {
    /// Injected system/user prompt token metadata.
    PromptInjected {
        session_id: String,
        prompt_tokens: usize,
    },
    /// Real-time streaming Chain-of-Thought reasoning token.
    ReasoningChunk { session_id: String, token: String },
    /// Real-time streaming user-visible content token.
    ContentChunk { session_id: String, token: String },
    /// Proposed tool invocation prior to execution.
    ToolCallProposed {
        session_id: String,
        tool: String,
        args: serde_json::Value,
    },
    /// Executed tool result observation.
    ToolCallExecuted {
        session_id: String,
        tool: String,
        result: serde_json::Value,
        duration_ms: u64,
    },
    /// Session conclusion with aggregate token metrics and latency.
    SessionFinished {
        session_id: String,
        total_tokens: usize,
        latency_ms: u64,
    },
    /// Custom typed event with explicit sensitivity classification.
    Custom {
        session_id: String,
        event_type: String,
        data: serde_json::Value,
        sensitivity: EventSensitivity,
    },
}

impl SessionEvent {
    pub fn session_id(&self) -> &str {
        match self {
            Self::PromptInjected { session_id, .. } => session_id,
            Self::ReasoningChunk { session_id, .. } => session_id,
            Self::ContentChunk { session_id, .. } => session_id,
            Self::ToolCallProposed { session_id, .. } => session_id,
            Self::ToolCallExecuted { session_id, .. } => session_id,
            Self::SessionFinished { session_id, .. } => session_id,
            Self::Custom { session_id, .. } => session_id,
        }
    }

    pub fn event_name(&self) -> &'static str {
        match self {
            Self::PromptInjected { .. } => "prompt_injected",
            Self::ReasoningChunk { .. } => "reasoning_chunk",
            Self::ContentChunk { .. } => "content_chunk",
            Self::ToolCallProposed { .. } => "tool_call_proposed",
            Self::ToolCallExecuted { .. } => "tool_call_executed",
            Self::SessionFinished { .. } => "session_finished",
            Self::Custom { .. } => "custom",
        }
    }

    pub fn prompt_injected(session_id: impl Into<String>, prompt_tokens: usize) -> Self {
        Self::PromptInjected {
            session_id: session_id.into(),
            prompt_tokens,
        }
    }

    pub fn reasoning_chunk(session_id: impl Into<String>, token: impl Into<String>) -> Self {
        Self::ReasoningChunk {
            session_id: session_id.into(),
            token: token.into(),
        }
    }

    pub fn content_chunk(session_id: impl Into<String>, token: impl Into<String>) -> Self {
        Self::ContentChunk {
            session_id: session_id.into(),
            token: token.into(),
        }
    }

    pub fn tool_call_proposed(
        session_id: impl Into<String>,
        tool: impl Into<String>,
        args: serde_json::Value,
    ) -> Self {
        Self::ToolCallProposed {
            session_id: session_id.into(),
            tool: tool.into(),
            args,
        }
    }

    pub fn tool_call_executed(
        session_id: impl Into<String>,
        tool: impl Into<String>,
        result: serde_json::Value,
        duration_ms: u64,
    ) -> Self {
        Self::ToolCallExecuted {
            session_id: session_id.into(),
            tool: tool.into(),
            result,
            duration_ms,
        }
    }

    pub fn session_finished(
        session_id: impl Into<String>,
        total_tokens: usize,
        latency_ms: u64,
    ) -> Self {
        Self::SessionFinished {
            session_id: session_id.into(),
            total_tokens,
            latency_ms,
        }
    }

    pub fn custom(
        session_id: impl Into<String>,
        event_type: impl Into<String>,
        data: serde_json::Value,
        sensitivity: EventSensitivity,
    ) -> Self {
        Self::Custom {
            session_id: session_id.into(),
            event_type: event_type.into(),
            data,
            sensitivity,
        }
    }

    /// Redacts secrets and sensitive information from the event payload.
    pub fn redact(&self) -> Self {
        match self {
            Self::Custom {
                session_id,
                event_type,
                data,
                sensitivity,
            } => {
                if *sensitivity == EventSensitivity::Secret
                    || *sensitivity == EventSensitivity::SensitivePii
                {
                    let scrubbed = match data {
                        serde_json::Value::String(s) => {
                            serde_json::Value::String(crate::cognitive::SecretScrubber::scrub(s))
                        }
                        serde_json::Value::Object(_) => {
                            crate::cognitive::SecretScrubber::scrub_json(data)
                        }
                        other => other.clone(),
                    };
                    Self::Custom {
                        session_id: session_id.clone(),
                        event_type: event_type.clone(),
                        data: scrubbed,
                        sensitivity: *sensitivity,
                    }
                } else {
                    self.clone()
                }
            }
            Self::ToolCallProposed {
                session_id,
                tool,
                args,
            } => Self::ToolCallProposed {
                session_id: session_id.clone(),
                tool: tool.clone(),
                args: crate::cognitive::SecretScrubber::scrub_json(args),
            },
            Self::ToolCallExecuted {
                session_id,
                tool,
                result,
                duration_ms,
            } => Self::ToolCallExecuted {
                session_id: session_id.clone(),
                tool: tool.clone(),
                result: crate::cognitive::SecretScrubber::scrub_json(result),
                duration_ms: *duration_ms,
            },
            other => other.clone(),
        }
    }
}

/// Append-only deterministic session event stream with multi-subscriber broadcast.
#[derive(Clone, Debug)]
pub struct SessionEventStream {
    pub tx: broadcast::Sender<SessionEvent>,
    history: Arc<RwLock<Vec<SessionEvent>>>,
}

impl SessionEventStream {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self {
            tx,
            history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Subscribes to new session events as they are published.
    pub fn subscribe(&self) -> broadcast::Receiver<SessionEvent> {
        self.tx.subscribe()
    }

    /// Publishes a session event, appending it to the history ledger and broadcasting to subscribers.
    pub async fn publish(&self, event: SessionEvent) -> Result<usize, String> {
        {
            let mut h = self.history.write().await;
            h.push(event.clone());
        }
        let send_res = self.tx.send(event);
        match send_res {
            Ok(subscribers) => Ok(subscribers),
            Err(_) => Ok(0), // Zero active subscribers is normal when no UI listener is attached
        }
    }

    /// Replays historical events for a given session ID in strict FIFO sequence.
    pub async fn replay(&self, session_id: &str) -> Vec<SessionEvent> {
        let h = self.history.read().await;
        h.iter()
            .filter(|e| e.session_id() == session_id)
            .cloned()
            .collect()
    }

    /// Clears historical events for a concluded session ID.
    pub async fn clear_session(&self, session_id: &str) {
        let mut h = self.history.write().await;
        h.retain(|e| e.session_id() != session_id);
    }

    /// Redacts secrets and PII from a session event.
    pub fn redact_event(event: &SessionEvent) -> SessionEvent {
        event.redact()
    }
}

impl Default for SessionEventStream {
    fn default() -> Self {
        Self::new(256)
    }
}
