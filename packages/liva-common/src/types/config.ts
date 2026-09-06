/**
 * liva-common/src/types/config.ts — Shared Configuration Types (SSOT)
 * ====================================================================
 * Derived from UIController.#getDefaultConfig() and all Dashboard component usage.
 * Both liva-gateway and liva-ui import these types to enforce compile-time safety.
 */

// ─── Avatar Configuration ───
export type EngineMode = 'auto' | '2D' | '3D';
export type AvatarFormat = 'vrm' | 'fbx' | 'live2d';

export interface AvatarConfig {
    engineMode: EngineMode;
    activeType?: '2d' | '3d';
    live2dModel: string;
    vrmModel: string;
    autoBlinkEnabled: boolean;
    lookAtMouseEnabled: boolean;
    lipSyncEnabled: boolean;
}

export interface AvatarModelInfo {
    name: string;
    filename: string;
    size: string;
    type: '2d' | '3d';
    format: AvatarFormat;
    isActive: boolean;
    hasTextureDir?: boolean;
}

// ─── AI Provider Configuration ───
export type AIProvider = 'local' | 'cloud' | 'hybrid';

export interface AIConfig {
    provider: AIProvider;
    cloudBaseUrl: string;
    cloudModel: string;
    localModelsDir: string;
    routerModel: string;
    expertModel: string;
    temperature: number;
    maxTokens: number;
    topP: number;
}

export interface IntegrationConfig {
    whisperCloudEnabled: boolean;
    whisperCloudUrl: string;
    tavilyEnabled: boolean;
    weatherEnabled: boolean;
    telegramEnabled: boolean;
    telegramAllowedIds: string;
    zaloEnabled: boolean;
    zaloAppId: string;
    zaloUserId: string;
    emailEnabled: boolean;
    emailHost: string;
    emailPort: string;
    emailUser: string;
    googleEnabled: boolean;
}

// ─── Voice Configuration ───
export type VoiceProvider = 'hybrid' | 'python' | 'kokoro';

export interface VoiceConfig {
    enabled: boolean;
    provider: VoiceProvider;
    activeProfile: string;
    trainingEnabled: boolean;
    sampleRate: number;
    language: string;
}

export interface VoiceProfile {
    id: string;
    name: string;
    description?: string;
    language: string;
    isActive: boolean;
}

// ─── UI Configuration ───
export interface UIConfig {
    widgetPosition: string;
    dashboardTheme: string;
    avatarMode?: EngineMode;
    activeModel?: { filename: string };
}

// ─── System / Digest Configuration ───
export interface SystemConfig {
    geolocationEnabled: boolean;

    digestInterestsEnabled: boolean;
    digestInterestsHour: number;
    digestInterestsMinute: number;
    digestInterestsDeliverUI: boolean;
    digestInterestsDeliverTelegram: boolean;
    digestInterestsDeliverZalo: boolean;
    digestInterestsDeliverEmail: boolean;

    digestFocusEnabled: boolean;
    digestFocusHour: number;
    digestFocusMinute: number;
    digestFocusDeliverUI: boolean;
    digestFocusDeliverTelegram: boolean;
    digestFocusDeliverZalo: boolean;
    digestFocusDeliverEmail: boolean;
    digestFocusTopics: string;
}

// ─── Top-Level Config (liva-config.json) ───
export interface LivaConfig {
    avatar: AvatarConfig;
    ai: AIConfig;
    voice: VoiceConfig;
    ui: UIConfig;
    system: SystemConfig;
    integrations?: IntegrationConfig;
}

// ─── System Status (from CoreKernel health check) ───
export interface SystemStatus {
    model?: string;
    provider?: string;
    latencyMs?: number;
    uptime?: number;
    memoryUsage?: number;
    cpuUsage?: number;
    gpuVram?: number;
    engineStatus?: string;
    /**
     * Số đo hệ thống THẬT (`sysinfo.rs` + `governor.rs`).
     *
     * Mọi trường đều `| null` chứ không phải tuỳ chọn-rồi-mặc-định-0: lõi trả
     * `null` khi **không đo được** (không có NVIDIA, không phải Windows, hoặc
     * lần lấy mẫu đầu chưa có mốc để so). Giao diện phải hiện `--`, không được
     * lấp bằng 0 — đó là quy ước `None là câu trả lời hợp lệ` của `sysinfo.rs`.
     */
    osStats?: {
        /** Tải CPU của các tiến trình **ngoài** LIVA, %. */
        cpuUsage?: number | null;
        /** Phần CPU của **chính LIVA**, cùng mẫu số với `cpuUsage`. */
        livaCpuUsage?: number | null;
        gpuUsage?: number | null;
        totalMemory?: number | null;
        freeMemory?: number | null;
    };
    rssMemory?: number | null;
    /**
     * Bảng sức khoẻ từng hệ. **Chỉ khai phần giao diện thật sự đọc** — object
     * gốc còn nhiều khoá khác (`orchestrator`, `voiceEngine`, `whisper`,
     * `remoteControl`…). Khai thiếu còn hơn khai sai: một kiểu đầy đủ nhưng
     * trôi khỏi lõi chính là thứ đã xảy ra với `SystemStatus` trước đây.
     */
    healthChecks?: {
        /** `isYielded` = governor đang hạ ưu tiên để nhường máy. */
        vramGuard?: { isYielded?: boolean };
    };
}

// ─── Skill Metadata (from SkillRegistry) ───
export interface SkillInfo {
    name: string;
    description: string;
    parameters: Record<string, unknown>;
    search_keywords?: string[];
    isCoreSkill?: boolean;
    requiresApproval?: boolean;
    enabled?: boolean;
}

// ─── Task (from TaskManager) ───
export interface TaskItem {
    id: string;
    title: string;
    description?: string;
    status: 'pending' | 'in-progress' | 'done';
    priority?: 'low' | 'medium' | 'high';
    createdAt?: number;
    updatedAt?: number;
}

// ─── Multi-Channel Management (Milestone 2) ───
export interface ChannelCapabilities {
    streaming_text: boolean;
    binary_attachments: boolean;
    voice_notes: boolean;
    interactive_buttons: boolean;
    typing_indicator: boolean;
    thread_replies: boolean;
}

export type ChannelConnectionStatus = 'connected' | 'disconnected' | 'reconnecting' | 'failed' | 'standby';

export interface ChannelItem {
    id: string;
    name: string;
    channel_type: string;
    status: { status: ChannelConnectionStatus; attempt?: number; next_retry_ms?: number; error?: string } | ChannelConnectionStatus;
    enabled: boolean;
    capabilities: ChannelCapabilities;
    last_seen_unix: number;
    message_count: number;
    config_summary: Record<string, string>;
}

// ─── Node Pairing Monitor (Milestone 2) ───
export interface PairedNodeInfo {
    nodeId: string;
    nodeName: string;
    role: 'desktop_ui' | 'widget' | 'mobile_companion' | 'headless_node' | 'cli_tool';
    publicKey: string;
    approvedAtUnix: number;
    lastSeenUnix: number;
    deviceType: 'mobile' | 'desktop' | 'server' | 'terminal' | 'widget';
}

export interface PendingPairingChallenge {
    challengeId: string;
    shortCode: string;
    nonce: string;
    nodeId: string;
    nodeName: string;
    role: string;
    publicKey: string;
    createdAtUnix: number;
    expiresAtUnix: number;
    ttlRemainingSeconds: number;
}

// ─── Browser Automation (Milestone 2) ───
export interface BrowserStatus {
    isRunning: boolean;
    isPaused: boolean;
    currentUrl: string;
    pageTitle: string;
    httpStatus: number;
    viewportWidth: number;
    viewportHeight: number;
    sandboxActive: boolean;
    ssrfGuard: boolean;
}

export interface BrowserActionRecord {
    id: string;
    timestamp_unix: number;
    action: string;
    target: string;
    status: string;
    details: string;
}

// ─── Skill Manifest & ClawHub (Milestone 2) ───
export interface SkillManifestInfo {
    skillId: string;
    name: string;
    version: string;
    description: string;
    author?: string;
    license?: string;
    triggers?: Array<{ type: string; config?: unknown }>;
    permissions?: Array<{ type: string; config?: unknown }>;
    tools?: Array<{ name: string; description: string; input_schema?: unknown; risk_level?: string }>;
    runtimeType?: string;
    markdownInstructions: string;
    rawContent: string;
    contentHash: string;
    dirPath: string;
}

export interface SkillLogEntry {
    id: string;
    skillId: string;
    timestampUnix: number;
    caller: string;
    status: 'SUCCESS' | 'ERROR';
    durationMs: number;
    input?: unknown;
    output?: unknown;
    error?: string;
}

