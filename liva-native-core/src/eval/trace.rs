//! Execution Trace Logger & Tamper-Evident Hash-Chained JSONL Audit Log (Feature F03).
//!
//! Provides:
//! - Complete schema for execution trace: `harness_version`, `session_id`, `turn_index`,
//!   `timestamp_utc`, `principal`, `channel`, `session_mode`, `metrics`, `trajectory`,
//!   `telemetry`, and `integrity`.
//! - Cryptographic hash chaining (SHA-256 / BLAKE3 compliant) for tamper detection and
//!   immutable evidence audit logs.
//! - `JsonlTracer` for high-performance sequential append and `verify_trace_chain` for verification.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

pub const GENESIS_PREV_HASH: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

/// Complete execution trace record representation for a single conversation turn or eval run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionTraceRecord {
    pub harness_version: String,
    pub session_id: String,
    pub turn_index: u32,
    pub timestamp_utc: String,
    pub principal: TracePrincipal,
    pub channel: TraceChannel,
    pub session_mode: TraceSessionMode,
    pub metrics: TraceMetrics,
    pub trajectory: Vec<TraceTrajectoryStep>,
    pub telemetry: TraceTelemetry,
    pub integrity: TraceIntegrity,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum TracePrincipal {
    Owner,
    OwnerDevice { device_id: String },
    Contact { contact_id: String },
    Unknown { channel: String, address: String },
    Skill { skill_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TraceChannel {
    Desktop,
    Telegram,
    Zalo,
    Companion,
    TestRunner,
    Cli,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TraceSessionMode {
    Reader,
    Actor,
    Browser,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TraceMetrics {
    pub raw_ttft_ms: u64,
    pub visible_ttft_ms: u64,
    pub total_duration_ms: u64,
    pub total_tokens: usize,
    pub visible_tokens: usize,
    pub reasoning_tokens: usize,
    pub tok_per_sec: f64,
    pub rss_idle_mb: f64,
    pub rss_peak_mb: f64,
    pub commit_charge_mb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TraceTrajectoryStep {
    pub step_index: u32,
    pub step_type: String,
    pub inputs: Vec<Value>,
    pub output: Value,
    pub tool_call: Option<TraceToolCall>,
    pub duration_us: u64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TraceToolCall {
    pub call_id: String,
    pub tool_name: String,
    pub canonical_args_hash: String,
    pub hmac_receipt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TraceTelemetry {
    pub model_id: String,
    pub backend: String,
    pub slot_id: u32,
    pub prefix_cache_hit: bool,
    pub n_ctx: usize,
    pub sys_ram_free_gb: f64,
    pub vram_allocated_mb: Option<f64>,
    pub per_token_breakdown: PerTokenBreakdown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PerTokenBreakdown {
    pub avg_tauri_ipc_us: u64,
    pub avg_cpu_sampling_us: u64,
    pub avg_flash_attention_us: u64,
    pub avg_backend_offload_us: u64,
    pub avg_grammar_masking_us: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TraceIntegrity {
    pub prev_hash: String,
    pub audit_hash: String,
}

/// Verification report returned by `verify_trace_chain`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TraceVerificationReport {
    pub total_records: usize,
    pub is_valid: bool,
    pub genesis_hash: String,
    pub latest_hash: String,
    pub file_path: String,
}

/// Computes the cryptographic audit hash of a trace record chained with `prev_hash`.
pub fn compute_audit_hash(prev_hash: &str, record: &ExecutionTraceRecord) -> String {
    let mut hasher = Sha256::new();
    hasher.update(prev_hash.as_bytes());
    hasher.update(b"|");

    // Create normalized copy without integrity.audit_hash
    let mut normalized = record.clone();
    normalized.integrity.prev_hash = prev_hash.to_string();
    normalized.integrity.audit_hash.clear();

    let json_bytes = serde_json::to_vec(&normalized).unwrap_or_default();
    hasher.update(&json_bytes);

    hex::encode(hasher.finalize())
}

/// Append-only JSONL execution tracer that enforces hash chaining.
pub struct JsonlTracer {
    output_path: PathBuf,
    last_audit_hash: String,
    record_count: usize,
}

impl JsonlTracer {
    /// Opens an existing JSONL file or creates a new one, recovering the latest audit hash.
    pub fn open_or_create(output_path: impl Into<PathBuf>) -> Result<Self, String> {
        let path = output_path.into();
        let mut last_hash = GENESIS_PREV_HASH.to_string();
        let mut count = 0;

        if path.exists() {
            let file = File::open(&path).map_err(|e| format!("cannot open trace file: {e}"))?;
            let reader = BufReader::new(file);
            for line_res in reader.lines() {
                let line = line_res.map_err(|e| format!("error reading trace line: {e}"))?;
                if line.trim().is_empty() {
                    continue;
                }
                let record: ExecutionTraceRecord = serde_json::from_str(&line)
                    .map_err(|e| format!("corrupt trace record in file: {e}"))?;
                last_hash = record.integrity.audit_hash;
                count += 1;
            }
        }

        Ok(Self {
            output_path: path,
            last_audit_hash: last_hash,
            record_count: count,
        })
    }

    /// Appends a new execution trace record, computing its audit hash and updating the chain.
    pub fn append_record(&mut self, record: &mut ExecutionTraceRecord) -> Result<String, String> {
        record.integrity.prev_hash = self.last_audit_hash.clone();
        let audit_hash = compute_audit_hash(&self.last_audit_hash, record);
        record.integrity.audit_hash = audit_hash.clone();

        if let Some(parent) = self.output_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create trace directory: {e}"))?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.output_path)
            .map_err(|e| format!("cannot open trace file for appending: {e}"))?;

        let line = serde_json::to_string(record)
            .map_err(|e| format!("failed to serialize trace record: {e}"))?;
        writeln!(file, "{line}").map_err(|e| format!("failed to write trace line: {e}"))?;

        self.last_audit_hash = audit_hash.clone();
        self.record_count += 1;

        Ok(audit_hash)
    }

    /// Returns the current latest audit hash in the chain.
    pub fn last_audit_hash(&self) -> &str {
        &self.last_audit_hash
    }

    /// Returns the number of records recorded in this session.
    pub fn record_count(&self) -> usize {
        self.record_count
    }
}

/// Verifies the cryptographic hash chain of an entire JSONL trace file.
pub fn verify_trace_chain(path: &Path) -> Result<TraceVerificationReport, String> {
    if !path.exists() {
        return Err(format!("trace file {:?} does not exist", path));
    }

    let file = File::open(path).map_err(|e| format!("cannot open file: {e}"))?;
    let reader = BufReader::new(file);

    let mut expected_prev = GENESIS_PREV_HASH.to_string();
    let mut genesis_hash = String::new();
    let mut latest_hash = String::new();
    let mut count = 0;

    for (index, line_res) in reader.lines().enumerate() {
        let line_num = index + 1;
        let line = line_res.map_err(|e| format!("line {line_num}: read error: {e}"))?;
        if line.trim().is_empty() {
            continue;
        }

        let record: ExecutionTraceRecord = serde_json::from_str(&line)
            .map_err(|e| format!("line {line_num}: invalid JSON schema: {e}"))?;

        if record.integrity.prev_hash != expected_prev {
            return Err(format!(
                "line {line_num}: broken hash chain link: expected prev_hash {}, got {}",
                expected_prev, record.integrity.prev_hash
            ));
        }

        let recomputed = compute_audit_hash(&expected_prev, &record);
        if recomputed != record.integrity.audit_hash {
            return Err(format!(
                "line {line_num}: tamper detected! recomputed audit_hash {} does not match recorded {}",
                recomputed, record.integrity.audit_hash
            ));
        }

        if count == 0 {
            genesis_hash = record.integrity.audit_hash.clone();
        }
        latest_hash = record.integrity.audit_hash.clone();
        expected_prev = record.integrity.audit_hash.clone();
        count += 1;
    }

    if count == 0 {
        return Err("trace file contains 0 records".to_string());
    }

    Ok(TraceVerificationReport {
        total_records: count,
        is_valid: true,
        genesis_hash,
        latest_hash,
        file_path: path.display().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_record(turn: u32, query: &str) -> ExecutionTraceRecord {
        ExecutionTraceRecord {
            harness_version: "liva-v1.0".into(),
            session_id: "test-session-001".into(),
            turn_index: turn,
            timestamp_utc: "2026-10-23T12:00:00Z".into(),
            principal: TracePrincipal::Owner,
            channel: TraceChannel::Desktop,
            session_mode: TraceSessionMode::Actor,
            metrics: TraceMetrics {
                raw_ttft_ms: 30,
                visible_ttft_ms: 75,
                total_duration_ms: 350,
                total_tokens: 45,
                visible_tokens: 35,
                reasoning_tokens: 10,
                tok_per_sec: 120.0,
                rss_idle_mb: 110.0,
                rss_peak_mb: 140.0,
                commit_charge_mb: 200.0,
            },
            trajectory: vec![TraceTrajectoryStep {
                step_index: 0,
                step_type: "PromptAssembly".into(),
                inputs: vec![Value::String(query.into())],
                output: Value::String("compiled prompt".into()),
                tool_call: None,
                duration_us: 1200,
                status: "Success".into(),
            }],
            telemetry: TraceTelemetry {
                model_id: "gemma-4-E4B-it-qat".into(),
                backend: "cuda".into(),
                slot_id: 0,
                prefix_cache_hit: true,
                n_ctx: 4096,
                sys_ram_free_gb: 24.0,
                vram_allocated_mb: Some(4200.0),
                per_token_breakdown: PerTokenBreakdown {
                    avg_tauri_ipc_us: 150,
                    avg_cpu_sampling_us: 200,
                    avg_flash_attention_us: 4200,
                    avg_backend_offload_us: 0,
                    avg_grammar_masking_us: 50,
                },
            },
            integrity: TraceIntegrity::default(),
        }
    }

    #[test]
    fn test_trace_record_serialization_and_hash_chain() {
        let temp_dir = std::env::temp_dir().join(format!("liva-trace-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        let trace_path = temp_dir.join("trace.jsonl");

        let mut tracer = JsonlTracer::open_or_create(&trace_path).expect("created tracer");
        assert_eq!(tracer.last_audit_hash(), GENESIS_PREV_HASH);

        let mut rec1 = dummy_record(0, "Bật đèn phòng khách");
        let hash1 = tracer.append_record(&mut rec1).expect("appended 1");
        assert_ne!(hash1, GENESIS_PREV_HASH);
        assert_eq!(rec1.integrity.prev_hash, GENESIS_PREV_HASH);
        assert_eq!(rec1.integrity.audit_hash, hash1);

        let mut rec2 = dummy_record(1, "Tắt quạt");
        let hash2 = tracer.append_record(&mut rec2).expect("appended 2");
        assert_ne!(hash2, hash1);
        assert_eq!(rec2.integrity.prev_hash, hash1);
        assert_eq!(rec2.integrity.audit_hash, hash2);

        // Verify chain integrity
        let report = verify_trace_chain(&trace_path).expect("chain valid");
        assert_eq!(report.total_records, 2);
        assert!(report.is_valid);
        assert_eq!(report.genesis_hash, hash1);
        assert_eq!(report.latest_hash, hash2);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_trace_tamper_detection() {
        let temp_dir =
            std::env::temp_dir().join(format!("liva-trace-tamper-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        let trace_path = temp_dir.join("trace.jsonl");

        let mut tracer = JsonlTracer::open_or_create(&trace_path).expect("created tracer");
        let mut rec1 = dummy_record(0, "Turn 0");
        let mut rec2 = dummy_record(1, "Turn 1");
        tracer.append_record(&mut rec1).unwrap();
        tracer.append_record(&mut rec2).unwrap();

        // Tamper with record 1 in file
        let content = std::fs::read_to_string(&trace_path).unwrap();
        let tampered = content.replace("Turn 0", "Hacked Prompt");
        std::fs::write(&trace_path, tampered).unwrap();

        let verify_res = verify_trace_chain(&trace_path);
        assert!(verify_res.is_err(), "verification must catch tampering");
        let err = verify_res.unwrap_err();
        assert!(
            err.contains("tamper detected"),
            "expected tamper message: {err}"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
