pub use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WeatherArgs {
    /// Tên địa điểm nếu người dùng nói rõ. Bỏ trống để dùng profile hoặc vị trí IP đã opt-in.
    #[serde(default)]
    pub location: Option<String>,
}

/// Turn verdict with probability and completion flag.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TurnVerdict {
    pub probability: f32,
    pub complete: bool,
}

/// Adaptive end-of-turn decision for the Two-Stage Turn Gate.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AdaptiveTurnDecision {
    /// Turn clearly complete (p > 0.92). Immediate cutoff at ~200ms silence.
    ImmediateCutoff { probability: f32 },
    /// Hesitation / thinking pause (0.50 <= p <= 0.92). Extend silence window up to ~450ms.
    HesitationWait { probability: f32 },
    /// User still actively speaking / incomplete (p < 0.50). Continue accumulating.
    Incomplete { probability: f32 },
}

impl AdaptiveTurnDecision {
    pub fn from_probability(p: f32) -> Self {
        if p > 0.92 {
            Self::ImmediateCutoff { probability: p }
        } else if p >= 0.50 {
            Self::HesitationWait { probability: p }
        } else {
            Self::Incomplete { probability: p }
        }
    }

    pub fn probability(&self) -> f32 {
        match *self {
            Self::ImmediateCutoff { probability }
            | Self::HesitationWait { probability }
            | Self::Incomplete { probability } => probability,
        }
    }

    pub fn is_immediate(&self) -> bool {
        matches!(self, Self::ImmediateCutoff { .. })
    }
}

// -----------------------------------------------------------------------------
// RFC 1: SCOPED TOOL REGISTRY & GUARDED EXECUTION TYPES
// -----------------------------------------------------------------------------

/// Principal-aware command authorization channel.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CommandPrincipal {
    Telegram,
    TauriSetup,
    TauriWidget,
    TauriDashboard,
    Test,
    LocalCli,
}

impl CommandPrincipal {
    /// Monotonic privilege hierarchy: higher number denotes strictly higher authority.
    ///
    /// 1 = Telegram (untrusted external webhook, read-only)
    /// 2 = TauriSetup (initial onboarding/provisioning only)
    /// 3 = TauriWidget (voice/avatar/lightweight UI commands)
    /// 4 = TauriDashboard (comprehensive management UI)
    /// 5 = Test / LocalCli (co-equal trusted local root)
    pub fn privilege_level(&self) -> u8 {
        match self {
            Self::Telegram => 1,
            Self::TauriSetup => 2,
            Self::TauriWidget => 3,
            Self::TauriDashboard => 4,
            Self::Test => 5,
            Self::LocalCli => 5,
        }
    }

    /// Checks if this principal satisfies or exceeds the required principal level.
    pub fn satisfies(&self, required: CommandPrincipal) -> bool {
        self.privilege_level() >= required.privilege_level()
    }
}

/// Execution policy defining auto-execution permissions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolExecPolicy {
    /// Safe, read-only or reversible operation; can execute automatically.
    Auto,
    /// Dangerous or state-mutating operation; requires user confirmation.
    ProposeOnly,
    /// Completely disabled in the current scope.
    Forbidden,
}

/// Lifecycle scope for registered tools.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolScope {
    /// Available across all sessions (e.g. native vault tools).
    Global,
    /// Scoped to a specific conversation or session ID.
    Session(String),
    /// Ephemeral scope for a single multi-step task or subagent run.
    Task(String),
}

/// Execution context injected into every tool invocation.
#[derive(Clone, Debug)]
pub struct ToolExecutionContext {
    pub session_id: String,
    pub caller_principal: CommandPrincipal,
    pub working_dir: std::path::PathBuf,
    pub event_tx: Option<tokio::sync::mpsc::Sender<String>>,
    pub token_budget_remaining: usize,
}

impl ToolExecutionContext {
    pub fn new(
        session_id: impl Into<String>,
        caller_principal: CommandPrincipal,
        working_dir: std::path::PathBuf,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            caller_principal,
            working_dir,
            event_tx: None,
            token_budget_remaining: 4096,
        }
    }
}

/// Tool definition and JSON schema contract.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub input_schema: schemars::schema::RootSchema,
}

/// Content payload within a tool execution result.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum ToolContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image", rename_all = "camelCase")]
    Image { data: String, mime_type: String },
    #[serde(other)]
    Unsupported,
}

/// Result of calling a scoped tool.
#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CallToolResult {
    #[serde(default)]
    pub content: Vec<ToolContent>,
    #[serde(default)]
    pub is_error: bool,
}

impl CallToolResult {
    pub fn success(text: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent::Text { text: text.into() }],
            is_error: false,
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent::Text { text: text.into() }],
            is_error: true,
        }
    }
}

/// Asynchronous trait implemented by all native and dynamic scoped tools.
#[async_trait::async_trait]
pub trait ScopedTool: Send + Sync {
    /// Tool definition and schema.
    fn definition(&self) -> Tool;

    /// Default execution policy for this tool.
    fn default_policy(&self) -> ToolExecPolicy {
        ToolExecPolicy::Auto
    }

    /// Minimum principal level required to invoke this tool.
    fn required_principal(&self) -> CommandPrincipal {
        CommandPrincipal::LocalCli
    }

    /// Execute the tool with sandboxed context.
    async fn execute(
        &self,
        arguments: serde_json::Value,
        context: ToolExecutionContext,
    ) -> Result<CallToolResult, String>;
}

// -----------------------------------------------------------------------------
// RFC 2: SESSION ACTOR & QUEUE MODE TYPES (F07 & F08)
// -----------------------------------------------------------------------------

/// 4 session turn queue modes governing concurrency and execution order (Feature F08).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionQueueMode {
    /// Injects steering guidance into the currently executing turn without restarting it.
    Steer,
    /// Enqueues the turn in a FIFO backlog for sequential execution after the current turn ends.
    Followup,
    /// Coalesces rapid sequential messages within a debounce window into a single batch turn.
    Collect,
    /// Immediately aborts the currently active turn via atomic cancellation and begins a new turn.
    Interrupt,
}

/// Request to execute a conversational turn within a session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionTurnRequest {
    pub session_id: String,
    pub turn_id: String,
    pub input_text: String,
    pub queue_mode: SessionQueueMode,
    pub principal: CommandPrincipal,
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub session_mode: SessionMode,
    #[serde(default)]
    pub turn_principal: Principal,
}

impl SessionTurnRequest {
    pub fn new(
        session_id: impl Into<String>,
        turn_id: impl Into<String>,
        input_text: impl Into<String>,
        queue_mode: SessionQueueMode,
        principal: CommandPrincipal,
    ) -> Self {
        let turn_principal = Principal::from(principal);
        let session_mode = if turn_principal.is_owner() {
            SessionMode::Actor
        } else {
            SessionMode::Reader
        };
        Self {
            session_id: session_id.into(),
            turn_id: turn_id.into(),
            input_text: input_text.into(),
            queue_mode,
            principal,
            metadata: std::collections::HashMap::new(),
            session_mode,
            turn_principal,
        }
    }

    pub fn with_mode_and_principal(mut self, mode: SessionMode, principal: Principal) -> Self {
        self.session_mode = mode;
        self.turn_principal = principal;
        self
    }
}

impl Default for SessionQueueMode {
    fn default() -> Self {
        Self::Followup
    }
}

impl Default for SessionTurnRequest {
    fn default() -> Self {
        Self {
            session_id: String::new(),
            turn_id: String::new(),
            input_text: String::new(),
            queue_mode: SessionQueueMode::Followup,
            principal: CommandPrincipal::LocalCli,
            metadata: std::collections::HashMap::new(),
            session_mode: SessionMode::Actor,
            turn_principal: Principal::Owner,
        }
    }
}



// -----------------------------------------------------------------------------
// RFC 3: SECURITY, RULE-OF-TWO, TAINT & CAPABILITY TYPES (M4: F19-F25)
// -----------------------------------------------------------------------------

/// 3 chế độ phiên hội thoại tuân thủ nguyên tắc Rule-of-Two (Feature F19).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionMode {
    /// Reader (A + B): Xử lý đầu vào không tin cậy + Đọc dữ liệu nhạy cảm/vault.
    /// Nghiêm cấm mọi hành vi đột biến trạng thái hoặc liên lạc ra ngoài (Vertex C = 0).
    Reader,
    /// Actor (B + C): Đọc dữ liệu nhạy cảm + Đột biến trạng thái/giao tiếp ngoài.
    /// Dành riêng cho lượt tin cậy của Owner; nghiêm cấm đầu vào không tin cậy trong ngữ cảnh (Vertex A = 0).
    Actor,
    /// Browser (A + C): Xử lý đầu vào không tin cậy + Hành động trên web công khai.
    /// Cách ly hoàn toàn khỏi kho vault cá nhân, bộ nhớ SQLite và bí mật hệ thống (Vertex B = 0).
    Browser,
}

impl Default for SessionMode {
    fn default() -> Self {
        Self::Actor
    }
}

/// 3 đỉnh an ninh trong Meta's Rule of Two.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RuleOfTwoVertices {
    /// Đỉnh A: Đầu vào không tin cậy (web, email, tin nhắn người lạ, OCR)
    pub untrusted_input: bool,
    /// Đỉnh B: Dữ liệu cá nhân nhạy cảm (vault, memory facts, credentials, profile)
    pub sensitive_data: bool,
    /// Đỉnh C: Đột biến trạng thái hoặc liên lạc ra ngoài (messaging, file write, shell, API)
    pub external_mutation: bool,
}

impl RuleOfTwoVertices {
    /// Kiểm tra bất biến: Có tối đa 2 trong 3 đỉnh được kích hoạt đồng thời.
    #[inline]
    pub fn is_valid(&self) -> bool {
        let count = (self.untrusted_input as u8)
            + (self.sensitive_data as u8)
            + (self.external_mutation as u8);
        count <= 2
    }

    pub fn validate(&self) -> Result<(), SecurityError> {
        if self.is_valid() {
            Ok(())
        } else {
            Err(SecurityError::RuleOfTwoViolation(
                "Rule-of-Two violated: cannot activate untrusted input, sensitive data, and external mutation simultaneously".into(),
            ))
        }
    }
}

/// Mức độ tin cậy của liên hệ trong danh bạ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContactTrustLevel {
    Acquaintance = 0,
    Colleague = 1,
    Family = 2,
}

/// Thang quyền hạn phân cấp của Principal (Feature F21).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityTier {
    Untrusted = 0,
    KnownContact = 1,
    OwnerDevice = 2,
    Owner = 3,
}

/// Định danh chi tiết của thực thể bắt đầu lượt hội thoại (Feature F21).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Principal {
    /// Người dùng trực tiếp tại máy trạm
    Owner,
    /// Thiết bị đồng hành đã ghép nối và xác thực chữ ký (Noise IK / Ed25519)
    OwnerDevice {
        device_id: String,
        device_name: String,
        public_key_fingerprint: String,
    },
    /// Liên hệ đã lưu trong danh bạ SQLite
    KnownContact {
        contact_id: String,
        display_name: String,
        channel: String,
        identifier: String,
        trust_level: ContactTrustLevel,
    },
    /// Thực thể bên ngoài chưa xác thực (Telegram, Zalo, Email lạ)
    UntrustedExternal {
        channel_id: String,
        sender_address: String,
        is_public_channel: bool,
    },
    /// Nguồn nội dung thụ động (Web scrape, OCR, tài liệu đính kèm)
    ContentSource {
        source_type: String,
        uri_or_origin: String,
    },
    /// Kỹ năng hoặc tác vụ nền kế thừa quyền từ principal cha
    Skill {
        skill_id: String,
        parent_principal: Box<Principal>,
    },
}

impl Default for Principal {
    fn default() -> Self {
        Self::Owner
    }
}

impl Principal {
    pub fn authority_tier(&self) -> AuthorityTier {
        match self {
            Self::Owner => AuthorityTier::Owner,
            Self::OwnerDevice { .. } => AuthorityTier::OwnerDevice,
            Self::KnownContact { .. } => AuthorityTier::KnownContact,
            Self::UntrustedExternal { .. } | Self::ContentSource { .. } => AuthorityTier::Untrusted,
            Self::Skill { parent_principal, .. } => parent_principal.authority_tier(),
        }
    }

    pub fn is_owner(&self) -> bool {
        matches!(self, Self::Owner | Self::OwnerDevice { .. })
    }
}

impl From<CommandPrincipal> for Principal {
    fn from(cp: CommandPrincipal) -> Self {
        match cp {
            CommandPrincipal::LocalCli
            | CommandPrincipal::Test
            | CommandPrincipal::TauriDashboard
            | CommandPrincipal::TauriWidget
            | CommandPrincipal::TauriSetup => Principal::Owner,
            CommandPrincipal::Telegram => Principal::UntrustedExternal {
                channel_id: "telegram".into(),
                sender_address: "telegram_user".into(),
                is_public_channel: false,
            },
        }
    }
}

/// Mức độ toàn vẹn của dữ liệu trong hệ thống (Feature F20).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrityLevel {
    Trusted = 0,
    SemiTrusted = 1,
    Untrusted = 2,
    Quarantined = 3,
}

impl Default for IntegrityLevel {
    fn default() -> Self {
        Self::Trusted
    }
}

/// Nguồn gốc dữ liệu phục vụ truy vết provenance (Feature F20).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DataOrigin {
    OwnerDirect { channel: String, timestamp: u64 },
    CompanionSigned { device_id: String, key_id: String },
    TelegramMessage { chat_id: i64, sender_id: i64, is_owner: bool },
    ZaloMessage { user_id: String, is_owner: bool },
    WebScrape { url: String, domain: String, fetched_at: u64 },
    DesktopOcr { window_title: String, process_name: String },
    FileAttachment { file_name: String, path: String },
    McpToolOutput { server_name: String, tool_name: String },
    AssistantProse { turn_id: String },
    Composite(Vec<DataOrigin>),
}

/// Cờ cảnh báo nguy cơ độc hại đính kèm dữ liệu (Feature F20).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct TaintFlags {
    pub untrusted_external: bool,
    pub prompt_injection_risk: bool,
    pub contains_urls: bool,
    pub invisible_unicode: bool,
    pub memghost_attempt: bool,
    pub sensitive_pii_present: bool,
}

/// Biên nhận giải mật dữ liệu sau khi được người dùng duyệt hoặc kiểm định (Feature F20).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeclassificationReceipt {
    pub receipt_id: String,
    pub authorized_by: String,
    pub payload_blake3_hash: String,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
}

impl DeclassificationReceipt {
    pub fn is_valid_for(&self, _origin: &DataOrigin) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        now <= self.expires_at_ms
    }
}

/// Vỏ bọc generic bảo vệ dữ liệu ô nhiễm theo Type System của Rust (Feature F20).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Labeled<T> {
    inner: T,
    origin: DataOrigin,
    integrity: IntegrityLevel,
    taint_flags: TaintFlags,
}

impl<T> Labeled<T> {
    /// Tạo vỏ bọc ô nhiễm cho dữ liệu từ nguồn ngoài.
    pub fn new_untrusted(data: T, origin: DataOrigin) -> Self {
        Self {
            inner: data,
            origin,
            integrity: IntegrityLevel::Untrusted,
            taint_flags: TaintFlags {
                untrusted_external: true,
                ..Default::default()
            },
        }
    }

    /// Tạo dữ liệu tin cậy cho Owner.
    pub fn new_owner(data: T, channel: impl Into<String>) -> Self {
        Self {
            inner: data,
            origin: DataOrigin::OwnerDirect {
                channel: channel.into(),
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
            },
            integrity: IntegrityLevel::Trusted,
            taint_flags: TaintFlags::default(),
        }
    }

    #[inline]
    pub fn is_tainted(&self) -> bool {
        self.integrity != IntegrityLevel::Trusted
    }

    #[inline]
    pub fn origin(&self) -> &DataOrigin {
        &self.origin
    }

    #[inline]
    pub fn integrity(&self) -> IntegrityLevel {
        self.integrity
    }

    #[inline]
    pub fn flags(&self) -> TaintFlags {
        self.taint_flags
    }

    #[inline]
    pub fn as_inner(&self) -> &T {
        &self.inner
    }

    /// Functor Map: Biến đổi nội dung bên trong nhưng bảo toàn 100% nguồn gốc và nhãn ô nhiễm.
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Labeled<U> {
        Labeled {
            inner: f(self.inner),
            origin: self.origin,
            integrity: self.integrity,
            taint_flags: self.taint_flags,
        }
    }

    /// Hợp nhất hai mảnh dữ liệu: nhãn kết quả mang mức độ ô nhiễm cao hơn và gộp nguồn gốc.
    pub fn combine<U, R, F: FnOnce(T, U) -> R>(self, other: Labeled<U>, f: F) -> Labeled<R> {
        let highest_integrity = self.integrity.max(other.integrity);
        let composite_origin = match (self.origin, other.origin) {
            (DataOrigin::Composite(mut a), DataOrigin::Composite(b)) => {
                a.extend(b);
                DataOrigin::Composite(a)
            }
            (DataOrigin::Composite(mut a), single) => {
                a.push(single);
                DataOrigin::Composite(a)
            }
            (single, DataOrigin::Composite(mut b)) => {
                let mut v = vec![single];
                v.append(&mut b);
                DataOrigin::Composite(v)
            }
            (a, b) => DataOrigin::Composite(vec![a, b]),
        };

        let merged_flags = TaintFlags {
            untrusted_external: self.taint_flags.untrusted_external || other.taint_flags.untrusted_external,
            prompt_injection_risk: self.taint_flags.prompt_injection_risk || other.taint_flags.prompt_injection_risk,
            contains_urls: self.taint_flags.contains_urls || other.taint_flags.contains_urls,
            invisible_unicode: self.taint_flags.invisible_unicode || other.taint_flags.invisible_unicode,
            memghost_attempt: self.taint_flags.memghost_attempt || other.taint_flags.memghost_attempt,
            sensitive_pii_present: self.taint_flags.sensitive_pii_present || other.taint_flags.sensitive_pii_present,
        };

        Labeled {
            inner: f(self.inner, other.inner),
            origin: composite_origin,
            integrity: highest_integrity,
            taint_flags: merged_flags,
        }
    }

    /// Giải mật (Declassify) dữ liệu khi có biên nhận phê duyệt hợp lệ.
    pub fn declassify(self, receipt: &DeclassificationReceipt) -> Result<T, SecurityError> {
        if receipt.is_valid_for(&self.origin) {
            Ok(self.inner)
        } else {
            Err(SecurityError::InvalidDeclassificationReceipt(
                "Declassification receipt is invalid or expired".into(),
            ))
        }
    }

    /// Giải nén không an toàn (chỉ dùng trong module nội bộ đã qua kiểm toán).
    pub unsafe fn unwrap_unchecked(self) -> T {
        self.inner
    }
}

/// Lỗi an ninh toàn diện cho hệ thống (Features F19–F25).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecurityError {
    /// Vi phạm Rule-of-Two: Gọi công cụ đột biến trong chế độ Reader
    MutationForbiddenInReaderMode(String),
    /// Vi phạm Rule-of-Two: Truy cập vault hoặc bí mật trong chế độ Browser
    VaultAccessForbiddenInBrowserMode(String),
    /// Vi phạm Rule-of-Two: Đồng thời kích hoạt cả 3 đỉnh A + B + C
    RuleOfTwoViolation(String),
    /// Vi phạm Data Taint: Đưa dữ liệu ô nhiễm vào sink đột biến mà chưa giải mật
    TaintedDataSinkViolation(String),
    /// Chứng nhận giải mật không khớp hoặc hết hạn
    InvalidDeclassificationReceipt(String),
    /// Token capability đã hết hạn
    TokenExpired(u64),
    /// Token capability đã hết số lần sử dụng
    TokenExhausted,
    /// Token capability bị thu hồi do epoch toàn cục tăng
    TokenEpochRevoked(u64, u64),
    /// Chữ ký mật mã BLAKE3 MAC không hợp lệ
    InvalidSignature,
    /// Principal không có đủ quyền hạn
    UnauthorizedPrincipal(String),
    /// Principal không đủ thẩm quyền hoặc công cụ của Owner đã bị lược bỏ (F21)
    PrincipalPruned(String),
    /// Capability token bị mở rộng trái phép (F22 - CVE-2026-32922)
    PrivilegeEscalationDetected(String),
    /// Toàn bộ quyền bị thu hồi do kích hoạt công tắc khẩn cấp (F25)
    KillSwitchActivated(String),
    /// Hành động bậc R4 bị cấm tuyệt đối đối với agent tự động (F23)
    ForbiddenR4Operation(String),
    /// Bí danh cho Canary tests kiểm tra R4
    R4Forbidden(String),
    /// Bắt buộc phê duyệt HITL đối với hành động bậc R3 (F23)
    HitlApprovalRequired { token: String, argument_hash: String },
}

impl std::fmt::Display for SecurityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MutationForbiddenInReaderMode(m) => write!(f, "SecurityError::MutationForbiddenInReaderMode: {m}"),
            Self::VaultAccessForbiddenInBrowserMode(m) => write!(f, "SecurityError::VaultAccessForbiddenInBrowserMode: {m}"),
            Self::RuleOfTwoViolation(m) => write!(f, "SecurityError::RuleOfTwoViolation: {m}"),
            Self::TaintedDataSinkViolation(m) => write!(f, "SecurityError::TaintedDataSinkViolation: {m}"),
            Self::InvalidDeclassificationReceipt(m) => write!(f, "SecurityError::InvalidDeclassificationReceipt: {m}"),
            Self::TokenExpired(exp) => write!(f, "SecurityError::TokenExpired: {exp}"),
            Self::TokenExhausted => write!(f, "SecurityError::TokenExhausted: maximum usage count reached"),
            Self::TokenEpochRevoked(tok, cur) => write!(f, "SecurityError::TokenEpochRevoked: token epoch {tok} revoked by current {cur}"),
            Self::InvalidSignature => write!(f, "SecurityError::InvalidSignature: cryptographic MAC verification failed"),
            Self::UnauthorizedPrincipal(m) => write!(f, "SecurityError::UnauthorizedPrincipal: {m}"),
            Self::PrincipalPruned(m) => write!(f, "SecurityError::PrincipalPruned: {m}"),
            Self::PrivilegeEscalationDetected(m) => write!(f, "SecurityError::PrivilegeEscalationDetected: {m}"),
            Self::KillSwitchActivated(m) => write!(f, "SecurityError::KillSwitchActivated: {m}"),
            Self::ForbiddenR4Operation(m) => write!(f, "SecurityError::ForbiddenR4Operation: {m}"),
            Self::R4Forbidden(m) => write!(f, "SecurityError::R4Forbidden: {m}"),
            Self::HitlApprovalRequired { token, argument_hash } => {
                write!(f, "SecurityError::HitlApprovalRequired: token={token}, hash={argument_hash}")
            }
        }
    }
}

impl std::error::Error for SecurityError {}

/// Output produced by a completed or interrupted conversational turn.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionTurnOutput {
    pub turn_id: String,
    pub response_text: String,
    pub is_interrupted: bool,
    pub is_steered: bool,
    pub execution_duration_ms: u64,
}

/// Strongly-typed error variants for SessionActor operations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SessionError {
    MailboxFull,
    MailboxClosed,
    QueueFull,
    Interrupted,
    Timeout,
    StateError(String),
    Internal(String),
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MailboxFull => write!(f, "Session mailbox is full; backpressure limit exceeded"),
            Self::MailboxClosed => write!(f, "Session actor mailbox is closed or terminated"),
            Self::QueueFull => write!(f, "Followup turn queue has exceeded maximum capacity"),
            Self::Interrupted => write!(f, "Turn execution was aborted by an interrupt request"),
            Self::Timeout => write!(f, "Operation timed out"),
            Self::StateError(e) => write!(f, "Session state error: {e}"),
            Self::Internal(e) => write!(f, "Internal error: {e}"),
        }
    }
}

impl std::error::Error for SessionError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adaptive_turn_decision_boundaries() {
        // Boundary: 0.0 -> Incomplete
        let d_zero = AdaptiveTurnDecision::from_probability(0.0);
        assert!(
            matches!(d_zero, AdaptiveTurnDecision::Incomplete { probability } if (probability - 0.0).abs() < f32::EPSILON)
        );
        assert!(!d_zero.is_immediate());
        assert_eq!(d_zero.probability(), 0.0);

        // Boundary: 0.49 -> Incomplete
        let d_49 = AdaptiveTurnDecision::from_probability(0.49);
        assert!(
            matches!(d_49, AdaptiveTurnDecision::Incomplete { probability } if (probability - 0.49).abs() < f32::EPSILON)
        );
        assert!(!d_49.is_immediate());
        assert_eq!(d_49.probability(), 0.49);

        // Boundary: 0.50 -> HesitationWait
        let d_50 = AdaptiveTurnDecision::from_probability(0.50);
        assert!(
            matches!(d_50, AdaptiveTurnDecision::HesitationWait { probability } if (probability - 0.50).abs() < f32::EPSILON)
        );
        assert!(!d_50.is_immediate());
        assert_eq!(d_50.probability(), 0.50);

        // Boundary: 0.92 -> HesitationWait
        let d_92 = AdaptiveTurnDecision::from_probability(0.92);
        assert!(
            matches!(d_92, AdaptiveTurnDecision::HesitationWait { probability } if (probability - 0.92).abs() < f32::EPSILON)
        );
        assert!(!d_92.is_immediate());
        assert_eq!(d_92.probability(), 0.92);

        // Boundary: 0.93 -> ImmediateCutoff
        let d_93 = AdaptiveTurnDecision::from_probability(0.93);
        assert!(
            matches!(d_93, AdaptiveTurnDecision::ImmediateCutoff { probability } if (probability - 0.93).abs() < f32::EPSILON)
        );
        assert!(d_93.is_immediate());
        assert_eq!(d_93.probability(), 0.93);

        // Boundary: 1.0 -> ImmediateCutoff
        let d_one = AdaptiveTurnDecision::from_probability(1.0);
        assert!(
            matches!(d_one, AdaptiveTurnDecision::ImmediateCutoff { probability } if (probability - 1.0).abs() < f32::EPSILON)
        );
        assert!(d_one.is_immediate());
        assert_eq!(d_one.probability(), 1.0);
    }

    #[test]
    fn test_turn_verdict_properties_and_serde() {
        let verdict = TurnVerdict {
            probability: 0.88,
            complete: true,
        };
        assert_eq!(verdict.probability, 0.88);
        assert!(verdict.complete);

        let verdict_copy = verdict;
        assert_eq!(verdict, verdict_copy);
        assert_eq!(
            format!("{verdict:?}"),
            "TurnVerdict { probability: 0.88, complete: true }"
        );
    }

    #[test]
    fn test_weather_args_schema_generation() {
        let schema = schemars::schema_for!(WeatherArgs);
        let metadata = schema.schema.metadata.as_ref().expect("schema metadata");
        assert_eq!(metadata.title.as_deref(), Some("WeatherArgs"));

        let object_validation = schema.schema.object.as_ref().expect("object validation");
        assert!(object_validation.properties.contains_key("location"));

        let args = WeatherArgs {
            location: Some("Hà Nội".to_string()),
        };
        assert_eq!(args.location.as_deref(), Some("Hà Nội"));
    }

    struct MockCalculatorTool;

    #[async_trait::async_trait]
    impl ScopedTool for MockCalculatorTool {
        fn definition(&self) -> Tool {
            Tool {
                name: "mock_calc".to_string(),
                description: "Mock Calculator".to_string(),
                input_schema: schemars::schema_for!(serde_json::Value),
            }
        }

        fn default_policy(&self) -> ToolExecPolicy {
            ToolExecPolicy::Auto
        }

        fn required_principal(&self) -> CommandPrincipal {
            CommandPrincipal::LocalCli
        }

        async fn execute(
            &self,
            arguments: serde_json::Value,
            context: ToolExecutionContext,
        ) -> Result<CallToolResult, String> {
            let expr = arguments["expr"].as_str().unwrap_or("0");
            assert_eq!(context.session_id, "test_session");
            Ok(CallToolResult::success(format!("evaluated: {expr}")))
        }
    }

    #[tokio::test]
    async fn test_scoped_tool_execution_and_policies() {
        let tool = MockCalculatorTool;
        assert_eq!(tool.definition().name, "mock_calc");
        assert_eq!(tool.default_policy(), ToolExecPolicy::Auto);
        assert_eq!(tool.required_principal(), CommandPrincipal::LocalCli);

        let ctx = ToolExecutionContext::new(
            "test_session",
            CommandPrincipal::LocalCli,
            std::path::PathBuf::from("."),
        );
        let res = tool
            .execute(serde_json::json!({ "expr": "2 + 2" }), ctx)
            .await
            .expect("tool execution");
        assert!(!res.is_error);
        assert_eq!(
            res.content,
            vec![ToolContent::Text {
                text: "evaluated: 2 + 2".to_string()
            }]
        );

        let scope_global = ToolScope::Global;
        let scope_session = ToolScope::Session("sess_123".to_string());
        let scope_task = ToolScope::Task("task_abc".to_string());
        assert_ne!(scope_global, scope_session);
        assert_ne!(scope_session, scope_task);

        let err_res = CallToolResult::error("fatal syntax error");
        assert!(err_res.is_error);
    }

    #[test]
    fn test_command_principal_monotonic_privilege_ordering() {
        assert_eq!(CommandPrincipal::Telegram.privilege_level(), 1);
        assert_eq!(CommandPrincipal::TauriSetup.privilege_level(), 2);
        assert_eq!(CommandPrincipal::TauriWidget.privilege_level(), 3);
        assert_eq!(CommandPrincipal::TauriDashboard.privilege_level(), 4);
        assert_eq!(CommandPrincipal::Test.privilege_level(), 5);
        assert_eq!(CommandPrincipal::LocalCli.privilege_level(), 5);

        assert!(
            CommandPrincipal::Telegram.privilege_level()
                < CommandPrincipal::TauriSetup.privilege_level()
        );
        assert!(
            CommandPrincipal::TauriSetup.privilege_level()
                < CommandPrincipal::TauriWidget.privilege_level()
        );
        assert!(
            CommandPrincipal::TauriWidget.privilege_level()
                < CommandPrincipal::TauriDashboard.privilege_level()
        );
        assert!(
            CommandPrincipal::TauriDashboard.privilege_level()
                < CommandPrincipal::Test.privilege_level()
        );
        assert!(
            CommandPrincipal::Test.privilege_level()
                <= CommandPrincipal::LocalCli.privilege_level()
        );

        assert!(CommandPrincipal::LocalCli.satisfies(CommandPrincipal::LocalCli));
        assert!(CommandPrincipal::Test.satisfies(CommandPrincipal::LocalCli));
        assert!(CommandPrincipal::LocalCli.satisfies(CommandPrincipal::Telegram));
        assert!(!CommandPrincipal::Telegram.satisfies(CommandPrincipal::LocalCli));
        assert!(!CommandPrincipal::TauriWidget.satisfies(CommandPrincipal::LocalCli));
        assert!(!CommandPrincipal::TauriDashboard.satisfies(CommandPrincipal::LocalCli));
    }

    #[test]
    fn test_tool_content_serde_fallback_unsupported() {
        let text_json = serde_json::json!({ "type": "text", "text": "hello" });
        let text_content: ToolContent = serde_json::from_value(text_json).unwrap();
        assert_eq!(
            text_content,
            ToolContent::Text {
                text: "hello".into()
            }
        );

        let image_json = serde_json::json!({
            "type": "image",
            "data": "base64data",
            "mimeType": "image/png"
        });
        let image_content: ToolContent = serde_json::from_value(image_json).unwrap();
        assert_eq!(
            image_content,
            ToolContent::Image {
                data: "base64data".into(),
                mime_type: "image/png".into()
            }
        );

        // Resource and unknown types must safely fall back to Unsupported
        let resource_json =
            serde_json::json!({ "type": "resource", "resource": { "uri": "file:///test" } });
        let resource_content: ToolContent = serde_json::from_value(resource_json).unwrap();
        assert_eq!(resource_content, ToolContent::Unsupported);

        let unknown_json = serde_json::json!({ "type": "audio", "data": "..." });
        let unknown_content: ToolContent = serde_json::from_value(unknown_json).unwrap();
        assert_eq!(unknown_content, ToolContent::Unsupported);
    }

    #[test]
    fn test_session_queue_modes_and_errors() {
        let modes = vec![
            (SessionQueueMode::Steer, "\"steer\""),
            (SessionQueueMode::Followup, "\"followup\""),
            (SessionQueueMode::Collect, "\"collect\""),
            (SessionQueueMode::Interrupt, "\"interrupt\""),
        ];
        for (mode, json_str) in modes {
            let serialized = serde_json::to_string(&mode).unwrap();
            assert_eq!(serialized, json_str);
            let deserialized: SessionQueueMode = serde_json::from_str(json_str).unwrap();
            assert_eq!(deserialized, mode);
        }

        let err = SessionError::MailboxFull;
        assert_eq!(format!("{err}"), "Session mailbox is full; backpressure limit exceeded");
        let err2 = SessionError::Interrupted;
        assert_eq!(format!("{err2}"), "Turn execution was aborted by an interrupt request");
    }

    #[test]
    fn test_rule_of_two_vertices_validation() {
        // 2 vertices active: valid
        let r1 = RuleOfTwoVertices {
            untrusted_input: true,
            sensitive_data: true,
            external_mutation: false,
        };
        assert!(r1.is_valid());
        assert!(r1.validate().is_ok());

        let r2 = RuleOfTwoVertices {
            untrusted_input: false,
            sensitive_data: true,
            external_mutation: true,
        };
        assert!(r2.is_valid());
        assert!(r2.validate().is_ok());

        // 3 vertices active: invalid (RuleOfTwoViolation)
        let r_all = RuleOfTwoVertices {
            untrusted_input: true,
            sensitive_data: true,
            external_mutation: true,
        };
        assert!(!r_all.is_valid());
        let res = r_all.validate();
        assert!(matches!(res, Err(SecurityError::RuleOfTwoViolation(_))));
    }

    #[test]
    fn test_principal_and_authority_tiers() {
        let owner = Principal::Owner;
        assert_eq!(owner.authority_tier(), AuthorityTier::Owner);
        assert!(owner.is_owner());

        let contact = Principal::KnownContact {
            contact_id: "c1".into(),
            display_name: "Alice".into(),
            channel: "telegram".into(),
            identifier: "123".into(),
            trust_level: ContactTrustLevel::Family,
        };
        assert_eq!(contact.authority_tier(), AuthorityTier::KnownContact);
        assert!(!contact.is_owner());

        let untrusted = Principal::UntrustedExternal {
            channel_id: "telegram".into(),
            sender_address: "stranger".into(),
            is_public_channel: false,
        };
        assert_eq!(untrusted.authority_tier(), AuthorityTier::Untrusted);
        assert!(!untrusted.is_owner());

        // From CommandPrincipal
        assert_eq!(Principal::from(CommandPrincipal::LocalCli), Principal::Owner);
        assert_eq!(Principal::from(CommandPrincipal::Test), Principal::Owner);
        assert!(matches!(
            Principal::from(CommandPrincipal::Telegram),
            Principal::UntrustedExternal { .. }
        ));
    }

    #[test]
    fn test_labeled_taint_propagation_and_declassification() {
        let untrusted = Labeled::new_untrusted(
            "dangerous payload".to_string(),
            DataOrigin::WebScrape {
                url: "https://evil.example".into(),
                domain: "evil.example".into(),
                fetched_at: 1000,
            },
        );
        assert!(untrusted.is_tainted());
        assert_eq!(untrusted.integrity(), IntegrityLevel::Untrusted);

        // Map preserves taint
        let transformed = untrusted.map(|s| s.to_uppercase());
        assert_eq!(transformed.as_inner(), "DANGEROUS PAYLOAD");
        assert!(transformed.is_tainted());

        // Combine combines taints
        let owner_data = Labeled::new_owner("my_secret".to_string(), "local");
        assert!(!owner_data.is_tainted());

        let combined = owner_data.combine(transformed, |a, b| format!("{a}:{b}"));
        assert!(combined.is_tainted());
        assert_eq!(combined.as_inner(), "my_secret:DANGEROUS PAYLOAD");

        // Declassification with expired receipt fails
        let expired_receipt = DeclassificationReceipt {
            receipt_id: "r1".into(),
            authorized_by: "OwnerHITL".into(),
            payload_blake3_hash: "hash".into(),
            issued_at_ms: 100,
            expires_at_ms: 200, // expired long ago
        };
        assert!(combined.declassify(&expired_receipt).is_err());
    }

    #[test]
    fn test_security_error_display_and_canary_matching() {
        let r4_err = SecurityError::ForbiddenR4Operation("cannot format drive".into());
        assert!(format!("{r4_err}").contains("ForbiddenR4Operation"));

        let atten_err = SecurityError::PrivilegeEscalationDetected("widened scope".into());
        assert!(format!("{atten_err}").contains("PrivilegeEscalationDetected"));

        let reader_err = SecurityError::MutationForbiddenInReaderMode("send_message".into());
        assert!(format!("{reader_err}").contains("MutationForbiddenInReaderMode"));

        let kill_err = SecurityError::KillSwitchActivated("emergency stop".into());
        assert!(format!("{kill_err}").contains("KillSwitchActivated"));
    }
}

