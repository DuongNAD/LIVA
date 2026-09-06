//! In-memory telemetry profiler, Prometheus metric registry, and OpenTelemetry distributed tracing (Milestone 4).
//!
//! Provides zero-allocation, sliding-window telemetry capture and lock-free metric instruments for:
//! - Time-To-First-Token (TTFT) distribution across prompts and models with standard Prometheus buckets.
//! - Token velocity and generation throughput gauges.
//! - Worker queue depth metrics categorized by priority (`voice`, `user`, `background`).
//! - Preemptive task cancellation counters (`liva_llm_preemptions_total`).
//! - SQLite connection pool wait and query duration latency histograms.
//! - Process memory RSS bytes and CPU utilization time series.
//! - W3C `traceparent` distributed context propagation across IPC, Tokio tasks, and async channels.
//! - Prometheus text exposition format (version 0.0.4).

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fmt::Write as FmtWrite;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{OnceLock, RwLock};
use std::time::Duration;

// ============================================================================
// W3C TRACE CONTEXT & DISTRIBUTED TRACING
// ============================================================================

/// W3C distributed tracing context header for OpenTelemetry interoperability.
///
/// Implements W3C Trace Context Level 1 specification (`traceparent` format):
/// `version - trace_id - parent_id - trace_flags`
///
/// Example: `00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TraceContext {
    /// Specification version (currently `0x00`).
    pub version: u8,
    /// 16-byte (128-bit) globally unique identifier representing the entire trace tree.
    pub trace_id: [u8; 16],
    /// 8-byte (64-bit) identifier representing the current span / parent caller.
    pub parent_id: [u8; 8],
    /// 8-bit trace flags bitmask (bit 0 indicates sampling: `0x01` = sampled).
    pub trace_flags: u8,
}

/// Errors occurring during W3C `traceparent` parsing and validation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TraceContextError {
    #[error("Invalid traceparent length or delimiter format")]
    InvalidFormat,
    #[error("Unsupported or invalid traceparent version: {0:#04x}")]
    InvalidVersion(u8),
    #[error("Invalid trace_id hex encoding")]
    InvalidTraceIdHex,
    #[error("Invalid trace_id: all zeros is forbidden by W3C specification")]
    AllZerosTraceId,
    #[error("Invalid parent_id hex encoding")]
    InvalidParentIdHex,
    #[error("Invalid parent_id: all zeros is forbidden by W3C specification")]
    AllZerosParentId,
    #[error("Invalid trace_flags hex encoding")]
    InvalidTraceFlagsHex,
}

impl TraceContext {
    /// Flag indicating that this trace has been sampled for collection.
    pub const FLAG_SAMPLED: u8 = 0x01;

    /// Generate a new root trace context with randomly generated non-zero `trace_id`
    /// and `parent_id`, version 0, and sampled flag enabled.
    pub fn new() -> Self {
        Self::with_sampled(true)
    }

    /// Generate a new root trace context with explicit sampling flag.
    pub fn with_sampled(sampled: bool) -> Self {
        let mut trace_id = [0u8; 16];
        let mut parent_id = [0u8; 8];

        while trace_id.iter().all(|&b| b == 0) {
            rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut trace_id);
        }
        while parent_id.iter().all(|&b| b == 0) {
            rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut parent_id);
        }

        Self {
            version: 0,
            trace_id,
            parent_id,
            trace_flags: if sampled { Self::FLAG_SAMPLED } else { 0x00 },
        }
    }

    /// Create a child span context within the same distributed trace tree.
    ///
    /// Preserves `version`, `trace_id`, and `trace_flags`, while generating
    /// a new unique 64-bit `parent_id` for the child operation.
    pub fn child_context(&self) -> Self {
        let mut new_parent_id = [0u8; 8];
        while new_parent_id.iter().all(|&b| b == 0) {
            rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut new_parent_id);
        }

        Self {
            version: self.version,
            trace_id: self.trace_id,
            parent_id: new_parent_id,
            trace_flags: self.trace_flags,
        }
    }

    /// Check if the trace context has the sampled flag enabled.
    pub fn is_sampled(&self) -> bool {
        (self.trace_flags & Self::FLAG_SAMPLED) != 0
    }

    /// Update the sampling state of this trace context.
    pub fn set_sampled(&mut self, sampled: bool) {
        if sampled {
            self.trace_flags |= Self::FLAG_SAMPLED;
        } else {
            self.trace_flags &= !Self::FLAG_SAMPLED;
        }
    }

    /// Format `trace_id` as a 32-character lowercase hex string.
    pub fn trace_id_hex(&self) -> String {
        hex::encode(self.trace_id)
    }

    /// Format `parent_id` as a 16-character lowercase hex string.
    pub fn parent_id_hex(&self) -> String {
        hex::encode(self.parent_id)
    }

    /// Serialize context to a standard W3C `traceparent` header string.
    ///
    /// Output format: `00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01`
    pub fn to_traceparent(&self) -> String {
        format!(
            "{:02x}-{}-{}-{:02x}",
            self.version,
            hex::encode(self.trace_id),
            hex::encode(self.parent_id),
            self.trace_flags
        )
    }

    /// Parse and validate a W3C `traceparent` header string.
    pub fn from_traceparent(s: &str) -> Result<Self, TraceContextError> {
        let s = s.trim();
        let parts: Vec<&str> = s.split('-').collect();
        if parts.len() < 4 {
            return Err(TraceContextError::InvalidFormat);
        }

        // 1. Version (2 hex chars)
        let version_str = parts[0];
        if version_str.len() != 2 {
            return Err(TraceContextError::InvalidFormat);
        }
        let version = u8::from_str_radix(version_str, 16)
            .map_err(|_| TraceContextError::InvalidFormat)?;
        if version == 0xff {
            return Err(TraceContextError::InvalidVersion(version));
        }

        // For version 00, exactly 4 parts are allowed by specification
        if version == 0 && parts.len() != 4 {
            return Err(TraceContextError::InvalidFormat);
        }

        // 2. Trace ID (32 hex chars = 16 bytes)
        let trace_id_str = parts[1];
        if trace_id_str.len() != 32 {
            return Err(TraceContextError::InvalidFormat);
        }
        let mut trace_id = [0u8; 16];
        hex::decode_to_slice(trace_id_str, &mut trace_id)
            .map_err(|_| TraceContextError::InvalidTraceIdHex)?;
        if trace_id.iter().all(|&b| b == 0) {
            return Err(TraceContextError::AllZerosTraceId);
        }

        // 3. Parent ID (16 hex chars = 8 bytes)
        let parent_id_str = parts[2];
        if parent_id_str.len() != 16 {
            return Err(TraceContextError::InvalidFormat);
        }
        let mut parent_id = [0u8; 8];
        hex::decode_to_slice(parent_id_str, &mut parent_id)
            .map_err(|_| TraceContextError::InvalidParentIdHex)?;
        if parent_id.iter().all(|&b| b == 0) {
            return Err(TraceContextError::AllZerosParentId);
        }

        // 4. Trace Flags (2 hex chars = 1 byte)
        let flags_str = parts[3];
        if flags_str.len() != 2 {
            return Err(TraceContextError::InvalidFormat);
        }
        let trace_flags = u8::from_str_radix(flags_str, 16)
            .map_err(|_| TraceContextError::InvalidTraceFlagsHex)?;

        Ok(Self {
            version,
            trace_id,
            parent_id,
            trace_flags,
        })
    }
}

impl Default for TraceContext {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TraceContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_traceparent())
    }
}

impl std::str::FromStr for TraceContext {
    type Err = TraceContextError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_traceparent(s)
    }
}

// ============================================================================
// LOCK-FREE ATOMIC FLOAT & ATOMIC HISTOGRAM INSTRUMENTS
// ============================================================================

/// Lock-free atomic 64-bit floating-point register.
#[derive(Debug, Default)]
pub struct AtomicF64 {
    bits: AtomicU64,
}

impl AtomicF64 {
    pub const fn new(val: f64) -> Self {
        Self {
            bits: AtomicU64::new(val.to_bits()),
        }
    }

    pub fn load(&self, order: Ordering) -> f64 {
        f64::from_bits(self.bits.load(order))
    }

    pub fn store(&self, val: f64, order: Ordering) {
        self.bits.store(val.to_bits(), order);
    }

    pub fn fetch_add(&self, val: f64, order: Ordering) -> f64 {
        let mut current_bits = self.bits.load(Ordering::Relaxed);
        loop {
            let current = f64::from_bits(current_bits);
            let new = current + val;
            let new_bits = new.to_bits();
            match self.bits.compare_exchange_weak(
                current_bits,
                new_bits,
                order,
                Ordering::Relaxed,
            ) {
                Ok(_) => return current,
                Err(actual) => current_bits = actual,
            }
        }
    }
}

/// Standard latency buckets for TTFT (in seconds):
/// 0.025s (25ms), 0.050s (50ms), 0.100s (100ms), 0.250s (250ms), 0.500s (500ms), 1.0s, 2.5s
pub const STANDARD_TTFT_BUCKETS_SECONDS: &[f64] = &[0.025, 0.050, 0.100, 0.250, 0.500, 1.0, 2.5];

/// Standard latency buckets for database pool connection wait time (in milliseconds):
pub const STANDARD_DB_POOL_WAIT_BUCKETS_MS: &[f64] = &[0.1, 0.5, 1.0, 2.5, 5.0, 10.0, 25.0, 50.0, 100.0, 500.0];

/// Standard latency buckets for database query duration (in milliseconds):
pub const STANDARD_DB_QUERY_BUCKETS_MS: &[f64] = &[0.2, 0.5, 1.0, 2.5, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0];

/// Lock-free cumulative histogram instrument matching Prometheus exposition rules.
pub struct AtomicHistogram {
    name: String,
    help: String,
    thresholds: Vec<f64>,
    buckets: Vec<AtomicU64>,
    count: AtomicU64,
    sum: AtomicF64,
}

impl AtomicHistogram {
    pub fn new(name: impl Into<String>, help: impl Into<String>, thresholds: &[f64]) -> Self {
        let buckets = (0..thresholds.len()).map(|_| AtomicU64::new(0)).collect();
        Self {
            name: name.into(),
            help: help.into(),
            thresholds: thresholds.to_vec(),
            buckets,
            count: AtomicU64::new(0),
            sum: AtomicF64::new(0.0),
        }
    }

    /// Record an observed value into the histogram.
    ///
    /// Increments all cumulative bucket thresholds $\ge \text{val}$, increments
    /// total count, and adds to total observation sum atomically without lock contention.
    pub fn record(&self, val: f64) {
        if val.is_nan() || val < 0.0 {
            return;
        }
        for (i, &threshold) in self.thresholds.iter().enumerate() {
            if val <= threshold {
                self.buckets[i].fetch_add(1, Ordering::Relaxed);
            }
        }
        self.count.fetch_add(1, Ordering::Relaxed);
        self.sum.fetch_add(val, Ordering::Relaxed);
    }

    pub fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }

    pub fn sum(&self) -> f64 {
        self.sum.load(Ordering::Relaxed)
    }

    pub fn reset(&self) {
        for b in &self.buckets {
            b.store(0, Ordering::Relaxed);
        }
        self.count.store(0, Ordering::Relaxed);
        self.sum.store(0.0, Ordering::Relaxed);
    }

    /// Format cumulative buckets, sum, and count in standard Prometheus text exposition format.
    pub fn export_prometheus(&self, out: &mut String) {
        let _ = writeln!(out, "# HELP {} {}", self.name, self.help);
        let _ = writeln!(out, "# TYPE {} histogram", self.name);

        let total_count = self.count.load(Ordering::Relaxed);
        for (i, threshold) in self.thresholds.iter().enumerate() {
            let bucket_count = self.buckets[i].load(Ordering::Relaxed);
            let thresh_str = format_bucket_label(*threshold);
            let _ = writeln!(
                out,
                "{}_bucket{{le=\"{}\"}} {}",
                self.name, thresh_str, bucket_count
            );
        }
        let _ = writeln!(out, "{}_bucket{{le=\"+Inf\"}} {}", self.name, total_count);
        let _ = writeln!(out, "{}_sum {:.6}", self.name, self.sum.load(Ordering::Relaxed));
        let _ = writeln!(out, "{}_count {}", self.name, total_count);
    }
}

fn format_bucket_label(val: f64) -> String {
    if val.fract() == 0.0 {
        format!("{:.0}", val)
    } else {
        format!("{}", val)
    }
}

// ============================================================================
// DATA MODELS & PROFILER STRUCTURES
// ============================================================================

/// A single timestamped latency measurement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyRecord {
    pub timestamp_ms: i64,
    pub operation: String,
    pub latency_ms: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// A periodic hardware / process resource sample.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSample {
    pub timestamp_ms: i64,
    pub cpu_percent: Option<f32>,
    pub liva_cpu_percent: Option<f32>,
    pub gpu_percent: Option<u8>,
    pub rss_bytes: Option<u64>,
    pub commit_bytes: Option<u64>,
}

/// A structured telemetry log entry matching the dashboard UI requirements.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEntry {
    pub level: String,
    pub time: String,
    pub category: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Statistical percentile summary for a specific latency metric.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyMetricsSummary {
    pub count: usize,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
    pub avg_ms: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_ms: Option<f64>,
}

impl Default for LatencyMetricsSummary {
    fn default() -> Self {
        Self {
            count: 0,
            p50_ms: 0.0,
            p95_ms: 0.0,
            min_ms: 0.0,
            max_ms: 0.0,
            avg_ms: 0.0,
            latest_ms: None,
        }
    }
}

/// Fixed-capacity in-memory ring buffer.
#[derive(Debug)]
struct RingBuffer<T> {
    buffer: VecDeque<T>,
    capacity: usize,
}

#[allow(dead_code)]
impl<T> RingBuffer<T> {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    fn push(&mut self, item: T) {
        if self.buffer.len() >= self.capacity {
            self.buffer.pop_front();
        }
        self.buffer.push_back(item);
    }

    fn items(&self) -> Vec<T>
    where
        T: Clone,
    {
        self.buffer.iter().cloned().collect()
    }

    fn len(&self) -> usize {
        self.buffer.len()
    }

    fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    fn clear(&mut self) {
        self.buffer.clear();
    }
}

// ============================================================================
// TELEMETRY PROFILER & PROMETHEUS REGISTRY
// ============================================================================

/// Thread-safe real-time telemetry profiler and Prometheus metrics exporter.
pub struct TelemetryProfiler {
    // Sliding-window ring buffers for dashboard UI & active diagnostic inspection
    ttft_ring: RwLock<RingBuffer<LatencyRecord>>,
    receive_stream_ring: RwLock<RingBuffer<LatencyRecord>>,
    ws_transit_ring: RwLock<RingBuffer<LatencyRecord>>,
    audio_latency_ring: RwLock<RingBuffer<LatencyRecord>>,
    resource_ring: RwLock<RingBuffer<ResourceSample>>,
    events_ring: RwLock<RingBuffer<TelemetryEntry>>,

    // Real-time lock-free Prometheus & OpenTelemetry metric instruments
    ttft_histogram: AtomicHistogram,
    tokens_per_second_gauge: AtomicF64,
    total_tokens_counter: AtomicU64,
    queue_depth_voice: AtomicUsize,
    queue_depth_user: AtomicUsize,
    queue_depth_background: AtomicUsize,
    preemptions_total: AtomicU64,
    db_pool_wait_histogram_ms: AtomicHistogram,
    db_query_histogram_ms: AtomicHistogram,
    process_rss_bytes_gauge: AtomicU64,
    process_cpu_percent_gauge: AtomicF64,
}

impl TelemetryProfiler {
    pub fn new() -> Self {
        Self {
            ttft_ring: RwLock::new(RingBuffer::with_capacity(128)),
            receive_stream_ring: RwLock::new(RingBuffer::with_capacity(128)),
            ws_transit_ring: RwLock::new(RingBuffer::with_capacity(128)),
            audio_latency_ring: RwLock::new(RingBuffer::with_capacity(128)),
            resource_ring: RwLock::new(RingBuffer::with_capacity(120)), // 60s @ 2Hz
            events_ring: RwLock::new(RingBuffer::with_capacity(256)),

            ttft_histogram: AtomicHistogram::new(
                "liva_ttft_seconds",
                "Time-to-first-token latency in seconds.",
                STANDARD_TTFT_BUCKETS_SECONDS,
            ),
            tokens_per_second_gauge: AtomicF64::new(0.0),
            total_tokens_counter: AtomicU64::new(0),
            queue_depth_voice: AtomicUsize::new(0),
            queue_depth_user: AtomicUsize::new(0),
            queue_depth_background: AtomicUsize::new(0),
            preemptions_total: AtomicU64::new(0),
            db_pool_wait_histogram_ms: AtomicHistogram::new(
                "liva_db_pool_wait_duration_ms",
                "Database connection acquisition wait duration in milliseconds.",
                STANDARD_DB_POOL_WAIT_BUCKETS_MS,
            ),
            db_query_histogram_ms: AtomicHistogram::new(
                "liva_db_query_duration_ms",
                "Database query execution duration in milliseconds.",
                STANDARD_DB_QUERY_BUCKETS_MS,
            ),
            process_rss_bytes_gauge: AtomicU64::new(0),
            process_cpu_percent_gauge: AtomicF64::new(0.0),
        }
    }

    // ── Metric Ingestion & Updaters ──────────────────────────────────────────

    /// Record a Time-To-First-Token (TTFT) measurement.
    ///
    /// Updates in-memory sliding ring buffer AND the lock-free Prometheus TTFT histogram.
    pub fn record_ttft(&self, model: &str, latency_ms: f64, prompt_tokens: usize) {
        let record = LatencyRecord {
            timestamp_ms: Utc::now().timestamp_millis(),
            operation: "ttft".to_string(),
            latency_ms,
            metadata: Some(serde_json::json!({
                "model": model,
                "prompt_tokens": prompt_tokens,
            })),
        };

        if let Ok(mut guard) = self.ttft_ring.write() {
            guard.push(record);
        }

        // Record in seconds into the Prometheus histogram
        self.ttft_histogram.record(latency_ms / 1000.0);

        self.record_event(
            "info",
            "llm",
            &format!("TTFT: {:.1}ms (model: {}, tokens: {})", latency_ms, model, prompt_tokens),
            Some(serde_json::json!({
                "latency_ms": latency_ms,
                "model": model,
                "prompt_tokens": prompt_tokens
            })),
        );
    }

    /// Record a TTFT measurement from `std::time::Duration`.
    pub fn record_ttft_duration(&self, duration: Duration) {
        let latency_ms = duration.as_secs_f64() * 1000.0;
        self.record_ttft("default", latency_ms, 0);
    }

    /// Record generated token count and update instantaneous velocity gauge.
    pub fn record_tokens_generated(&self, count: u64, duration: Duration) {
        self.total_tokens_counter.fetch_add(count, Ordering::Relaxed);
        let secs = duration.as_secs_f64();
        if secs > 0.0 {
            let velocity = (count as f64) / secs;
            self.tokens_per_second_gauge.store(velocity, Ordering::Relaxed);
        }
    }

    /// Explicitly set the active token generation velocity gauge.
    pub fn set_token_velocity(&self, tokens_per_second: f64) {
        self.tokens_per_second_gauge.store(tokens_per_second, Ordering::Relaxed);
    }

    /// Update worker queue depth for a priority category (`voice`, `user`, `background`).
    pub fn set_queue_depth(&self, priority: &str, depth: usize) {
        match priority.to_lowercase().as_str() {
            "voice" | "realtimevoice" | "0" => self.queue_depth_voice.store(depth, Ordering::Relaxed),
            "user" | "interactiveuser" | "1" => self.queue_depth_user.store(depth, Ordering::Relaxed),
            "background" | "backgroundconsolidation" | "2" => {
                self.queue_depth_background.store(depth, Ordering::Relaxed)
            }
            _ => {}
        }
    }

    /// Increment worker queue depth for a priority category.
    pub fn inc_queue_depth(&self, priority: &str) {
        match priority.to_lowercase().as_str() {
            "voice" | "realtimevoice" | "0" => {
                self.queue_depth_voice.fetch_add(1, Ordering::Relaxed);
            }
            "user" | "interactiveuser" | "1" => {
                self.queue_depth_user.fetch_add(1, Ordering::Relaxed);
            }
            "background" | "backgroundconsolidation" | "2" => {
                self.queue_depth_background.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
    }

    /// Decrement worker queue depth for a priority category.
    pub fn dec_queue_depth(&self, priority: &str) {
        match priority.to_lowercase().as_str() {
            "voice" | "realtimevoice" | "0" => {
                let _ = self.queue_depth_voice.fetch_update(
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                    |d| Some(d.saturating_sub(1)),
                );
            }
            "user" | "interactiveuser" | "1" => {
                let _ = self.queue_depth_user.fetch_update(
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                    |d| Some(d.saturating_sub(1)),
                );
            }
            "background" | "backgroundconsolidation" | "2" => {
                let _ = self.queue_depth_background.fetch_update(
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                    |d| Some(d.saturating_sub(1)),
                );
            }
            _ => {}
        }
    }

    /// Read current worker queue depth for a priority category.
    pub fn get_queue_depth(&self, priority: &str) -> usize {
        match priority.to_lowercase().as_str() {
            "voice" | "realtimevoice" | "0" => self.queue_depth_voice.load(Ordering::Relaxed),
            "user" | "interactiveuser" | "1" => self.queue_depth_user.load(Ordering::Relaxed),
            "background" | "backgroundconsolidation" | "2" => {
                self.queue_depth_background.load(Ordering::Relaxed)
            }
            _ => 0,
        }
    }

    /// Record a preemptive task yield event.
    pub fn record_preemption(&self) {
        self.preemptions_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a batch of preemption events.
    pub fn record_preemptions(&self, count: u64) {
        self.preemptions_total.fetch_add(count, Ordering::Relaxed);
    }

    /// Total number of recorded LLM preemptions.
    pub fn get_preemptions(&self) -> u64 {
        self.preemptions_total.load(Ordering::Relaxed)
    }

    /// Record database pool connection checkout wait duration.
    pub fn record_db_pool_wait(&self, duration: Duration) {
        self.db_pool_wait_histogram_ms.record(duration.as_secs_f64() * 1000.0);
    }

    /// Record database query execution latency.
    pub fn record_db_query(&self, duration: Duration) {
        self.db_query_histogram_ms.record(duration.as_secs_f64() * 1000.0);
    }

    /// Record database read query latency.
    pub fn record_db_read_latency(&self, duration: Duration) {
        self.record_db_query(duration);
    }

    /// Record database write transaction latency.
    pub fn record_db_write_latency(&self, duration: Duration) {
        self.record_db_query(duration);
    }

    /// Record a receive-to-stream initiation latency.
    pub fn record_receive_to_stream(&self, path: &str, latency_ms: f64) {
        let record = LatencyRecord {
            timestamp_ms: Utc::now().timestamp_millis(),
            operation: "receive_to_stream".to_string(),
            latency_ms,
            metadata: Some(serde_json::json!({ "path": path })),
        };

        if let Ok(mut guard) = self.receive_stream_ring.write() {
            guard.push(record);
        }
    }

    /// Record a WebSocket binary or text chunk transit latency.
    pub fn record_ws_transit(&self, op_code: u8, transit_ms: f64, size_bytes: usize) {
        let record = LatencyRecord {
            timestamp_ms: Utc::now().timestamp_millis(),
            operation: "ws_transit".to_string(),
            latency_ms: transit_ms,
            metadata: Some(serde_json::json!({
                "op_code": op_code,
                "size_bytes": size_bytes
            })),
        };

        if let Ok(mut guard) = self.ws_transit_ring.write() {
            guard.push(record);
        }
    }

    /// Record an audio stage latency (STT, TTS first chunk, AEC, VAD).
    pub fn record_audio_latency(&self, stage: &str, latency_ms: f64) {
        let record = LatencyRecord {
            timestamp_ms: Utc::now().timestamp_millis(),
            operation: format!("audio:{}", stage),
            latency_ms,
            metadata: Some(serde_json::json!({ "stage": stage })),
        };

        if let Ok(mut guard) = self.audio_latency_ring.write() {
            guard.push(record);
        }
    }

    /// Record hardware / process resource usage sample.
    pub fn record_resource_sample(
        &self,
        cpu_percent: Option<f32>,
        liva_cpu_percent: Option<f32>,
        gpu_percent: Option<u8>,
        rss_bytes: Option<u64>,
        commit_bytes: Option<u64>,
    ) {
        let sample = ResourceSample {
            timestamp_ms: Utc::now().timestamp_millis(),
            cpu_percent,
            liva_cpu_percent,
            gpu_percent,
            rss_bytes,
            commit_bytes,
        };

        if let Some(rss) = rss_bytes {
            self.process_rss_bytes_gauge.store(rss, Ordering::Relaxed);
        }
        if let Some(cpu) = liva_cpu_percent.or(cpu_percent) {
            self.process_cpu_percent_gauge.store(cpu as f64, Ordering::Relaxed);
        }

        if let Ok(mut guard) = self.resource_ring.write() {
            guard.push(sample);
        }
    }

    /// Explicitly set process resident memory bytes gauge.
    pub fn set_process_rss_bytes(&self, bytes: u64) {
        self.process_rss_bytes_gauge.store(bytes, Ordering::Relaxed);
    }

    /// Explicitly set process CPU utilization percentage gauge.
    pub fn set_process_cpu_percent(&self, percent: f32) {
        self.process_cpu_percent_gauge.store(percent as f64, Ordering::Relaxed);
    }

    /// Record a general structured telemetry event.
    pub fn record_event(
        &self,
        level: &str,
        category: &str,
        message: &str,
        metadata: Option<serde_json::Value>,
    ) {
        let latency_ms = metadata
            .as_ref()
            .and_then(|m| m.get("latency_ms"))
            .and_then(|v| v.as_f64());

        let entry = TelemetryEntry {
            level: level.to_string(),
            time: Utc::now().to_rfc3339(),
            category: category.to_string(),
            message: message.to_string(),
            latency_ms,
            metadata,
        };

        if let Ok(mut guard) = self.events_ring.write() {
            guard.push(entry);
        }
    }

    // ── Export & Query Methods ───────────────────────────────────────────────

    /// Retrieve recent telemetry log entries formatted for the Dashboard UI.
    pub fn get_recent_events(&self, limit: Option<usize>) -> Vec<TelemetryEntry> {
        let guard = match self.events_ring.read() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };

        let items = guard.items();
        let limit = limit.unwrap_or(items.len());
        if items.len() <= limit {
            items
        } else {
            items[items.len() - limit..].to_vec()
        }
    }

    /// Compute percentile summary for a slice of records.
    fn compute_summary(records: &[LatencyRecord]) -> LatencyMetricsSummary {
        if records.is_empty() {
            return LatencyMetricsSummary::default();
        }

        let mut values: Vec<f64> = records.iter().map(|r| r.latency_ms).collect();
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let count = values.len();
        let min_ms = values.first().copied().unwrap_or(0.0);
        let max_ms = values.last().copied().unwrap_or(0.0);
        let sum: f64 = values.iter().sum();
        let avg_ms = sum / count as f64;

        let p50_idx = ((count as f64 * 0.50).ceil() as usize).saturating_sub(1).min(count - 1);
        let p95_idx = ((count as f64 * 0.95).ceil() as usize).saturating_sub(1).min(count - 1);

        let p50_ms = values[p50_idx];
        let p95_ms = values[p95_idx];
        let latest_ms = records.last().map(|r| r.latency_ms);

        LatencyMetricsSummary {
            count,
            p50_ms,
            p95_ms,
            min_ms,
            max_ms,
            avg_ms,
            latest_ms,
        }
    }

    /// Retrieve latency summary across all tracked pipelines.
    pub fn get_latency_summary(&self) -> serde_json::Value {
        let ttft_records = self.ttft_ring.read().map(|g| g.items()).unwrap_or_default();
        let receive_records = self.receive_stream_ring.read().map(|g| g.items()).unwrap_or_default();
        let ws_records = self.ws_transit_ring.read().map(|g| g.items()).unwrap_or_default();
        let audio_records = self.audio_latency_ring.read().map(|g| g.items()).unwrap_or_default();

        let ttft_summary = Self::compute_summary(&ttft_records);
        let receive_summary = Self::compute_summary(&receive_records);
        let ws_summary = Self::compute_summary(&ws_records);
        let audio_summary = Self::compute_summary(&audio_records);

        serde_json::json!({
            "ttft": ttft_summary,
            "receive_to_stream": receive_summary,
            "ws_transit": ws_summary,
            "audio": audio_summary,
        })
    }

    /// Retrieve the latest recorded TTFT in milliseconds, if any.
    pub fn latest_ttft_ms(&self) -> Option<f64> {
        self.ttft_ring.read().ok().and_then(|g| g.items().last().map(|r| r.latency_ms))
    }

    /// Retrieve the latest recorded audio latency in milliseconds, if any.
    pub fn latest_audio_latency_ms(&self) -> Option<f64> {
        self.audio_latency_ring.read().ok().and_then(|g| g.items().last().map(|r| r.latency_ms))
    }

    /// Retrieve a comprehensive telemetry snapshot for IPC inspection (`system:telemetry`).
    pub fn get_telemetry_snapshot(&self) -> serde_json::Value {
        let latency_summary = self.get_latency_summary();
        let recent_events = self.get_recent_events(Some(50));
        let recent_resources = self.resource_ring.read().map(|g| g.items()).unwrap_or_default();

        serde_json::json!({
            "timestamp": Utc::now().to_rfc3339(),
            "latencies": latency_summary,
            "recent_events": recent_events,
            "resource_history": recent_resources,
            "queue_depths": {
                "voice": self.queue_depth_voice.load(Ordering::Relaxed),
                "user": self.queue_depth_user.load(Ordering::Relaxed),
                "background": self.queue_depth_background.load(Ordering::Relaxed),
            },
            "preemptions_total": self.preemptions_total.load(Ordering::Relaxed),
            "tokens_generated_total": self.total_tokens_counter.load(Ordering::Relaxed),
            "tokens_per_second": self.tokens_per_second_gauge.load(Ordering::Relaxed),
        })
    }

    /// Export real-time metrics in compliant Prometheus text exposition format (version 0.0.4).
    pub fn export_prometheus_metrics(&self) -> String {
        let mut out = String::with_capacity(2048);

        // 1. TTFT Histogram
        self.ttft_histogram.export_prometheus(&mut out);
        out.push('\n');

        // 2. Token Velocity Gauge
        let _ = writeln!(
            out,
            "# HELP liva_tokens_per_second Active LLM generation token velocity in tokens per second."
        );
        let _ = writeln!(out, "# TYPE liva_tokens_per_second gauge");
        let _ = writeln!(
            out,
            "liva_tokens_per_second {:.2}",
            self.tokens_per_second_gauge.load(Ordering::Relaxed)
        );
        out.push('\n');

        // 3. Worker Queue Depths Gauge
        let _ = writeln!(
            out,
            "# HELP liva_worker_queue_depth Current pending request queue depth by priority tier."
        );
        let _ = writeln!(out, "# TYPE liva_worker_queue_depth gauge");
        let _ = writeln!(
            out,
            "liva_worker_queue_depth{{priority=\"voice\"}} {}",
            self.queue_depth_voice.load(Ordering::Relaxed)
        );
        let _ = writeln!(
            out,
            "liva_worker_queue_depth{{priority=\"user\"}} {}",
            self.queue_depth_user.load(Ordering::Relaxed)
        );
        let _ = writeln!(
            out,
            "liva_worker_queue_depth{{priority=\"background\"}} {}",
            self.queue_depth_background.load(Ordering::Relaxed)
        );
        out.push('\n');

        // 4. Preemption Counter
        let _ = writeln!(
            out,
            "# HELP liva_llm_preemptions_total Total number of LLM generation preemption events."
        );
        let _ = writeln!(out, "# TYPE liva_llm_preemptions_total counter");
        let _ = writeln!(
            out,
            "liva_llm_preemptions_total {}",
            self.preemptions_total.load(Ordering::Relaxed)
        );
        out.push('\n');

        // 5. Database Pool Wait Duration Histogram
        self.db_pool_wait_histogram_ms.export_prometheus(&mut out);
        out.push('\n');

        // 6. Database Query Execution Duration Histogram
        self.db_query_histogram_ms.export_prometheus(&mut out);
        out.push('\n');

        // 7. Process Memory RSS Bytes Gauge
        let _ = writeln!(
            out,
            "# HELP liva_process_rss_bytes Resident set size (RSS) memory usage of the process in bytes."
        );
        let _ = writeln!(out, "# TYPE liva_process_rss_bytes gauge");
        let _ = writeln!(
            out,
            "liva_process_rss_bytes {}",
            self.process_rss_bytes_gauge.load(Ordering::Relaxed)
        );
        out.push('\n');

        // 8. Process CPU Utilization Percent Gauge
        let _ = writeln!(
            out,
            "# HELP liva_process_cpu_percent Process CPU utilization percentage."
        );
        let _ = writeln!(out, "# TYPE liva_process_cpu_percent gauge");
        let _ = writeln!(
            out,
            "liva_process_cpu_percent {:.2}",
            self.process_cpu_percent_gauge.load(Ordering::Relaxed)
        );
        out.push('\n');

        // 9. Total Tokens Generated Counter
        let _ = writeln!(
            out,
            "# HELP liva_tokens_generated_total Total number of tokens generated by LLM engines."
        );
        let _ = writeln!(out, "# TYPE liva_tokens_generated_total counter");
        let _ = writeln!(
            out,
            "liva_tokens_generated_total {}",
            self.total_tokens_counter.load(Ordering::Relaxed)
        );

        out
    }

    /// Clear all stored telemetry records and reset metric registers.
    pub fn clear(&self) {
        if let Ok(mut g) = self.ttft_ring.write() { g.clear(); }
        if let Ok(mut g) = self.receive_stream_ring.write() { g.clear(); }
        if let Ok(mut g) = self.ws_transit_ring.write() { g.clear(); }
        if let Ok(mut g) = self.audio_latency_ring.write() { g.clear(); }
        if let Ok(mut g) = self.resource_ring.write() { g.clear(); }
        if let Ok(mut g) = self.events_ring.write() { g.clear(); }

        self.ttft_histogram.reset();
        self.tokens_per_second_gauge.store(0.0, Ordering::Relaxed);
        self.total_tokens_counter.store(0, Ordering::Relaxed);
        self.queue_depth_voice.store(0, Ordering::Relaxed);
        self.queue_depth_user.store(0, Ordering::Relaxed);
        self.queue_depth_background.store(0, Ordering::Relaxed);
        self.preemptions_total.store(0, Ordering::Relaxed);
        self.db_pool_wait_histogram_ms.reset();
        self.db_query_histogram_ms.reset();
        self.process_rss_bytes_gauge.store(0, Ordering::Relaxed);
        self.process_cpu_percent_gauge.store(0.0, Ordering::Relaxed);
    }
}

impl Default for TelemetryProfiler {
    fn default() -> Self {
        Self::new()
    }
}

static GLOBAL_TELEMETRY: OnceLock<TelemetryProfiler> = OnceLock::new();

/// Access the singleton global telemetry profiler.
pub fn global_telemetry() -> &'static TelemetryProfiler {
    GLOBAL_TELEMETRY.get_or_init(TelemetryProfiler::new)
}

// ============================================================================
// CONVENIENCE GLOBAL METRIC HELPERS
// ============================================================================

/// Record Time-To-First-Token latency duration on the global telemetry registry.
pub fn record_ttft(duration: Duration) {
    global_telemetry().record_ttft_duration(duration);
}

/// Record generated token count and update instantaneous velocity gauge on the global telemetry registry.
pub fn record_tokens_generated(count: u64, duration: Duration) {
    global_telemetry().record_tokens_generated(count, duration);
}

/// Record database read latency duration on the global telemetry registry.
pub fn record_db_read_latency(duration: Duration) {
    global_telemetry().record_db_read_latency(duration);
}

/// Record database write latency duration on the global telemetry registry.
pub fn record_db_write_latency(duration: Duration) {
    global_telemetry().record_db_write_latency(duration);
}

/// Record database connection acquisition wait latency on the global telemetry registry.
pub fn record_db_pool_wait(duration: Duration) {
    global_telemetry().record_db_pool_wait(duration);
}

/// Record a preemption event on the global telemetry registry.
pub fn record_preemption() {
    global_telemetry().record_preemption();
}

/// Set worker queue depth for a priority category on the global telemetry registry.
pub fn set_queue_depth(priority: &str, depth: usize) {
    global_telemetry().set_queue_depth(priority, depth);
}

/// Export real-time metrics in Prometheus text format (version 0.0.4) from the global telemetry registry.
pub fn export_prometheus_metrics() -> String {
    global_telemetry().export_prometheus_metrics()
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_buffer_capacity_eviction() {
        let mut ring = RingBuffer::with_capacity(3);
        assert!(ring.is_empty());
        assert_eq!(ring.len(), 0);

        ring.push(1);
        ring.push(2);
        ring.push(3);
        assert_eq!(ring.len(), 3);
        assert_eq!(ring.items(), vec![1, 2, 3]);

        ring.push(4);
        assert_eq!(ring.len(), 3);
        assert_eq!(ring.items(), vec![2, 3, 4]);

        ring.clear();
        assert!(ring.is_empty());
    }

    #[test]
    fn test_latency_summary_computation() {
        let profiler = TelemetryProfiler::new();
        profiler.record_ttft("test-model", 100.0, 32);
        profiler.record_ttft("test-model", 200.0, 64);
        profiler.record_ttft("test-model", 300.0, 128);

        let summary = profiler.get_latency_summary();
        let ttft = &summary["ttft"];
        assert_eq!(ttft["count"], 3);
        assert_eq!(ttft["min_ms"], 100.0);
        assert_eq!(ttft["max_ms"], 300.0);
        assert_eq!(ttft["avg_ms"], 200.0);
        assert_eq!(ttft["p50_ms"], 200.0);
        assert_eq!(ttft["latest_ms"], 300.0);
    }

    #[test]
    fn test_telemetry_snapshot_and_events() {
        let profiler = TelemetryProfiler::new();
        profiler.record_event("warn", "system", "High memory usage detected", None);
        profiler.record_receive_to_stream("ws", 12.5);
        profiler.record_ws_transit(0x01, 2.1, 1024);
        profiler.record_resource_sample(Some(45.0), Some(4.5), None, Some(1024 * 1024 * 100), None);

        let snapshot = profiler.get_telemetry_snapshot();
        assert!(snapshot.get("latencies").is_some());
        assert!(snapshot.get("recent_events").is_some());
        assert!(snapshot.get("resource_history").is_some());

        let events = profiler.get_recent_events(Some(10));
        assert!(!events.is_empty());
        assert_eq!(events.last().unwrap().message, "High memory usage detected");
    }

    // ── W3C TraceContext Tests ───────────────────────────────────────────────

    #[test]
    fn test_traceparent_new_and_roundtrip_formatting() {
        let ctx = TraceContext::new();
        assert_eq!(ctx.version, 0);
        assert!(ctx.is_sampled());
        assert!(!ctx.trace_id.iter().all(|&b| b == 0));
        assert!(!ctx.parent_id.iter().all(|&b| b == 0));

        let formatted = ctx.to_traceparent();
        assert_eq!(formatted.len(), 55);
        assert!(formatted.starts_with("00-"));
        assert!(formatted.ends_with("-01"));

        let parsed = TraceContext::from_traceparent(&formatted).expect("Must parse valid traceparent");
        assert_eq!(ctx, parsed);
        assert_eq!(parsed.trace_id_hex(), ctx.trace_id_hex());
        assert_eq!(parsed.parent_id_hex(), ctx.parent_id_hex());
    }

    #[test]
    fn test_traceparent_child_context_derivation() {
        let parent = TraceContext::new();
        let child = parent.child_context();

        assert_eq!(child.version, parent.version);
        assert_eq!(child.trace_id, parent.trace_id);
        assert_eq!(child.trace_flags, parent.trace_flags);
        assert_ne!(child.parent_id, parent.parent_id);
    }

    #[test]
    fn test_traceparent_validation_and_error_handling() {
        // Known valid W3C example
        let valid_str = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
        let ctx = TraceContext::from_traceparent(valid_str).unwrap();
        assert_eq!(ctx.version, 0);
        assert_eq!(ctx.trace_flags, 1);
        assert_eq!(ctx.to_traceparent(), valid_str);

        // Invalid format: missing hyphens
        assert_eq!(
            TraceContext::from_traceparent("004bf92f3577b34da6a3ce929d0e0e473600f067aa0ba902b701"),
            Err(TraceContextError::InvalidFormat)
        );

        // Version ff is forbidden by W3C spec
        assert_eq!(
            TraceContext::from_traceparent("ff-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"),
            Err(TraceContextError::InvalidVersion(0xff))
        );

        // All zeros trace_id is forbidden
        assert_eq!(
            TraceContext::from_traceparent("00-00000000000000000000000000000000-00f067aa0ba902b7-01"),
            Err(TraceContextError::AllZerosTraceId)
        );

        // All zeros parent_id is forbidden
        assert_eq!(
            TraceContext::from_traceparent("00-4bf92f3577b34da6a3ce929d0e0e4736-0000000000000000-01"),
            Err(TraceContextError::AllZerosParentId)
        );

        // Invalid hex characters
        assert!(TraceContext::from_traceparent("00-4bf92f3577b34da6a3ce929d0e0e47ZZ-00f067aa0ba902b7-01").is_err());
    }

    #[test]
    fn test_traceparent_display_and_from_str() {
        let ctx = TraceContext::new();
        let display_str = format!("{}", ctx);
        let parsed_ctx: TraceContext = display_str.parse().unwrap();
        assert_eq!(ctx, parsed_ctx);
    }

    // ── Prometheus Metric Instruments & String Export Tests ──────────────────

    #[test]
    fn test_prometheus_ttft_histogram_recording() {
        let profiler = TelemetryProfiler::new();

        // Record observations falling into various buckets:
        // 0.020s -> <=0.025
        // 0.040s -> <=0.050
        // 0.080s -> <=0.100
        // 0.200s -> <=0.250
        // 0.400s -> <=0.500
        // 0.800s -> <=1.0
        // 2.000s -> <=2.5
        // 3.000s -> >2.5 (+Inf only)
        profiler.record_ttft("test_m", 20.0, 10);
        profiler.record_ttft("test_m", 40.0, 10);
        profiler.record_ttft("test_m", 80.0, 10);
        profiler.record_ttft("test_m", 200.0, 10);
        profiler.record_ttft("test_m", 400.0, 10);
        profiler.record_ttft("test_m", 800.0, 10);
        profiler.record_ttft("test_m", 2000.0, 10);
        profiler.record_ttft("test_m", 3000.0, 10);

        let output = profiler.export_prometheus_metrics();
        assert!(output.contains("# HELP liva_ttft_seconds Time-to-first-token latency in seconds."));
        assert!(output.contains("# TYPE liva_ttft_seconds histogram"));
        assert!(output.contains("liva_ttft_seconds_bucket{le=\"0.025\"} 1"));
        assert!(output.contains("liva_ttft_seconds_bucket{le=\"0.05\"} 2"));
        assert!(output.contains("liva_ttft_seconds_bucket{le=\"0.1\"} 3"));
        assert!(output.contains("liva_ttft_seconds_bucket{le=\"0.25\"} 4"));
        assert!(output.contains("liva_ttft_seconds_bucket{le=\"0.5\"} 5"));
        assert!(output.contains("liva_ttft_seconds_bucket{le=\"1\"} 6"));
        assert!(output.contains("liva_ttft_seconds_bucket{le=\"2.5\"} 7"));
        assert!(output.contains("liva_ttft_seconds_bucket{le=\"+Inf\"} 8"));
        assert!(output.contains("liva_ttft_seconds_count 8"));
    }

    #[test]
    fn test_prometheus_token_velocity_and_total_counters() {
        let profiler = TelemetryProfiler::new();

        profiler.record_tokens_generated(100, Duration::from_secs(2)); // 50.0 tok/s
        assert_eq!(profiler.total_tokens_counter.load(Ordering::Relaxed), 100);

        let output = profiler.export_prometheus_metrics();
        assert!(output.contains("# HELP liva_tokens_per_second"));
        assert!(output.contains("# TYPE liva_tokens_per_second gauge"));
        assert!(output.contains("liva_tokens_per_second 50.00"));

        assert!(output.contains("# HELP liva_tokens_generated_total"));
        assert!(output.contains("# TYPE liva_tokens_generated_total counter"));
        assert!(output.contains("liva_tokens_generated_total 100"));
    }

    #[test]
    fn test_prometheus_worker_queue_depth_gauge() {
        let profiler = TelemetryProfiler::new();

        profiler.set_queue_depth("voice", 2);
        profiler.set_queue_depth("user", 5);
        profiler.set_queue_depth("background", 12);

        assert_eq!(profiler.get_queue_depth("voice"), 2);
        assert_eq!(profiler.get_queue_depth("user"), 5);
        assert_eq!(profiler.get_queue_depth("background"), 12);

        profiler.inc_queue_depth("voice");
        profiler.dec_queue_depth("user");

        let output = profiler.export_prometheus_metrics();
        assert!(output.contains("# HELP liva_worker_queue_depth"));
        assert!(output.contains("# TYPE liva_worker_queue_depth gauge"));
        assert!(output.contains("liva_worker_queue_depth{priority=\"voice\"} 3"));
        assert!(output.contains("liva_worker_queue_depth{priority=\"user\"} 4"));
        assert!(output.contains("liva_worker_queue_depth{priority=\"background\"} 12"));
    }

    #[test]
    fn test_prometheus_preemption_counter() {
        let profiler = TelemetryProfiler::new();

        profiler.record_preemption();
        profiler.record_preemption();
        profiler.record_preemptions(3);

        assert_eq!(profiler.get_preemptions(), 5);

        let output = profiler.export_prometheus_metrics();
        assert!(output.contains("# HELP liva_llm_preemptions_total"));
        assert!(output.contains("# TYPE liva_llm_preemptions_total counter"));
        assert!(output.contains("liva_llm_preemptions_total 5"));
    }

    #[test]
    fn test_prometheus_db_pool_and_query_histograms() {
        let profiler = TelemetryProfiler::new();

        profiler.record_db_pool_wait(Duration::from_micros(400)); // 0.4ms -> bucket 0.5
        profiler.record_db_pool_wait(Duration::from_millis(2));   // 2.0ms -> bucket 2.5
        profiler.record_db_read_latency(Duration::from_millis(1));
        profiler.record_db_write_latency(Duration::from_millis(8));

        let output = profiler.export_prometheus_metrics();
        assert!(output.contains("# HELP liva_db_pool_wait_duration_ms"));
        assert!(output.contains("# TYPE liva_db_pool_wait_duration_ms histogram"));
        assert!(output.contains("liva_db_pool_wait_duration_ms_count 2"));

        assert!(output.contains("# HELP liva_db_query_duration_ms"));
        assert!(output.contains("# TYPE liva_db_query_duration_ms histogram"));
        assert!(output.contains("liva_db_query_duration_ms_count 2"));
    }

    #[test]
    fn test_prometheus_process_rss_and_cpu_gauges() {
        let profiler = TelemetryProfiler::new();

        profiler.set_process_rss_bytes(52_428_800); // 50MB
        profiler.set_process_cpu_percent(14.25);

        let output = profiler.export_prometheus_metrics();
        assert!(output.contains("# HELP liva_process_rss_bytes"));
        assert!(output.contains("# TYPE liva_process_rss_bytes gauge"));
        assert!(output.contains("liva_process_rss_bytes 52428800"));

        assert!(output.contains("# HELP liva_process_cpu_percent"));
        assert!(output.contains("# TYPE liva_process_cpu_percent gauge"));
        assert!(output.contains("liva_process_cpu_percent 14.25"));
    }

    #[test]
    fn test_global_convenience_functions() {
        let g = global_telemetry();
        g.clear();

        record_ttft(Duration::from_millis(45));
        record_tokens_generated(60, Duration::from_secs(1));
        record_db_read_latency(Duration::from_millis(2));
        record_db_write_latency(Duration::from_millis(5));
        record_db_pool_wait(Duration::from_micros(300));
        record_preemption();
        set_queue_depth("voice", 1);

        let output = export_prometheus_metrics();
        assert!(output.contains("liva_ttft_seconds_count 1"));
        assert!(output.contains("liva_tokens_generated_total 60"));
        assert!(output.contains("liva_tokens_per_second 60.00"));
        assert!(output.contains("liva_llm_preemptions_total 1"));
        assert!(output.contains("liva_worker_queue_depth{priority=\"voice\"} 1"));
        assert!(output.contains("liva_db_query_duration_ms_count 2"));
        assert!(output.contains("liva_db_pool_wait_duration_ms_count 1"));
    }

    #[test]
    fn test_concurrent_metric_updates_thread_safety() {
        use std::sync::Arc;
        let profiler = Arc::new(TelemetryProfiler::new());
        let mut handles = Vec::new();

        for i in 0..16 {
            let p = profiler.clone();
            handles.push(std::thread::spawn(move || {
                for j in 0..100 {
                    p.record_ttft("concurrent_test", (i + j) as f64 * 5.0, 10);
                    p.record_tokens_generated(2, Duration::from_millis(50));
                    p.record_db_pool_wait(Duration::from_micros(100));
                    p.record_db_query(Duration::from_millis(1));
                    p.record_preemption();
                    p.set_queue_depth("user", (i + j) % 10);
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(profiler.ttft_histogram.count(), 1600);
        assert_eq!(profiler.total_tokens_counter.load(Ordering::Relaxed), 3200);
        assert_eq!(profiler.preemptions_total.load(Ordering::Relaxed), 1600);
        assert_eq!(profiler.db_pool_wait_histogram_ms.count(), 1600);
        assert_eq!(profiler.db_query_histogram_ms.count(), 1600);

        let metrics_str = profiler.export_prometheus_metrics();
        assert!(!metrics_str.is_empty());
    }
}
