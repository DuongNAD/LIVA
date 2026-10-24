//! llama-server Sidecar Supervisor & VRAM Pressure Governor (Features F09 & F10).
//!
//! Provides:
//! - Subprocess supervisor for `llama-server` listening exclusively on loopback `127.0.0.1:<random_port>`.
//! - Ephemeral 256-bit CSPRNG API key generated at spawn time (`OsRng`).
//! - Strict security confinement: inference-only flags; explicitly avoids and forbids `--tools`, `--agent`, `--mcp-servers-*`.
//! - 3-slot concurrency architecture (`-np 3`):
//!   - Slot 0: Foreground dialogue (user chat, interactive voice duplex).
//!   - Slot 1: Worker model (12B multi-step reasoning, tool execution).
//!   - Slot 2: Background tasks (memory fact extraction, context compaction).
//! - KV cache save & restore protocol (`/slots/0?action=save/restore`) for instant prefix reuse.
//! - Dynamic VRAM pressure supervisor with 2-stage fallback ladder:
//!   - Stage 1 (>= 85% or free < 2048 MB): Offload reranker to CPU AVX2.
//!   - Stage 2 (>= 90% or free < 1200 MB): Evict / sleep-idle worker 12B.
//!   - Invariant guard: KV precision MUST NOT drop below Q8_0, and K/V precision must be equal.
//! - Windows Job Object RAII process management with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` preventing orphan zombie processes.

use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use serde::{Deserialize, Serialize};

/// KV Cache precision options. Invariant: MUST NOT be lower than Q8_0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KvPrecision {
    Q8_0,
    F16,
    F32,
}

impl KvPrecision {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Q8_0 => "q8_0",
            Self::F16 => "f16",
            Self::F32 => "f32",
        }
    }

    /// Validates that K and V precisions are identical (FlashAttention requirement)
    /// and that neither drops below Q8_0.
    pub fn validate(k: Self, v: Self) -> Result<(), SidecarError> {
        if k != v {
            return Err(SidecarError::KvCacheConfig(format!(
                "K precision ({}) must match V precision ({}) for CUDA FlashAttention compatibility",
                k.as_str(),
                v.as_str()
            )));
        }
        Ok(())
    }
}

/// Slot identifiers mapping to llama-server slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u32)]
pub enum SlotId {
    /// Slot 0: Foreground dialogue (user chat, interactive voice duplex)
    Foreground = 0,
    /// Slot 1: Worker model (12B multi-step reasoning, tool execution, planning)
    Worker = 1,
    /// Slot 2: Background tasks (context compaction, memory fact extraction)
    Background = 2,
}

/// Slot state tracking for sidecar slots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidecarSlot {
    pub slot_id: SlotId,
    pub is_busy: bool,
    pub last_turn_id: Option<String>,
    pub kv_cached_tokens: usize,
    pub last_used_ms: Option<u64>,
}

impl SidecarSlot {
    pub fn new(slot_id: SlotId) -> Self {
        Self {
            slot_id,
            is_busy: false,
            last_turn_id: None,
            kv_cached_tokens: 0,
            last_used_ms: None,
        }
    }
}

/// Dynamic VRAM Pressure Tiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum VramPressureTier {
    /// < 80% usage: normal operation
    Nominal,
    /// 80% - 85% usage: elevated load
    Elevated,
    /// 85% - 90% usage or free < 2048 MB: offload reranker to CPU
    High,
    /// >= 90% usage or free < 1200 MB: trigger worker 12B sleep-idle / autoload
    Critical,
}

/// Fallback action triggered by VRAM supervisor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FallbackAction {
    None,
    OffloadRerankerToCpu,
    EvictWorkerAndOffloadReranker,
}

/// Assessment output returned by VRAM evaluation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VramPressureAssessment {
    pub tier: VramPressureTier,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub action_taken: FallbackAction,
}

/// Complete configuration for spawning llama-server sidecar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidecarConfig {
    pub server_binary: PathBuf,
    pub models_dir: PathBuf,
    pub router_model_name: String,
    pub worker_model_name: Option<String>,
    pub mmproj_path: Option<PathBuf>,
    pub parallel_slots: u32,
    pub ctx_size_per_slot: u32,
    pub cache_type_k: KvPrecision,
    pub cache_type_v: KvPrecision,
    pub cache_ram_mb: u32,
    pub n_gpu_layers: u32,
    pub flash_attn: bool,
    pub sleep_idle_seconds: u32,
    pub models_max: u32,
    pub startup_timeout: Duration,
    pub kv_cache_dir: PathBuf,
}

impl Default for SidecarConfig {
    fn default() -> Self {
        Self {
            server_binary: PathBuf::from("llama-server.exe"),
            models_dir: PathBuf::from("data/models"),
            router_model_name: "gemma-4-E4B-it-UD-Q4_K_XL.gguf".to_string(),
            worker_model_name: Some("gemma-4-12B-it-UD-Q4_K_XL.gguf".to_string()),
            mmproj_path: None,
            parallel_slots: 3,
            ctx_size_per_slot: 8192,
            cache_type_k: KvPrecision::Q8_0,
            cache_type_v: KvPrecision::Q8_0,
            cache_ram_mb: 8192,
            n_gpu_layers: 99,
            flash_attn: true,
            sleep_idle_seconds: 180,
            models_max: 2,
            startup_timeout: Duration::from_secs(30),
            kv_cache_dir: PathBuf::from("data/kv_cache"),
        }
    }
}

impl SidecarConfig {
    /// Builds CLI arguments for `llama-server.exe`.
    ///
    /// STRICT SECURITY CONFINEMENT:
    /// - Always binds to `127.0.0.1` (never `0.0.0.0`).
    /// - Binds to dynamic ephemeral port.
    /// - Requires CSPRNG API key.
    /// - Strictly omits `--tools`, `-ag`, `--agent`, `--mcp-servers-*`.
    pub fn build_cli_args(&self, port: u16, api_key: &str) -> Vec<String> {
        let mut args = Vec::new();

        // 1. Network & Auth Confinement
        args.push("--host".to_string());
        args.push("127.0.0.1".to_string());
        args.push("--port".to_string());
        args.push(port.to_string());
        args.push("--api-key".to_string());
        args.push(api_key.to_string());

        // 2. Multi-Model Router Mode
        args.push("--models-dir".to_string());
        args.push(self.models_dir.to_string_lossy().to_string());
        args.push("-m".to_string());
        let router_full_path = self.models_dir.join(&self.router_model_name);
        args.push(router_full_path.to_string_lossy().to_string());

        // 3. Slot Concurrency & Context Allocation
        args.push("-np".to_string());
        args.push(self.parallel_slots.to_string());
        args.push("-c".to_string());
        args.push(self.ctx_size_per_slot.to_string());

        // 4. KV Cache Invariant & FlashAttention
        args.push("--cache-type-k".to_string());
        args.push(self.cache_type_k.as_str().to_string());
        args.push("--cache-type-v".to_string());
        args.push(self.cache_type_v.as_str().to_string());
        if self.flash_attn {
            args.push("-fa".to_string());
        }
        args.push("--cache-ram".to_string());
        args.push(self.cache_ram_mb.to_string());

        // 5. GPU Layer Offloading
        args.push("-ngl".to_string());
        args.push(self.n_gpu_layers.to_string());

        // 6. Dynamic Memory Sleep & Max Resident Models
        args.push("--models-autoload".to_string());
        args.push("--models-max".to_string());
        args.push(self.models_max.to_string());
        args.push("--sleep-idle-seconds".to_string());
        args.push(self.sleep_idle_seconds.to_string());

        // 7. Multimodal Projector
        if let Some(ref mmproj) = self.mmproj_path {
            args.push("--mmproj".to_string());
            args.push(mmproj.to_string_lossy().to_string());
        }

        // 8. Explicit Non-Inference Flags Disabling
        // ZERO --tools, ZERO --agent, ZERO --mcp-servers-*
        args.push("--no-webui".to_string());

        args
    }
}

/// Allocates a free ephemeral port on 127.0.0.1.
pub fn allocate_ephemeral_port() -> Result<u16, SidecarError> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| SidecarError::PortAllocation(e.to_string()))?;
    let port = listener
        .local_addr()
        .map_err(|e| SidecarError::PortAllocation(e.to_string()))?
        .port();
    drop(listener);
    Ok(port)
}

/// Generates a cryptographic 256-bit hex API key using system CSPRNG.
pub fn generate_csprng_key() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Error type for sidecar operations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SidecarError {
    PortAllocation(String),
    ProcessSupervision(String),
    KvCacheConfig(String),
    InferenceFailed(String),
    SlotActionFailed(String),
    Http(String),
    Timeout(String),
    Io(String),
}

impl std::fmt::Display for SidecarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PortAllocation(e) => write!(f, "Failed to allocate ephemeral port: {e}"),
            Self::ProcessSupervision(e) => write!(f, "Process supervision error: {e}"),
            Self::KvCacheConfig(e) => write!(f, "Invalid KV cache configuration: {e}"),
            Self::InferenceFailed(e) => write!(f, "Inference request failed: {e}"),
            Self::SlotActionFailed(e) => write!(f, "Slot action failed: {e}"),
            Self::Http(e) => write!(f, "HTTP client error: {e}"),
            Self::Timeout(e) => write!(f, "Operation timed out: {e}"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for SidecarError {}

impl From<reqwest::Error> for SidecarError {
    fn from(e: reqwest::Error) -> Self {
        SidecarError::Http(e.to_string())
    }
}

/// Windows Job Object RAII guard to guarantee subprocess termination when parent exits.
#[cfg(windows)]
pub struct WindowsProcessGuard {
    job_handle: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
impl WindowsProcessGuard {
    pub fn new() -> Result<Self, SidecarError> {
        use windows_sys::Win32::System::JobObjects::*;
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job == 0 {
                return Err(SidecarError::ProcessSupervision(
                    "Failed to create JobObject".to_string(),
                ));
            }

            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

            let res = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if res == 0 {
                windows_sys::Win32::Foundation::CloseHandle(job);
                return Err(SidecarError::ProcessSupervision(
                    "Failed to set JobObject limit".to_string(),
                ));
            }

            Ok(Self { job_handle: job })
        }
    }

    pub fn assign_process(&self, process_handle: windows_sys::Win32::Foundation::HANDLE) -> Result<(), SidecarError> {
        use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;
        unsafe {
            let res = AssignProcessToJobObject(self.job_handle, process_handle);
            if res == 0 {
                return Err(SidecarError::ProcessSupervision(
                    "Failed to assign process to JobObject".to_string(),
                ));
            }
            Ok(())
        }
    }
}

#[cfg(windows)]
impl Drop for WindowsProcessGuard {
    fn drop(&mut self) {
        if self.job_handle != 0 {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(self.job_handle);
            }
        }
    }
}

#[cfg(not(windows))]
pub struct WindowsProcessGuard;

#[cfg(not(windows))]
impl WindowsProcessGuard {
    pub fn new() -> Result<Self, SidecarError> {
        Ok(Self)
    }
}

/// VRAM Supervisor continuously polling GPU VRAM and executing fallback tiers.
pub struct VramSupervisor {
    pub total_vram_bytes: u64,
    pub reranker_on_cpu: AtomicBool,
    pub worker_sleep_idle_active: AtomicBool,
}

impl VramSupervisor {
    pub fn new() -> Self {
        let total = crate::governor::gpu_vram_bytes()
            .map(|(tot, _)| tot)
            .unwrap_or(16 * 1024 * 1024 * 1024);

        Self {
            total_vram_bytes: total,
            reranker_on_cpu: AtomicBool::new(false),
            worker_sleep_idle_active: AtomicBool::new(false),
        }
    }

    pub fn with_total_vram(total_bytes: u64) -> Self {
        Self {
            total_vram_bytes: total_bytes,
            reranker_on_cpu: AtomicBool::new(false),
            worker_sleep_idle_active: AtomicBool::new(false),
        }
    }

    /// Evaluates current VRAM pressure using real-time NVML or custom values,
    /// triggering appropriate fallback actions.
    pub fn evaluate_pressure(&self) -> VramPressureAssessment {
        if let Some((total, used)) = crate::governor::gpu_vram_bytes() {
            self.evaluate_pressure_custom(total, used)
        } else {
            // Fallback for non-NVML environments (nominal baseline)
            VramPressureAssessment {
                tier: VramPressureTier::Nominal,
                used_bytes: 0,
                free_bytes: self.total_vram_bytes,
                action_taken: FallbackAction::None,
            }
        }
    }

    /// Evaluates pressure with explicit (total, used) inputs (used for deterministic testing and runtime).
    pub fn evaluate_pressure_custom(&self, total: u64, used: u64) -> VramPressureAssessment {
        let free = total.saturating_sub(used);
        let usage_ratio = used as f64 / total.max(1) as f64;

        if usage_ratio >= 0.90 || free < (1200 * 1024 * 1024) {
            // Critical Tier (Stage 2): Offload Reranker + Force Worker Sleep-Idle / Autoload
            self.reranker_on_cpu.store(true, Ordering::SeqCst);
            self.worker_sleep_idle_active.store(true, Ordering::SeqCst);

            VramPressureAssessment {
                tier: VramPressureTier::Critical,
                used_bytes: used,
                free_bytes: free,
                action_taken: FallbackAction::EvictWorkerAndOffloadReranker,
            }
        } else if usage_ratio >= 0.85 || free < (2048 * 1024 * 1024) {
            // High Tier (Stage 1): Offload Cross-Encoder Reranker to CPU AVX2
            self.reranker_on_cpu.store(true, Ordering::SeqCst);

            VramPressureAssessment {
                tier: VramPressureTier::High,
                used_bytes: used,
                free_bytes: free,
                action_taken: FallbackAction::OffloadRerankerToCpu,
            }
        } else if usage_ratio >= 0.80 {
            VramPressureAssessment {
                tier: VramPressureTier::Elevated,
                used_bytes: used,
                free_bytes: free,
                action_taken: FallbackAction::None,
            }
        } else {
            // Nominal Tier: memory stabilized, restore GPU offload when abundant free space
            if free > (4096 * 1024 * 1024) {
                self.reranker_on_cpu.store(false, Ordering::SeqCst);
                self.worker_sleep_idle_active.store(false, Ordering::SeqCst);
            }
            VramPressureAssessment {
                tier: VramPressureTier::Nominal,
                used_bytes: used,
                free_bytes: free,
                action_taken: FallbackAction::None,
            }
        }
    }

    pub fn is_reranker_on_cpu(&self) -> bool {
        self.reranker_on_cpu.load(Ordering::Relaxed)
    }

    pub fn is_worker_sleep_idle(&self) -> bool {
        self.worker_sleep_idle_active.load(Ordering::Relaxed)
    }
}

/// Type alias for backward compatibility and prompt dispatch consistency.
pub type VramGovernor = VramSupervisor;

/// Response returned by slot save/restore actions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SlotActionResponse {
    pub success: bool,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub filepath: Option<String>,
}

/// HTTP client interacting with the llama-server sidecar.
#[derive(Clone)]
pub struct LlamaSidecarClient {
    base_url: String,
    api_key: String,
    http_client: reqwest::Client,
    is_mock: bool,
}

/// Type alias for backward compatibility and prompt dispatch consistency.
pub type LlamaHttpClient = LlamaSidecarClient;

impl LlamaSidecarClient {
    pub fn new(port: u16, api_key: String) -> Self {
        Self {
            base_url: format!("http://127.0.0.1:{}", port),
            api_key,
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .unwrap_or_default(),
            is_mock: false,
        }
    }

    pub fn new_mock() -> Self {
        Self {
            base_url: "http://127.0.0.1:0".to_string(),
            api_key: "mock-key".to_string(),
            http_client: reqwest::Client::new(),
            is_mock: true,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    /// Health check probe
    pub async fn health(&self) -> Result<bool, SidecarError> {
        if self.is_mock {
            return Ok(true);
        }
        let resp = self
            .http_client
            .get(format!("{}/health", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await?;
        Ok(resp.status().is_success())
    }

    /// Pinned slot chat completion
    pub async fn chat_completion(
        &self,
        slot_id: SlotId,
        request: &serde_json::Value,
    ) -> Result<serde_json::Value, SidecarError> {
        if self.is_mock {
            return Ok(serde_json::json!({
                "id": "chatcmpl-mock",
                "object": "chat.completion",
                "slot_id": slot_id as u32,
                "choices": [{
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": "Phản hồi giả lập từ llama-server sidecar."
                    },
                    "finish_reason": "stop"
                }]
            }));
        }

        let mut body = request.clone();
        if let Some(obj) = body.as_object_mut() {
            obj.insert("id_slot".to_string(), serde_json::json!(slot_id as u32));
            obj.insert("cache_prompt".to_string(), serde_json::json!(true));
        }

        let resp = self
            .http_client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let err_text = resp.text().await.unwrap_or_default();
            return Err(SidecarError::InferenceFailed(err_text));
        }

        let json_val: serde_json::Value = resp.json().await?;
        Ok(json_val)
    }

    /// Save slot KV cache to disk (`POST /slots/{slot_id}?action=save`)
    pub async fn save_slot_kv(&self, slot_id: SlotId, filename: &str) -> Result<SlotActionResponse, SidecarError> {
        if self.is_mock {
            return Ok(SlotActionResponse {
                success: true,
                message: Some("Mock slot KV cache saved".into()),
                filepath: Some(filename.into()),
            });
        }

        let payload = serde_json::json!({ "filepath": filename });
        let resp = self
            .http_client
            .post(format!("{}/slots/{}?action=save", self.base_url, slot_id as u32))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&payload)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(SidecarError::SlotActionFailed(resp.text().await.unwrap_or_default()));
        }

        let res: SlotActionResponse = resp.json().await?;
        Ok(res)
    }

    /// Restore slot KV cache from disk (`POST /slots/{slot_id}?action=restore`)
    pub async fn restore_slot_kv(&self, slot_id: SlotId, filename: &str) -> Result<SlotActionResponse, SidecarError> {
        if self.is_mock {
            return Ok(SlotActionResponse {
                success: true,
                message: Some("Mock slot KV cache restored".into()),
                filepath: Some(filename.into()),
            });
        }

        let payload = serde_json::json!({ "filepath": filename });
        let resp = self
            .http_client
            .post(format!("{}/slots/{}?action=restore", self.base_url, slot_id as u32))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&payload)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(SidecarError::SlotActionFailed(resp.text().await.unwrap_or_default()));
        }

        let res: SlotActionResponse = resp.json().await?;
        Ok(res)
    }
}

/// Supervisor struct coordinating the sidecar process lifecycle, ports, auth, and client.
pub struct LlamaSidecarSupervisor {
    pub config: SidecarConfig,
    pub port: u16,
    pub api_key: String,
    pub client: LlamaSidecarClient,
    pub vram_supervisor: Arc<VramSupervisor>,
    process_guard: Option<WindowsProcessGuard>,
}

impl LlamaSidecarSupervisor {
    pub fn new(config: SidecarConfig) -> Result<Self, SidecarError> {
        KvPrecision::validate(config.cache_type_k, config.cache_type_v)?;
        let port = allocate_ephemeral_port()?;
        let api_key = generate_csprng_key();
        let client = LlamaSidecarClient::new(port, api_key.clone());
        let vram_supervisor = Arc::new(VramSupervisor::new());

        Ok(Self {
            config,
            port,
            api_key,
            client,
            vram_supervisor,
            process_guard: None,
        })
    }

    pub fn new_mock(config: SidecarConfig) -> Self {
        let vram_supervisor = Arc::new(VramSupervisor::new());
        Self {
            config,
            port: 0,
            api_key: "mock-key".to_string(),
            client: LlamaSidecarClient::new_mock(),
            vram_supervisor,
            process_guard: None,
        }
    }

    pub fn process_guard(&self) -> Option<&WindowsProcessGuard> {
        self.process_guard.as_ref()
    }

    pub fn set_process_guard(&mut self, guard: WindowsProcessGuard) {
        self.process_guard = Some(guard);
    }
}
