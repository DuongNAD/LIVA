/**
 * liva-common — Shared Types & Contracts for LIVA System
 * =======================================================
 * Single Source of Truth (SSOT) for data structures exchanged
 * between liva-gateway (Backend) and liva-ui (Frontend).
 */

// Config types (liva-config.json shape)
export type {
    LivaConfig,
    AvatarConfig,
    AvatarModelInfo,
    AIConfig,
    AIProvider,
    VoiceConfig,
    VoiceProvider,
    VoiceProfile,
    UIConfig,
    SystemConfig,
    SystemStatus,
    SkillInfo,
    TaskItem,
    EngineMode,
    AvatarFormat,
} from './types/config.js';

// WebSocket event contract
export type {
    WSClientEvent,
    WSServerEvent,
    WSMessage,
    TaskPlanReplyPayload,
    GPUSetupPayload,
    AvatarModelsPayload,
    EnvConfigPayload,
} from './types/websocket.js';
