/**
 * DependencyContainer — Sprint 3: Lightweight DI holder for CoreKernel sub-systems.
 *
 * This is NOT a full IoC container (InversifyJS overkill).
 * It's a typed struct that holds references to all shared services
 * so that BootstrapManager, RemoteControlHub, and EventPipeline
 * can access them without importing CoreKernel directly.
 */

import type { UIController } from "./UIController";
import type { AgentLoop } from "./AgentLoop";
import type { MemoryManager } from "../MemoryManager";
import type { SkillRegistry } from "../SkillRegistry";
import type { ZaloPolling } from "./ZaloPolling";
import type { VoiceEngine } from "../services/VoiceEngine";
import type { KokoroVoiceEngine } from "../services/KokoroVoiceEngine";
import type { WhisperNode } from "../services/WhisperNode";
import type { WhisperJSNode } from "../services/WhisperJSNode";
import type { SmartTurnVAD } from "../services/SmartTurnVAD";
import type { HeartbeatManager } from "./HeartbeatManager";
import type { AppWatcherService } from "../services/AppWatcherService";
import type { TelegramBridge } from "../channels/TelegramBridge";
import type { MetaBridge } from "../channels/MetaBridge";
import type { ChannelRouter } from "../channels/ChannelNormalizer";
import type { CDPBridge } from "../bridges/CDPBridge";
import type { ApprovalEngine } from "./ApprovalEngine";
import type { SecurityGateway } from "../security/SecurityGateway";
import type { AutoAcceptDaemon } from "../security/AutoAcceptDaemon";
import type { VSCodeBridge } from "../bridges/VSCodeBridge";
import type { SessionOrchestrator } from "./SessionOrchestrator";
import type { NLCommandTranslator } from "./NLCommandTranslator";
import type { EmailClientManager } from "../services/EmailClientManager";
import type { GitNexusIndexer } from "../evolution/GitNexusIndexer";

/**
 * Shared dependency bag passed to all sub-managers.
 * All fields are mutable references (same instances as CoreKernel public properties).
 */
export interface DependencyContainer {
    // Base Components
    memory: MemoryManager;
    registry: SkillRegistry;
    ui: UIController;
    agentLoop: AgentLoop;
    zalo: ZaloPolling;
    voiceEngine: VoiceEngine | KokoroVoiceEngine;
    whisperNode: WhisperNode | WhisperJSNode;
    smartTurnVAD: SmartTurnVAD | null;
    heartbeat: HeartbeatManager;
    appWatcher: AppWatcherService;

    // Remote Control Hub Components
    telegram: TelegramBridge;
    meta: MetaBridge;
    cdpBridge: CDPBridge;
    approvalEngine: ApprovalEngine;
    channelRouter: ChannelRouter;
    securityGateway: SecurityGateway;
    autoAcceptDaemon: AutoAcceptDaemon;

    // Phase 2 Components
    vscodeBridge: VSCodeBridge;
    sessions: SessionOrchestrator;
    nlTranslator: NLCommandTranslator;
    emailManager: EmailClientManager;
    gitNexusIndexer: GitNexusIndexer;

    // Internal accessors (CoreKernel exposes these via closures)
    dispatch: (id: string, payload: unknown) => Promise<void>;
    addTelemetryLog: (level: string, message: string) => void;
    getDefaultRemoteSenderId: () => string;
}
