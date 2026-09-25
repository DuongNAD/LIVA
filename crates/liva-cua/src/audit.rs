//! Structured Audit Ledger, In-Memory Ring Buffer, SQLite WAL Persistence,
//! and Streaming JSONL Log with Daily Rotation for LIVA CUA.
//!
//! Provides zero-contention audit event logging (< 1 microsecond record latency),
//! 256-slot circular replay buffer, micro-batched persistence to SQLite `cua_audit_ledger`,
//! date-partitioned JSONL streaming, and rich indexed query API for the Tauri dashboard.

use crate::types::{CuaActionEffect, CuaDeliveryMode, CuaRect, PermissionMode};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info, warn};

// ============================================================================
// 1. DATA MODELS & SCHEMAS
// ============================================================================

/// Security verdict reached prior to or during action execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecurityVerdict {
    /// Action cleared all security and state checks and was permitted to execute.
    Allowed,
    /// Action was refused by security policy (e.g. Standard mode read-only,
    /// target not in allowlist, window on protected denylist, dangerous hotkey).
    BlockedByPolicy,
    /// Action was actively aborted by the emergency kill-switch (< 15ms SLA).
    AbortedByKillSwitch,
    /// Action failed pre-execution state checks (window minimized, hidden,
    /// PID mismatch, UIPI integrity barrier).
    FailedPreCheck,
}

impl std::fmt::Display for SecurityVerdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Allowed => write!(f, "allowed"),
            Self::BlockedByPolicy => write!(f, "blocked_by_policy"),
            Self::AbortedByKillSwitch => write!(f, "aborted_by_kill_switch"),
            Self::FailedPreCheck => write!(f, "failed_pre_check"),
        }
    }
}

impl std::str::FromStr for SecurityVerdict {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "allowed" => Ok(Self::Allowed),
            "blocked_by_policy" => Ok(Self::BlockedByPolicy),
            "aborted_by_kill_switch" => Ok(Self::AbortedByKillSwitch),
            "failed_pre_check" => Ok(Self::FailedPreCheck),
            other => Err(format!("Unknown security verdict: {}", other)),
        }
    }
}

/// Target application and window identity metadata.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TargetAppInfo {
    /// Base executable name of the target process (e.g. "notepad.exe", "msedge.exe").
    pub process_name: Option<String>,
    /// Owning OS Process Identifier (PID).
    pub process_id: Option<u32>,
    /// Target window title caption.
    pub window_title: Option<String>,
    /// Win32 HWND handle represented as unsigned 64-bit integer.
    pub hwnd: u64,
}

/// Action coordinate and bounding geometry context.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ActionCoordinates {
    /// Virtual desktop screen X coordinate in physical pixels.
    pub screen_x: Option<i32>,
    /// Virtual desktop screen Y coordinate in physical pixels.
    pub screen_y: Option<i32>,
    /// Window client area relative X coordinate in pixels.
    pub client_x: Option<i32>,
    /// Window client area relative Y coordinate in pixels.
    pub client_y: Option<i32>,
    /// Target UI element or window bounding box.
    pub bounds: Option<CuaRect>,
}

/// Comprehensive, tamper-evident audit record for every attempted CUA action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CuaAuditEvent {
    /// Globally unique event identifier (UUIDv4 or monotonic unique string).
    pub event_id: String,
    /// Event occurrence timestamp in milliseconds since Unix epoch (UTC).
    pub timestamp_unix_ms: u64,
    /// Identifier of the calling session, agent workflow, or conversation.
    pub session_id: String,
    /// Verbose action type name (e.g. "click", "double_click", "scroll", "type_text", "hotkey").
    pub action_type: String,
    /// Target process, window, and HWND details.
    pub target_app: TargetAppInfo,
    /// Action screen and client coordinates.
    pub coordinates: ActionCoordinates,
    /// Active security permission mode at execution time.
    pub permission_mode: PermissionMode,
    /// Security governor verdict.
    pub security_verdict: SecurityVerdict,
    /// Policy rejection or failure rationale if blocked or refused.
    pub policy_violation: Option<String>,
    /// Input delivery mode used ("background", "foreground").
    pub execution_delivery: CuaDeliveryMode,
    /// Outcome classification ("completed", "refused", "aborted", "failed").
    pub action_effect: CuaActionEffect,
    /// Total action dispatch latency in milliseconds.
    pub latency_ms: u64,
    /// Machine-readable error code if failed or refused.
    pub error_code: Option<String>,
}

// ============================================================================
// 2. IN-MEMORY RING BUFFER (CAPACITY 256, ZERO CONTENTION)
// ============================================================================

/// Fixed ring buffer capacity (strictly a power of 2 for fast bitwise indexing).
pub const RING_BUFFER_CAPACITY: usize = 256;
const RING_BUFFER_MASK: usize = RING_BUFFER_CAPACITY - 1;

/// Zero-lock-contention in-memory circular replay buffer.
///
/// Uses an atomic monotonic head counter and striped slot locks. Writes complete in
/// < 50 nanoseconds without taking any global mutex, ensuring desktop input synthesis
/// is never perturbed by audit buffering.
pub struct AuditRingBuffer {
    head: AtomicU64,
    slots: Box<[RwLock<Option<Arc<CuaAuditEvent>>>; RING_BUFFER_CAPACITY]>,
}

impl Default for AuditRingBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl AuditRingBuffer {
    /// Allocates an empty 256-slot circular ring buffer.
    pub fn new() -> Self {
        let slots = Box::new(std::array::from_fn(|_| RwLock::new(None)));
        Self {
            head: AtomicU64::new(0),
            slots,
        }
    }

    /// Pushes an event into the ring buffer with zero global lock contention.
    /// Overwrites the slot at `(seq & 255)`.
    pub fn push(&self, event: CuaAuditEvent) -> Arc<CuaAuditEvent> {
        let arc_event = Arc::new(event);
        let seq = self.head.fetch_add(1, Ordering::Relaxed);
        let slot_idx = (seq as usize) & RING_BUFFER_MASK;

        if let Ok(mut guard) = self.slots[slot_idx].write() {
            *guard = Some(Arc::clone(&arc_event));
        }

        arc_event
    }

    /// Reads up to `limit` most recent events in reverse chronological order (newest first).
    pub fn read_recent(&self, limit: usize) -> Vec<Arc<CuaAuditEvent>> {
        let current_head = self.head.load(Ordering::Relaxed);
        if current_head == 0 {
            return Vec::new();
        }

        let count = limit.min(RING_BUFFER_CAPACITY).min(current_head as usize);
        let mut results = Vec::with_capacity(count);

        for i in 0..count {
            let seq = current_head.saturating_sub(1 + i as u64);
            let slot_idx = (seq as usize) & RING_BUFFER_MASK;

            if let Ok(guard) = self.slots[slot_idx].read() {
                if let Some(ref ev) = *guard {
                    results.push(Arc::clone(ev));
                }
            }
        }

        results
    }

    /// Returns the total cumulative number of events pushed since initialization.
    pub fn total_pushed(&self) -> u64 {
        self.head.load(Ordering::Relaxed)
    }

    /// Returns the current number of valid events stored in memory (up to 256).
    pub fn len(&self) -> usize {
        let head = self.head.load(Ordering::Relaxed);
        (head as usize).min(RING_BUFFER_CAPACITY)
    }

    /// True if the ring buffer contains no events.
    pub fn is_empty(&self) -> bool {
        self.head.load(Ordering::Relaxed) == 0
    }
}

// ============================================================================
// 3. SQLITE WAL PERSISTENCE & MIGRATIONS
// ============================================================================

/// SQLite storage schema and indexed query operations for `cua_audit_ledger`.
pub struct CuaAuditStore;

impl CuaAuditStore {
    /// Creates the `cua_audit_ledger` table and performance indexes if they do not exist.
    pub fn init_tables(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS cua_audit_ledger (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                event_id TEXT UNIQUE NOT NULL,
                timestamp_unix_ms INTEGER NOT NULL,
                session_id TEXT NOT NULL,
                action_type TEXT NOT NULL,
                process_name TEXT,
                process_id INTEGER,
                window_title TEXT,
                hwnd INTEGER NOT NULL,
                screen_x INTEGER,
                screen_y INTEGER,
                client_x INTEGER,
                client_y INTEGER,
                bounds_x INTEGER,
                bounds_y INTEGER,
                bounds_w INTEGER,
                bounds_h INTEGER,
                permission_mode TEXT NOT NULL,
                security_verdict TEXT NOT NULL,
                policy_violation TEXT,
                execution_delivery TEXT NOT NULL,
                action_effect TEXT NOT NULL,
                latency_ms INTEGER NOT NULL,
                error_code TEXT,
                raw_json TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_cua_audit_ts 
                ON cua_audit_ledger(timestamp_unix_ms DESC);

            CREATE INDEX IF NOT EXISTS idx_cua_audit_sess_ts 
                ON cua_audit_ledger(session_id, timestamp_unix_ms DESC);

            CREATE INDEX IF NOT EXISTS idx_cua_audit_verdict 
                ON cua_audit_ledger(security_verdict, timestamp_unix_ms DESC);

            CREATE INDEX IF NOT EXISTS idx_cua_audit_process 
                ON cua_audit_ledger(process_name, timestamp_unix_ms DESC);

            CREATE INDEX IF NOT EXISTS idx_cua_audit_action 
                ON cua_audit_ledger(action_type, timestamp_unix_ms DESC);
            "#,
        )
    }

    /// Flushes a micro-batch of events to SQLite WAL inside a single transaction.
    pub fn insert_batch(
        conn: &rusqlite::Connection,
        events: &[CuaAuditEvent],
    ) -> rusqlite::Result<()> {
        if events.is_empty() {
            return Ok(());
        }

        let mut stmt = conn.prepare_cached(
            r#"
            INSERT OR IGNORE INTO cua_audit_ledger (
                event_id, timestamp_unix_ms, session_id, action_type,
                process_name, process_id, window_title, hwnd,
                screen_x, screen_y, client_x, client_y,
                bounds_x, bounds_y, bounds_w, bounds_h,
                permission_mode, security_verdict, policy_violation,
                execution_delivery, action_effect, latency_ms, error_code, raw_json
            ) VALUES (
                ?1, ?2, ?3, ?4,
                ?5, ?6, ?7, ?8,
                ?9, ?10, ?11, ?12,
                ?13, ?14, ?15, ?16,
                ?17, ?18, ?19,
                ?20, ?21, ?22, ?23, ?24
            )
            "#,
        )?;

        for ev in events {
            let (bx, by, bw, bh) = match ev.coordinates.bounds {
                Some(r) => (Some(r.x), Some(r.y), Some(r.width), Some(r.height)),
                None => (None, None, None, None),
            };

            let perm_str = match ev.permission_mode {
                PermissionMode::Standard => "standard",
                PermissionMode::Bounded => "bounded",
                PermissionMode::Unrestricted => "unrestricted",
            };

            let delivery_str = match ev.execution_delivery {
                CuaDeliveryMode::Background => "background",
                CuaDeliveryMode::Foreground => "foreground",
            };

            let effect_str = match ev.action_effect {
                CuaActionEffect::Completed => "completed",
                CuaActionEffect::Refused => "refused",
                CuaActionEffect::Aborted => "aborted",
                CuaActionEffect::Failed => "failed",
            };

            let raw_json = serde_json::to_string(ev).unwrap_or_default();

            stmt.execute(rusqlite::params![
                ev.event_id,
                ev.timestamp_unix_ms as i64,
                ev.session_id,
                ev.action_type,
                ev.target_app.process_name,
                ev.target_app.process_id,
                ev.target_app.window_title,
                ev.target_app.hwnd as i64,
                ev.coordinates.screen_x,
                ev.coordinates.screen_y,
                ev.coordinates.client_x,
                ev.coordinates.client_y,
                bx,
                by,
                bw,
                bh,
                perm_str,
                ev.security_verdict.to_string(),
                ev.policy_violation,
                delivery_str,
                effect_str,
                ev.latency_ms as i64,
                ev.error_code,
                raw_json,
            ])?;
        }

        Ok(())
    }

    /// Queries the SQLite audit ledger with dynamic filtering and pagination.
    pub fn query(
        conn: &rusqlite::Connection,
        filter: &AuditQueryFilter,
    ) -> rusqlite::Result<AuditQueryResult> {
        let mut conditions = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(session_id) = &filter.session_id {
            conditions.push(format!("session_id = ?{}", params.len() + 1));
            params.push(Box::new(session_id.clone()));
        }

        if let Some(action_type) = &filter.action_type {
            conditions.push(format!("action_type = ?{}", params.len() + 1));
            params.push(Box::new(action_type.clone()));
        }

        if let Some(verdict) = &filter.security_verdict {
            conditions.push(format!("security_verdict = ?{}", params.len() + 1));
            params.push(Box::new(verdict.to_string()));
        }

        if let Some(process_name) = &filter.process_name {
            conditions.push(format!("process_name = ?{}", params.len() + 1));
            params.push(Box::new(process_name.clone()));
        }

        if let Some(start_ts) = filter.start_timestamp_ms {
            conditions.push(format!("timestamp_unix_ms >= ?{}", params.len() + 1));
            params.push(Box::new(start_ts as i64));
        }

        if let Some(end_ts) = filter.end_timestamp_ms {
            conditions.push(format!("timestamp_unix_ms <= ?{}", params.len() + 1));
            params.push(Box::new(end_ts as i64));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        // 1. Total matched count
        let count_sql = format!("SELECT COUNT(*) FROM cua_audit_ledger {}", where_clause);
        let mut count_stmt = conn.prepare(&count_sql)?;
        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let total_matched: usize = count_stmt.query_row(param_refs.as_slice(), |row| row.get(0))?;

        // 2. Paginated rows
        let limit = filter.limit.unwrap_or(50).min(1000);
        let offset = filter.offset.unwrap_or(0);
        let query_sql = format!(
            "SELECT raw_json FROM cua_audit_ledger {} ORDER BY timestamp_unix_ms DESC LIMIT ?{} OFFSET ?{}",
            where_clause,
            params.len() + 1,
            params.len() + 2,
        );

        let mut query_stmt = conn.prepare(&query_sql)?;
        let mut all_params = params;
        all_params.push(Box::new(limit as i64));
        all_params.push(Box::new(offset as i64));
        let all_param_refs: Vec<&dyn rusqlite::ToSql> =
            all_params.iter().map(|p| p.as_ref()).collect();

        let rows = query_stmt.query_map(all_param_refs.as_slice(), |row| {
            let json_str: String = row.get(0)?;
            Ok(json_str)
        })?;

        let mut events = Vec::new();
        for r in rows {
            let json_str = r?;
            if let Ok(ev) = serde_json::from_str::<CuaAuditEvent>(&json_str) {
                events.push(ev);
            }
        }

        Ok(AuditQueryResult {
            events,
            total_matched,
            limit,
            offset,
        })
    }
}

// ============================================================================
// 4. STREAMING JSONL LOGGER WITH DAILY ROTATION
// ============================================================================

/// Probes an existing log file to determine its date from the first event record or file modification time.
fn detect_file_date(path: &Path) -> Option<String> {
    if !path.exists() {
        return None;
    }

    let metadata = std::fs::metadata(path).ok()?;
    if metadata.len() == 0 {
        return None;
    }

    // 1. Primary: probe the timestamp of the first JSONL record in the file
    if let Ok(file) = File::open(path) {
        use std::io::{BufRead, BufReader};
        let reader = BufReader::new(file);
        if let Some(Ok(first_line)) = reader.lines().next() {
            #[derive(Deserialize)]
            struct TimestampProbe {
                timestamp_unix_ms: u64,
            }
            if let Ok(probe) = serde_json::from_str::<TimestampProbe>(&first_line) {
                return Some(unix_ms_to_date_string(probe.timestamp_unix_ms));
            }
        }
    }

    // 2. Secondary fallback: use file system modification time
    if let Ok(modified) = metadata.modified() {
        if let Ok(duration) = modified.duration_since(UNIX_EPOCH) {
            let mtime_ms = duration.as_millis() as u64;
            return Some(unix_ms_to_date_string(mtime_ms));
        }
    }

    None
}

/// Manages append-only JSONL event writing with automatic midnight date rotation.
pub struct StreamingJsonlLogger {
    log_dir: PathBuf,
    active_filename: String,
    current_date_stamp: Option<String>,
    writer: Option<BufWriter<File>>,
    enabled: bool,
}

impl StreamingJsonlLogger {
    /// Initializes a new JSONL logger pointing to `log_dir/active_filename`.
    pub fn new(
        log_dir: impl AsRef<Path>,
        active_filename: impl Into<String>,
        enabled: bool,
    ) -> Self {
        Self {
            log_dir: log_dir.as_ref().to_path_buf(),
            active_filename: active_filename.into(),
            current_date_stamp: None,
            writer: None,
            enabled,
        }
    }

    /// Appends a batch of audit events to the active JSONL file, rotating if date rolls over.
    pub fn write_batch(&mut self, events: &[CuaAuditEvent]) -> std::io::Result<()> {
        if !self.enabled || events.is_empty() {
            return Ok(());
        }

        let first_ts = events[0].timestamp_unix_ms;
        let event_date = unix_ms_to_date_string(first_ts);

        self.ensure_active_file(&event_date)?;

        if let Some(ref mut writer) = self.writer {
            for ev in events {
                let serialized = serde_json::to_string(ev)?;
                writer.write_all(serialized.as_bytes())?;
                writer.write_all(b"\n")?;
            }
            writer.flush()?;
        }

        Ok(())
    }

    fn ensure_active_file(&mut self, target_date: &str) -> std::io::Result<()> {
        let needs_rotation = match &self.current_date_stamp {
            Some(active_date) => active_date != target_date,
            None => true,
        };

        if needs_rotation {
            // Close existing writer
            if let Some(mut w) = self.writer.take() {
                let _ = w.flush();
            }

            std::fs::create_dir_all(&self.log_dir)?;
            let active_path = self.log_dir.join(&self.active_filename);

            // Determine previous date stamp:
            // - If running continuously across midnight, take from in-memory `current_date_stamp`
            // - If starting up / restarting with `current_date_stamp: None`, probe existing file on disk
            let prev_date = self
                .current_date_stamp
                .take()
                .or_else(|| detect_file_date(&active_path));

            if let Some(prev) = prev_date {
                if active_path.exists() && prev != target_date {
                    let rotated_filename = format!("cua_audit_{}.jsonl", prev);
                    let rotated_path = self.log_dir.join(&rotated_filename);
                    if !rotated_path.exists() {
                        let _ = std::fs::rename(&active_path, &rotated_path);
                    } else {
                        let collision_filename =
                            format!("cua_audit_{}_{}.jsonl", prev, current_unix_ms());
                        let _ =
                            std::fs::rename(&active_path, self.log_dir.join(collision_filename));
                    }
                }
            }

            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&active_path)?;

            self.writer = Some(BufWriter::new(file));
            self.current_date_stamp = Some(target_date.to_string());
        }

        Ok(())
    }
}

/// Converts Unix millisecond timestamp to "YYYY-MM-DD" UTC string without external crates.
/// Uses Howard Hinnant's public domain civil calendar conversion algorithm.
pub fn unix_ms_to_date_string(timestamp_ms: u64) -> String {
    let secs = (timestamp_ms / 1000) as i64;
    let mut days = secs / 86400;

    days += 719468;
    let era = (if days >= 0 { days } else { days - 146096 }) / 146097;
    let doe = (days - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{:04}-{:02}-{:02}", y, m, d)
}

// ============================================================================
// 5. QUERY & FILTER API FOR TAURI DASHBOARD
// ============================================================================

/// Query parameters passed from the Tauri dashboard to inspect the audit ledger.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuditQueryFilter {
    /// Optional filter by session / conversation ID.
    pub session_id: Option<String>,
    /// Optional filter by action type name.
    pub action_type: Option<String>,
    /// Optional filter by security verdict.
    pub security_verdict: Option<SecurityVerdict>,
    /// Optional filter by target process image name (e.g. "notepad.exe").
    pub process_name: Option<String>,
    /// Minimum timestamp (inclusive) in Unix milliseconds.
    pub start_timestamp_ms: Option<u64>,
    /// Maximum timestamp (inclusive) in Unix milliseconds.
    pub end_timestamp_ms: Option<u64>,
    /// Maximum items to return (default 50, capped at 1000).
    pub limit: Option<usize>,
    /// Pagination offset.
    pub offset: Option<usize>,
}

/// Paginated audit query response returned to Tauri UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditQueryResult {
    pub events: Vec<CuaAuditEvent>,
    pub total_matched: usize,
    pub limit: usize,
    pub offset: usize,
}

// ============================================================================
// 6. RECORDER FACADE & BACKGROUND PERSISTENCE PIPELINE
// ============================================================================

/// Configuration options for the CUA Audit Subsystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CuaAuditConfig {
    /// Whether audit recording is active (default true).
    pub enabled: bool,
    /// Path to directory holding streaming JSONL logs (default "logs").
    pub log_dir: PathBuf,
    /// Active JSONL filename (default "cua_audit.jsonl").
    pub active_jsonl_filename: String,
    /// Maximum batch size before flushing to disk and SQLite (default 64).
    pub micro_batch_size: usize,
    /// Maximum duration to wait before flushing incomplete batch (default 100ms).
    pub micro_batch_timeout_ms: u64,
    /// Internal queue buffer capacity (default 256).
    pub mpsc_buffer_capacity: usize,
}

impl Default for CuaAuditConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            log_dir: PathBuf::from("logs"),
            active_jsonl_filename: "cua_audit.jsonl".to_string(),
            micro_batch_size: 64,
            micro_batch_timeout_ms: 100,
            mpsc_buffer_capacity: 256,
        }
    }
}

/// Runtime operational metrics for the audit subsystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CuaAuditStats {
    pub total_recorded: u64,
    pub ring_buffer_items: usize,
    pub dropped_events: usize,
    pub is_enabled: bool,
}

/// Unified, non-blocking facade for CUA audit logging.
///
/// Dispatches audit events into the 256-slot zero-contention ring buffer
/// and queues them asynchronously for micro-batched SQLite and JSONL persistence.
pub struct CuaAuditRecorder {
    config: CuaAuditConfig,
    ring_buffer: Arc<AuditRingBuffer>,
    tx: mpsc::Sender<CuaAuditEvent>,
    dropped_counter: AtomicUsize,
    total_recorded: AtomicU64,
}

impl CuaAuditRecorder {
    /// Initializes the audit recorder and starts the background persistence worker.
    pub fn new(
        config: CuaAuditConfig,
        db_handle: Option<liva_storage::DbActorHandle>,
    ) -> Arc<Self> {
        let ring_buffer = Arc::new(AuditRingBuffer::new());
        let (tx, rx) = mpsc::channel(config.mpsc_buffer_capacity);

        let recorder = Arc::new(Self {
            config: config.clone(),
            ring_buffer: Arc::clone(&ring_buffer),
            tx,
            dropped_counter: AtomicUsize::new(0),
            total_recorded: AtomicU64::new(0),
        });

        // Spawn background persistence actor if in tokio context; fallback to dedicated thread if called from non-async test
        let worker_config = config;
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                run_audit_persistence_worker(rx, worker_config, db_handle).await;
            });
        } else {
            std::thread::spawn(move || {
                if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    rt.block_on(run_audit_persistence_worker(rx, worker_config, db_handle));
                }
            });
        }

        recorder
    }

    /// Records an audit event with ZERO lock contention (< 1 microsecond).
    ///
    /// Never blocks the caller thread or execution loop.
    pub fn record(&self, event: CuaAuditEvent) {
        if !self.config.enabled {
            return;
        }

        self.total_recorded.fetch_add(1, Ordering::Relaxed);

        // 1. Immediately store into 256-slot ring buffer (< 50ns)
        self.ring_buffer.push(event.clone());

        // 2. Non-blocking queue send to background persistence worker
        if let Err(_err) = self.tx.try_send(event) {
            // Channel full or saturated: increment dropped persist counter to protect latency SLA
            self.dropped_counter.fetch_add(1, Ordering::Relaxed);
            warn!(
                "CUA audit persistence channel saturated: event retained in RAM ring buffer only"
            );
        }
    }

    /// Instantly returns the most recent `limit` events from memory in microseconds.
    pub fn get_recent(&self, limit: usize) -> Vec<Arc<CuaAuditEvent>> {
        self.ring_buffer.read_recent(limit)
    }

    /// Queries runtime audit metrics.
    pub fn stats(&self) -> CuaAuditStats {
        CuaAuditStats {
            total_recorded: self.total_recorded.load(Ordering::Relaxed),
            ring_buffer_items: self.ring_buffer.len(),
            dropped_events: self.dropped_counter.load(Ordering::Relaxed),
            is_enabled: self.config.enabled,
        }
    }

    /// Returns a direct reference to the underlying ring buffer.
    pub fn ring_buffer(&self) -> &Arc<AuditRingBuffer> {
        &self.ring_buffer
    }
}

/// Helper function to capture current epoch milliseconds.
pub fn current_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as u64
}

// ============================================================================
// 7. BACKGROUND PERSISTENCE WORKER LOOP
// ============================================================================

async fn run_audit_persistence_worker(
    mut rx: mpsc::Receiver<CuaAuditEvent>,
    config: CuaAuditConfig,
    db_handle: Option<liva_storage::DbActorHandle>,
) {
    // 0. Auto-initialize SQLite schema before processing incoming batches
    if let Some(ref handle) = db_handle {
        let (tx, rx_init) = oneshot::channel();
        let init_op: liva_storage::DbWriteOp = Box::new(|conn: &rusqlite::Connection| {
            CuaAuditStore::init_tables(conn).map_err(|e| e.to_string())
        });

        if let Err(e) = handle
            .send(liva_storage::DbWriteCommand::Execute {
                op: init_op,
                result_tx: Some(tx),
            })
            .await
        {
            error!(
                "Failed to dispatch CuaAuditStore::init_tables to DbActor: {}",
                e
            );
        } else {
            match rx_init.await {
                Ok(Ok(())) => {
                    info!("CuaAuditStore::init_tables initialized successfully via DbActor")
                }
                Ok(Err(e)) => error!("CuaAuditStore::init_tables failed on database: {}", e),
                Err(_) => warn!("CuaAuditStore::init_tables response dropped by DbActor"),
            }
        }
    }

    let mut jsonl_logger = StreamingJsonlLogger::new(
        &config.log_dir,
        &config.active_jsonl_filename,
        config.enabled,
    );

    let batch_size = config.micro_batch_size;
    let batch_wait = Duration::from_millis(config.micro_batch_timeout_ms);

    while let Some(first_ev) = rx.recv().await {
        let mut batch = Vec::with_capacity(batch_size);
        batch.push(first_ev);

        let start = Instant::now();
        while batch.len() < batch_size {
            let elapsed = start.elapsed();
            if elapsed >= batch_wait {
                break;
            }
            match tokio::time::timeout(batch_wait - elapsed, rx.recv()).await {
                Ok(Some(ev)) => batch.push(ev),
                Ok(None) => break, // Channel closed
                Err(_) => break,   // Timeout reached
            }
        }

        // 1. Write batch to daily rotated JSONL
        if let Err(e) = jsonl_logger.write_batch(&batch) {
            error!("Failed to write CUA audit batch to JSONL: {}", e);
        }

        // 2. Persist batch to SQLite WAL via DbActor
        if let Some(ref handle) = db_handle {
            let events_to_persist = batch;
            let op: liva_storage::DbWriteOp = Box::new(move |conn: &rusqlite::Connection| {
                CuaAuditStore::insert_batch(conn, &events_to_persist).map_err(|e| e.to_string())
            });

            if let Err(e) = handle
                .send(liva_storage::DbWriteCommand::Execute {
                    op,
                    result_tx: None,
                })
                .await
            {
                error!("Failed to dispatch CUA audit batch to DbActor: {}", e);
            }
        }
    }

    info!("CUA audit persistence worker terminated cleanly.");
}
