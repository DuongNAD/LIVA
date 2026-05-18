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
    cloudApiKey: string;
    cloudModel: string;
    localModelsDir: string;
    routerModel: string;
    expertModel: string;
    temperature: number;
    maxTokens: number;
    topP: number;
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
