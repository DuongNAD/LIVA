import * as path from "node:path";
import * as fsp from "node:fs/promises";
import { logger } from "../utils/logger";

import { MemoryManager } from "../MemoryManager";
import { SkillRegistry } from "../SkillRegistry";
import { UIController } from "./UIController";
import { AgentLoop } from "./AgentLoop";
import { ZaloPolling } from "./ZaloPolling";
import { PowerMonitorService } from "../services/PowerMonitorService";
import { VoiceOrchestrator } from "./orchestrators/VoiceOrchestrator";
import { Scheduler } from "../kernel/Scheduler";
import { HeartbeatManager } from "./HeartbeatManager";
import { AppWatcherService } from "../services/AppWatcherService";

import { TelegramBridge } from "../channels/TelegramBridge";
import { MetaBridge } from "../channels/MetaBridge";
import { CDPBridge } from "../bridges/CDPBridge";
import { ApprovalEngine } from "./ApprovalEngine";
import { ChannelRouter } from "../channels/ChannelNormalizer";
import { SecurityGateway } from "../security/SecurityGateway";
import { AutoAcceptDaemon } from "../security/AutoAcceptDaemon";

import { VSCodeBridge } from "../bridges/VSCodeBridge";
import { SessionOrchestrator } from "./SessionOrchestrator";
import { NLCommandTranslator } from "./NLCommandTranslator";
import { EmailClientManager } from "../services/EmailClientManager";
import { AutoReplyManager } from "../services/AutoReplyManager";
import { GitNexusIndexer } from "../evolution/GitNexusIndexer";
import { ProactiveDaemon } from "../services/ProactiveDaemon";
import { PresenceDetector } from "../services/PresenceDetector";

import { KernelDI } from "./kernel/KernelDI";
import { KernelLifecycle } from "./kernel/KernelLifecycle";
import { KernelEventRouter } from "./kernel/KernelEventRouter";
import { PluginSkillOrchestrator } from "./kernel/PluginSkillOrchestrator";

/**
 * @type_level_programming
 * HYPER-TYPED BRANDING SYSTEM (Structural Identity via Interface Merging)
 */
export type Brand<T, F> = T & { readonly __brand_identity: F };

/**
 * @evolution_target
 * AUTHORITY TOKEN: KernelAuthority
 */
export type KernelAuthority = boolean & Brand<boolean, "CORE_KERNEL_SIGNED">;

/**
 * @evolution_target
 * COMMAND TOKEN: CommandToken<T, Status>
 * Evolution: Includes TTL (Time-To-Live) for Garbage Collection.
 */
export type CommandToken<T extends string, Status extends string> = {
  readonly __id: T;
  readonly __authority: KernelAuthority;
  readonly __expiresAt: number;
} & Brand<{ __id: T }, Status>;

/**
 * @evolution_target
 * TRANSITION SCHEMA (Strict Authority Requirement)
 */
export interface TransitionSchema<T extends string, Status extends string> {
  readonly token: CommandToken<T, Status>;
  readonly execute: (payload: unknown, isDryRun?: boolean) => Promise<void>;
}

/**
 * @tensor_logic
 * DEFINITION: ReactiveStateTensor
 */
export interface ReactiveStateTensor {
  readonly dimensions: number[];
  getWeight(latencyMs: number): number;
  updateWeights(feedbackLoop: number[]): void;
}

export class CoreKernel {
  public memory!: MemoryManager;
  public registry!: SkillRegistry;
  public voiceMode: "IDLE" | "ACTIVE" = "IDLE";
  public ui!: UIController;
  public agentLoop!: AgentLoop;
  public zalo!: ZaloPolling;
  public powerMonitor!: PowerMonitorService;

  // Decoupled via DependencyContainer
  public voiceOrchestrator!: VoiceOrchestrator;
  public get voiceEngine() {
    return this.voiceOrchestrator.voiceEngine;
  }
  public set voiceEngine(v) {
    if (this.voiceOrchestrator.voiceEngine) {
      this.voiceOrchestrator.voiceEngine.off("audio_buffer", this.onVoiceEngineAudioBuffer);
    }
    this.voiceOrchestrator.voiceEngine = v;
    if (v) {
      v.on("audio_buffer", this.onVoiceEngineAudioBuffer);
    }
  }
  public get whisperNode() {
    return this.voiceOrchestrator.whisperNode;
  }
  public get vadBridge() {
    return this.voiceOrchestrator.vadBridge;
  }
  public set vadBridge(v) {
    this.voiceOrchestrator.vadBridge = v;
  }

  // [v26] AIOS Kernel Scheduler
  public scheduler!: Scheduler;

  public heartbeat!: HeartbeatManager;
  public appWatcher!: AppWatcherService;

  // [v5.0] Remote Control Hub Components
  public telegram!: TelegramBridge;
  public meta!: MetaBridge;
  public cdpBridge!: CDPBridge;
  public approvalEngine!: ApprovalEngine;
  public channelRouter!: ChannelRouter;
  public securityGateway!: SecurityGateway;
  public autoAcceptDaemon!: AutoAcceptDaemon;

  // [v5.0] Phase 2 Components
  public vscodeBridge!: VSCodeBridge;
  public sessions!: SessionOrchestrator;
  public nlTranslator!: NLCommandTranslator;
  public emailManager!: EmailClientManager;
  public autoReply!: AutoReplyManager;
  public gitNexusIndexer!: GitNexusIndexer;
  public proactiveInterestsDaemon: ProactiveDaemon | null = null;
  public proactiveFocusDaemon: ProactiveDaemon | null = null;
  public presenceDetector!: PresenceDetector;
  public presence: "ACTIVE" | "AWAY" = "ACTIVE";

  // Facade fields and sub-modules
  public eventRouter!: KernelEventRouter;
  public skillOrchestrator!: PluginSkillOrchestrator;
  public transitionSchema!: Map<string, TransitionSchema<string, string>>;
  public orchestrationTensor!: ReactiveStateTensor;
  public isTtsFallbackActive: boolean = false;
  public currentLatency: number = 0;
  public gcIntervalId: NodeJS.Timeout | null = null;
  public latestCameraFrame: string | null = null;
  public cachedStaticStats: Record<string, unknown> | null = null;
  public telemetryLogs: { time: number; level: string; message: string }[] = [];
  public isVramYielded = false;
  public idleTimeout: NodeJS.Timeout | null = null;

  public readonly DEFAULT_TTL = 60000; // 60 seconds default

  public onVoiceEngineAudioBuffer = (buffer: Buffer) => {
    if (this.presence === "AWAY") return;
    this.ui.broadcastTTSAudio(buffer);
  };

  public getIdleTimeout() {
    return this.idleTimeout;
  }
  public setIdleTimeout(t: NodeJS.Timeout | null) {
    this.idleTimeout = t;
  }
  public getGcIntervalId() {
    return this.gcIntervalId;
  }
  public setGcIntervalId(id: NodeJS.Timeout | null) {
    this.gcIntervalId = id;
  }
  public getTransitionSchema() {
    return this.transitionSchema;
  }
  public setTransitionSchema(schema: Map<string, TransitionSchema<string, string>>) {
    this.transitionSchema = schema;
  }
  public getOrchestrationTensor() {
    return this.orchestrationTensor;
  }
  public setOrchestrationTensor(tensor: ReactiveStateTensor) {
    this.orchestrationTensor = tensor;
  }

  public addTelemetryLog(level: string, message: string) {
    this.telemetryLogs.unshift({ time: Date.now(), level, message });
    if (this.telemetryLogs.length > 50) this.telemetryLogs.pop();
  }

  public mintCommandToken<T extends string, Status extends string>(id: T, ttl: number = this.DEFAULT_TTL): CommandToken<T, Status> {
    return {
      __id: id,
      __authority: true as unknown as KernelAuthority,
      __expiresAt: Date.now() + ttl,
      __brand_identity: "" as unknown as CommandToken<T, Status>["__brand_identity"],
    } as unknown as CommandToken<T, Status>;
  }

  public registerAuthorityTransition<T extends string, Status extends string>(
    id: string,
    schema: TransitionSchema<T, Status>
  ) {
    this.transitionSchema.set(id, schema);
  }

  public getDefaultRemoteSenderId(): string {
    const ids = (process.env.TELEGRAM_ALLOWED_IDS || "")
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);
    return ids[0] || "";
  }

  public async dispatch(id: string, payload: unknown, isDryRun?: boolean) {
    const transition = this.transitionSchema.get(id);
    if (transition) {
      if (transition.token.__authority && transition.token.__expiresAt > Date.now()) {
        await transition.execute(payload, isDryRun);
      } else if (transition.token.__expiresAt <= Date.now()) {
        logger.error(`❌ [Authority Violation] Token for command: ${id} has expired.`);
      } else {
        logger.error(`❌ [Authority Violation] Forged token detected for command: ${id}`);
      }
    } else {
      logger.error(`❌ [Authority Violation] Attempted to dispatch unregistered handle: ${id}`);
    }
  }

  public async loadAIConfig(): Promise<Record<string, unknown>> {
    try {
      const p = path.join(process.cwd(), "..", "data", "liva-config.json");
      const data = await fsp.readFile(p, "utf8");
      return JSON.parse(data).ai || {};
    } catch {
      return {};
    }
  }

  public async mergeAIConfig(payload: Record<string, unknown>): Promise<Record<string, unknown>> {
    const ai = await this.loadAIConfig();
    return { ...ai, ...(payload.ai || payload) };
  }

  public async persistConfigPatch(patch: unknown): Promise<void> {
    try {
      const p = path.join(process.cwd(), "..", "data", "liva-config.json");
      const config = JSON.parse(await fsp.readFile(p, "utf8"));
      Object.assign(config, patch);
      const tmpPath = `${p}.tmp`;
      await fsp.writeFile(tmpPath, JSON.stringify(config, null, 2), "utf8");

      const { safeRename } = await import("../utils/FileUtils");
      await safeRename(tmpPath, p);
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      logger.error({ err: errMsg }, "[CoreKernel] Failed to persist config patch");
    }
  }

  public async loadVoiceConfig(): Promise<Record<string, unknown>> {
    try {
      const p = path.join(process.cwd(), "..", "data", "liva-config.json");
      const data = await fsp.readFile(p, "utf8");
      return JSON.parse(data).voice || {};
    } catch {
      return {};
    }
  }

  public async getVoiceStatus(): Promise<Record<string, unknown>> {
    return this.loadVoiceConfig();
  }

  public getVoiceProfiles(): Record<string, unknown>[] {
    return [
      {
        id: "vi-VN-HoaiMyNeural",
        name: "Hoài My (Vietnamese)",
        lang: "vi-VN",
        description: "Giọng nữ Việt Nam — Thân thiện, Tích cực",
        gender: "Female",
      },
      {
        id: "en-US-AvaMultilingualNeural",
        name: "Ava (US Multilingual)",
        lang: "en-US",
        description: "Expressive, Caring, Pleasant — Đa ngôn ngữ",
        gender: "Female",
      },
      {
        id: "en-US-AriaNeural",
        name: "Aria (US News)",
        lang: "en-US",
        description: "Positive, Confident — Chuyên nghiệp",
        gender: "Female",
      },
      {
        id: "en-US-JennyNeural",
        name: "Jenny (US General)",
        lang: "en-US",
        description: "Friendly, Considerate — Đa năng",
        gender: "Female",
      },
      {
        id: "ja-JP-NanamiNeural",
        name: "Nanami (Japanese)",
        lang: "ja-JP",
        description: "Giọng nữ Nhật Bản — Thân thiện",
        gender: "Female",
      },
      {
        id: "ko-KR-SunHiNeural",
        name: "SunHi (Korean)",
        lang: "ko-KR",
        description: "Giọng nữ Hàn Quốc — Tích cực",
        gender: "Female",
      },
      {
        id: "zh-CN-XiaoxiaoNeural",
        name: "Xiaoxiao (Chinese)",
        lang: "zh-CN",
        description: "Giọng nữ Trung Quốc — Ấm áp",
        gender: "Female",
      },
    ];
  }

  constructor() {
    KernelDI.wire(this);
    this.eventRouter = new KernelEventRouter(this);
    this.eventRouter.registerAll();

    // Register authority transitions
    this.registerAuthorityTransition<"ui_broadcast", "ACTIVE">("ui_broadcast", {
      token: this.mintCommandToken<"ui_broadcast", "ACTIVE">("ui_broadcast", 99999999999),
      execute: async (payload: unknown) => {
        const event = payload as { name: string; data?: Record<string, unknown> };
        await this.ui.broadcastUIEvent(event.name, event.data);
      },
    });

    this.registerAuthorityTransition<"agent_input", "ACTIVE">("agent_input", {
      token: this.mintCommandToken<"agent_input", "ACTIVE">("agent_input", 99999999999),
      execute: async (payload: unknown, isDryRun?: boolean) => {
        const text = payload as string;
        await this.agentLoop.handleUserInput(text, false, false, isDryRun);
      },
    });

    this.skillOrchestrator.startWatcher();
    this.gcIntervalId = KernelLifecycle.startGarbageCollection(this);
  }

  public async bootstrap(): Promise<void> {
    await KernelLifecycle.bootstrap(this);
  }

  public async shutdown(): Promise<void> {
    await KernelLifecycle.shutdown(this);
  }

  public async fetchSystemLocation(): Promise<unknown> {
    return await KernelLifecycle.fetchSystemLocation(this);
  }

  public async yieldVRAM(): Promise<void> {
    await KernelLifecycle.yieldVRAM(this);
  }

  public async reclaimVRAM(): Promise<void> {
    await KernelLifecycle.reclaimVRAM(this);
  }
}