import OpenAI from "openai";
import { UIController } from "./UIController";
import { AgentLoop } from "./AgentLoop";
import { MemoryManager } from "../MemoryManager";
import { SkillRegistry } from "../SkillRegistry";
import { ZaloPolling } from "./ZaloPolling";
import { VoiceEngine } from "../services/VoiceEngine";
import { KokoroVoiceEngine } from "../services/KokoroVoiceEngine";
import { IVoiceEngine } from "../services/IVoiceEngine";
import { WhisperNode } from "../services/WhisperNode";
import { SmartTurnVAD } from "../services/SmartTurnVAD";
import { VADWorkerBridge } from "../services/VADWorkerBridge";
import { EmbeddingService } from "../services/EmbeddingService";
import { SensoryManager } from "../memory/SensoryManager";
import { safeFetch } from "../utils/HttpClient";
import { logger } from "../utils/logger";
import { HeraCompass } from "../memory/HeraCompass";
import { HeartbeatManager } from "./HeartbeatManager";
import { AppWatcherService } from "../services/AppWatcherService";

// [v5.0] Remote Control Hub — Phase 1 & 3 Imports
import { TelegramBridge } from "../channels/TelegramBridge";
import { MetaBridge } from "../channels/MetaBridge";
import { ChannelRouter } from "../channels/ChannelNormalizer";
import type { NormalizedMessage } from "../channels/ChannelNormalizer";
import { CDPBridge } from "../bridges/CDPBridge";
import { ApprovalEngine } from "./ApprovalEngine";
import { SecurityGateway } from "../security/SecurityGateway";
import { AutoAcceptDaemon } from "../security/AutoAcceptDaemon";

// [v5.0] Remote Control Hub — Phase 2 Imports
import { VSCodeBridge } from "../bridges/VSCodeBridge";
import { SessionOrchestrator } from "./SessionOrchestrator";
import { NLCommandTranslator } from "./NLCommandTranslator";
import { EmailClientManager } from "../services/EmailClientManager";
import { GitNexusIndexer } from "../evolution/GitNexusIndexer";
import { ProactiveDaemon } from "../services/ProactiveDaemon";
import { VRAMGuard } from "../services/VRAMGuard";
import type { ChatCompletionResponse as NativeIPCChatResponse } from "../utils/NativeIPCClient";

// [Phase 3] Extracted reactive wiring module
import { wireReactiveSync } from "./events/ReactiveSync";

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
interface TransitionSchema<T extends string, Status extends string> {
  readonly token: CommandToken<T, Status>;
  readonly execute: (payload: any) => Promise<void>;
}

/**
 * @tensor_logic
 * DEFINITION: ReactiveStateTensor
 */
interface ReactiveStateTensor {
  readonly dimensions: number[];
  getWeight(latencyMs: number): number;
  updateWeights(feedbackLoop: number[]): void;
}

/**
 * @evolution_target
 * CLASS: CoreKernel (The Hyper-Typed Integrity Fabric)
 */
export class CoreKernel {
  // Base Components
  public memory: MemoryManager;
  public registry: SkillRegistry;
  public ui: UIController;
  public agentLoop: AgentLoop;
  public zalo: ZaloPolling;
  public voiceEngine: IVoiceEngine | null = null;
  public whisperNode: WhisperNode;
  public smartTurnVAD: SmartTurnVAD | null = null;
  public vadBridge: VADWorkerBridge | null = null;
  public heartbeat: HeartbeatManager;
  public appWatcher: AppWatcherService;

  // [v5.0] Remote Control Hub Components
  public telegram: TelegramBridge;
  public meta: MetaBridge;
  public cdpBridge: CDPBridge;
  public approvalEngine: ApprovalEngine;
  public channelRouter: ChannelRouter;
  public securityGateway: SecurityGateway;
  public autoAcceptDaemon: AutoAcceptDaemon;

  // [v5.0] Phase 2 Components
  public vscodeBridge: VSCodeBridge;
  public sessions: SessionOrchestrator;
  public nlTranslator: NLCommandTranslator;
  public emailManager: EmailClientManager;
  public gitNexusIndexer: GitNexusIndexer;
  public proactiveInterestsDaemon: ProactiveDaemon | null = null;
  public proactiveFocusDaemon: ProactiveDaemon | null = null;
  // [v24] Pillar 1: Preemptive VRAM Yielding
  public vramGuard: VRAMGuard;

  // Hard Private Members (Opague Engine Isolation via #)
  #orchestrationTensor: ReactiveStateTensor;
  #isTtsFallbackActive: boolean = false;
  /** @evolution_target O(1) Dispatch Map */
  #transitionSchema: Map<string, TransitionSchema<any, any>>;
  #currentLatency: number = 0;
  /** @evolution_target Garbage Collection Interval */
  #gcIntervalId: NodeJS.Timeout | null = null;
  // 🔒 [Memory Fix #3] Lưu handle FileWatcher để close() khi shutdown (tránh rò rỉ fs handle)
  #fileWatcher: ReturnType<typeof import('fs').watch> | null = null;
  // 👁️ [Camera Vision] Latest webcam frame (base64 JPEG) for AI multimodal
  #latestCameraFrame: string | null = null;
  readonly DEFAULT_TTL = 60000; // 60 seconds default

  /** @evolution_target Telemetry Health Logs */
  private telemetryLogs: { time: number; level: string; message: string }[] = [];

  private addTelemetryLog(level: string, message: string) {
      this.telemetryLogs.unshift({ time: Date.now(), level, message });
      if (this.telemetryLogs.length > 50) this.telemetryLogs.pop();
  }

  /**
   * @private_factory
   * Mints non-forgeable branded handles with TTL.
   */
  #mintCommandToken<T extends string, Status extends string>(id: T, ttl: number = this.DEFAULT_TTL): CommandToken<T, Status> {
    return {
      __id: id,
      __authority: true as unknown as KernelAuthority,
      __expiresAt: Date.now() + ttl,
      __brand_identity: "" as unknown as CommandToken<T, Status>['__brand_identity']
    } as unknown as CommandToken<T, Status>;
  }

  constructor() {
    this.memory = new MemoryManager("liv_async_core");
    this.registry = new SkillRegistry();
    this.ui = new UIController(8082);
    this.agentLoop = new AgentLoop(this.memory, this.registry);
    this.heartbeat = new HeartbeatManager(this.agentLoop);
    this.zalo = new ZaloPolling();
    this.appWatcher = new AppWatcherService(this.memory);

    // [v5.0] Remote Control Hub — Initialize
    this.telegram = new TelegramBridge();
    this.meta = new MetaBridge(Number(process.env.META_WEBHOOK_PORT) || 3000);
    this.cdpBridge = new CDPBridge(
        process.env.CDP_HOST || "127.0.0.1",
        Number(process.env.CDP_PORT) || 9222
    );
    this.autoAcceptDaemon = new AutoAcceptDaemon(this.cdpBridge, this.telegram);
    this.telegram.setBridges(this.cdpBridge, this.autoAcceptDaemon);
    this.approvalEngine = new ApprovalEngine();
    this.channelRouter = new ChannelRouter();
    this.channelRouter.register(this.telegram);
    this.channelRouter.register(this.meta);
    this.securityGateway = new SecurityGateway();
    
    // [v5.0] Phase 2 Initialize
    this.vscodeBridge = new VSCodeBridge(
        process.env.VSCODE_WS_HOST || "127.0.0.1",
        Number(process.env.VSCODE_WS_PORT) || 3710
    );
    this.sessions = new SessionOrchestrator();
    this.nlTranslator = new NLCommandTranslator();
    this.emailManager = new EmailClientManager();
    this.gitNexusIndexer = new GitNexusIndexer();
    // [v24] Pillar 1: VRAM Yielding — GPU monitor
    this.vramGuard = new VRAMGuard();
    // TTS Engine Selection: Hybrid Architecture
    const forceMode = process.env.LIVA_TTS_ENGINE;
    if (!forceMode || forceMode === 'python') {
      logger.info(`🗣️ [CoreKernel] TTS Engine: Python Edge-TTS (Primary)`);
      this.voiceEngine = new VoiceEngine();
    } else {
      logger.info(`🗣️ [CoreKernel] TTS Engine: Kokoro-JS Local (Forced via Env)`);
      this.voiceEngine = new KokoroVoiceEngine();
      this.#isTtsFallbackActive = true;
    }
    // STT Engine Selection: WhisperNode (HTTP)
    logger.info(`👂 [CoreKernel] STT Engine: WhisperNode HTTP`);
    this.whisperNode = new WhisperNode();

    this.#transitionSchema = new Map();
    this.#orchestrationTensor = {
      dimensions: [3, 3],
      getWeight: (latencyMs: number) => Math.max(0.1, 1 / (latencyMs + 1)),
      updateWeights: (feedbackLoop: number[]) => { /* Tensor update logic */ }
    };

    // --- START GARBAGE COLLECTION ENGINE ---
    this.#startGarbageCollection();

    // --- V14: HOT-SWAP DNA FILE WATCHER ---
    this.#watchSkillMutations();

    // --- CENTRALIZED AUTHORITY REGISTRATION ---
    this.#registerAuthorityTransition<"ui_broadcast", "ACTIVE">(
      "ui_broadcast", 
      {
        token: this.#mintCommandToken<"ui_broadcast", "ACTIVE">("ui_broadcast", 99999999999),
        execute: async (event: { name: string; data?: any }) => {
          await this.ui.broadcastUIEvent(event.name, event.data);
        }
      }
    );

    this.#registerAuthorityTransition<"agent_input", "ACTIVE">(
      "agent_input", 
      {
        token: this.#mintCommandToken<"agent_input", "ACTIVE">("agent_input", 99999999999),
        execute: async (text: string) => {
          await this.agentLoop.handleUserInput(text);
        }
      }
    );

    this.ui.on("user_input", async (userText: string) => {
      const weight = this.#orchestrationTensor.getWeight(this.#currentLatency);
      await this.#dispatch("agent_input", userText);
/* istanbul ignore next */
      if (weight <= 0.2) {
        /* istanbul ignore next */
        logger.warn(`⚠️ [Orchestrator] High latency (${this.#currentLatency}ms). Proceeding anyway.`);
      }
    });

    this.ui.on("get_user_profile", async (ws) => {
      const profile = await this.memory.getUserProfile();
      this.ui.sendUserProfile(ws, profile);
    });

    this.ui.on("update_user_profile", async (ws, profileData) => {
      // Validate
      if (!profileData || typeof profileData.name !== "string" || !profileData.name.trim() || !String(profileData.birthYear || "").trim() || !profileData.nationality?.trim()) {
        logger.warn("⚠️ [CoreKernel] Invalid profile update request rejected.");
        return;
      }
      
      await this.memory.updateUserProfile(profileData);
      const updated = await this.memory.getUserProfile();
      
      // Emit success back to the specific client
      if (ws.readyState === 1) { // WebSocket.OPEN
        ws.send(JSON.stringify({ event: "profile_updated_success", payload: updated }));
      }
      
      // Trigger AI Sync (Reload System Location for context)
      // If the location has changed, we should update AgentLoop so PromptBuilder picks it up.
      // E.g., this.agentLoop.setSystemLocation(updated.location);
      if (updated.location) {
          // If timezone wasn't changed, keep current
          const tz = this.agentLoop.currentSystemTimezone;
          this.agentLoop.setSystemLocation(updated.location, tz);
      }
    });

    this.ui.on("config_updated", (config: any) => {
      this.#handleConfigUpdated(config);
    });

    this.zalo.on("zalo_incoming", async (userText: string) => {
      await this.#dispatch("agent_input", userText);
    });

    // --- [v5.0] TELEGRAM EVENT PIPELINE ---
    this.telegram.on("message", async (msg: NormalizedMessage) => {
      // Security gate: validate sender through SecurityGateway
      const blockReason = this.securityGateway.validateIncoming(msg.channel, msg.senderId);
      if (blockReason) {
        logger.warn(`[RemoteControl] 🛡️ Blocked: ${blockReason}`);
        return;
      }

      logger.info(`📱 [RemoteControl] Telegram command from ${msg.senderName}: "${msg.text}"`);
      const enrichedMessage = `[Tin nhắn từ Telegram điện thoại]: ${msg.text}`;
      
      // Keep session history
      const sessionId = this.sessions.getOrCreateSession(msg.senderId, msg.channel).id;
      this.sessions.appendMessage(sessionId, msg);

      // Translate NL to IDE Command
      const intent = await this.nlTranslator.translate(msg.text);
      if (intent.action !== "unknown" && intent.confidence > 0.8) {
        logger.info(`[RemoteControl] NL translated to IDE action: ${intent.action}`);
        // Can be forwarded to AgentLoop as an execution token, or handled natively.
      }

      await this.#dispatch("agent_input", enrichedMessage);
    });

    // Meta Webhook Pipeline
    this.meta.on("message", async (msg: NormalizedMessage) => {
      const blockReason = this.securityGateway.validateIncoming(msg.channel, msg.senderId);
      if (blockReason) return;

      logger.info(`📱 [RemoteControl] Meta command from ${msg.senderName}: "${msg.text}"`);
      const enrichedMessage = `[Tin nhắn từ Messenger/IG]: ${msg.text}`;
      
      const sessionId = this.sessions.getOrCreateSession(msg.senderId, msg.channel).id;
      this.sessions.appendMessage(sessionId, msg);

      const intent = await this.nlTranslator.translate(msg.text);
      if (intent.action !== "unknown" && intent.confidence > 0.8) {
        logger.info(`[RemoteControl] NL translated to IDE action: ${intent.action}`);
      }

      await this.#dispatch("agent_input", enrichedMessage);
    });

    this.meta.on("postback", async (postback: { senderId: string; payload: string }) => {
      logger.info(`[MetaBridge] Received postback: ${postback.payload}`);
      if (postback.payload.startsWith("approve:") || postback.payload.startsWith("reject:")) {
        const [action, id] = postback.payload.split(":");
        this.approvalEngine.resolveApproval(id, action === "approve");
      }
    });

    // Handle Telegram approval callback buttons (Approve/Reject)
    this.telegram.on("callback_query", async (query: { queryId: string; senderId: string; data: string; chatId?: number; messageId?: number }) => {
      const { data, chatId, messageId } = query;

/* istanbul ignore next */
      if (data.startsWith("approve:") || data.startsWith("reject:")) {
        const parts = data.split(":");
        const approved = parts[0] === "approve";
        const approvalId = parts[1];

        if (approvalId.startsWith("hitl-")) {
            import("../security/HITLGuard").then(m => m.HITLGuard.respond(approvalId, approved));
        } else {
            this.approvalEngine.resolveApproval(approvalId, approved);
        }

        // Update the Telegram message to show decision
/* istanbul ignore next */
        if (chatId && messageId) {
/* istanbul ignore next */
          const statusText = approved ? "✅ **APPROVED** — Đã phê duyệt." : "❌ **REJECTED** — Đã từ chối.";
          this.telegram.editMessage(String(chatId), messageId, statusText).catch(() => {});
        }
      }
    });

    // --- [v5.0] CDP BRIDGE — Approval Button Detection ---
    this.cdpBridge.on("approval_required", async (payload: { text: string; selector: string }) => {
      logger.info(`[CDP] 🔔 IDE yêu cầu phê duyệt: "${payload.text}"`);

      // Create approval record
      const risk = this.securityGateway.classifyRisk(payload.text);
      const approvalId = this.approvalEngine.createApproval(
        "antigravity",
        payload.text,
        `IDE button detected: ${payload.selector}`,
        risk
      );

      // Forward to Telegram (primary remote control channel)
      try {
        await this.approvalEngine.forwardToChannel(approvalId, this.telegram, this.#getDefaultRemoteSenderId());
      } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
        logger.warn(`[CDP] Could not forward approval to Telegram: ${errMsg}`);
      }

      // Also broadcast to local UI
      await this.#dispatch("ui_broadcast", {
        name: "exec_approval_required",
        data: { approvalId, toolName: "IDE", command: payload.text, reason: payload.selector }
      });
    });

    // When approval is granted, click the button in IDE
    this.approvalEngine.on("approval_granted", async (approval: any) => {
/* istanbul ignore next */
      if (approval.source === "antigravity" && this.cdpBridge.isConnected()) {
        logger.info(`[CDP] ✅ Remote approval granted — clicking button in IDE`);
        try {
          await this.cdpBridge.clickApprovalButton(true);
        } catch (e: unknown) {
          const errMsg = e instanceof Error ? e.message : String(e);
          /* istanbul ignore next */
          logger.error(`[CDP] Failed to click approval button: ${errMsg}`);
        }
      }
    });

    this.approvalEngine.on("approval_denied", async (approval: any) => {
/* istanbul ignore next */
      if (approval.source === "antigravity" && this.cdpBridge.isConnected()) {
        logger.info(`[CDP] ❌ Remote approval denied — clicking reject in IDE`);
        try {
          await this.cdpBridge.clickApprovalButton(false);
        } catch (e: unknown) {
          const errMsg = e instanceof Error ? e.message : String(e);
          /* istanbul ignore next */
          logger.error(`[CDP] Failed to click reject button: ${errMsg}`);
        }
      }
    });

    // ╔══════════════════════════════════════════════════════════════════════╗
    // ║  v25 PILLAR 4: WAKE-WORD EDGE OFFLOADING                            ║
    // ║  Wake word detection now handled by FRONTEND (ONNX WASM)              ║
    // ║  Backend only receives "wake_word_triggered" event when detected       ║
    // ╚══════════════════════════════════════════════════════════════════════╝

    // --- AUDIO INPUT → VADWorkerBridge → WhisperNode ---
    /**
     * [v25 Pillar 4] 
     * 
     * Wake word is now detected on FRONTEND using ONNX WASM.
     * Backend receives audio only when user is in full STT mode (push-to-talk).
     * 
     * Audio routing:
     * - Frontend ONNX: Wake word detection (always-on mic)
     * - Backend Whisper: Main STT transcription (only when user speaks)
     */
    this.ui.on("audio_input", (buffer: Buffer) => {
      if (this.vadBridge && this.vadBridge.isReady) {
        // PRIMARY PATH: Neural VAD — convert Buffer to Float32Array for worker
        const float32 = new Float32Array(buffer.buffer, buffer.byteOffset, buffer.byteLength / 4);
        this.vadBridge.pushAudioSamples(float32);
      } else {
        // FALLBACK PATH: Legacy silence timer
        logger.debug("[Audio] VAD not ready, using legacy pushAudioChunk");
        this.whisperNode.pushAudioChunk(buffer);
      }
    });

    // --- [v25 Pillar 4] WAKE WORD TRIGGERED (from Frontend ONNX) ---
    // Frontend detected wake word → activate voice mode
    this.ui.on("wake_word_triggered", () => {
      logger.info(`[CoreKernel] Wake word triggered from frontend (ONNX WASM)`);
      
      // Notify all UI clients: wake word was detected → UI activates voice mode
      this.ui.broadcastUIEvent("wake_word_detected", { trailingText: "" });
      
      // NOTE: Frontend handles the voice activation UI flow
      // Backend just receives the notification for logging/analytics
    });

    // --- [DEPRECATED v25] Wake Word Mode Toggle ---
    // Wake word mode is now entirely managed by frontend
    // This event is kept for backward compatibility but does nothing
    this.ui.on("wake_word_mode", (_enabled: boolean) => {
      logger.debug(`[CoreKernel] wake_word_mode event received (deprecated — now handled on frontend)`);
    });

    // --- [PILLAR 1] STAGE 1: AUDIO DUCKING (Spinal Reflex — 0ms latency) ---
    // When user starts speaking, DON'T kill LLM — just reduce TTS volume.
    // LLM continues generating tokens silently. If user only coughed/said "ừm",
    // we restore volume and lose ZERO computation.
    this.ui.on("speech_start_vad", () => {
      logger.debug("[v23 Stage 1] 🔉 Audio Ducking: TTS volume → 20% (LLM still running)");
      this.ui.broadcastUIEvent("audio_ducking", { volume: 0.2 });
    });

    // --- [PILLAR 2] SPECULATIVE RAG WARMING ---
    // When partial transcription arrives (>5 words), pre-warm L2/L3 memory cache.
    // By the time user finishes speaking, vectors are already in RAM.
    this.whisperNode.on("transcription_partial", async (partialText: string) => {
      const wordCount = partialText.trim().split(/\s+/).length;
      if (wordCount >= 5) {
        logger.debug(`[v23 Speculative RAG] 🔮 Pre-warming context for: "${partialText.substring(0, 50)}..."`);
        // Fire-and-forget: warm the SemanticRouter + sqlite-vec cache
        this.agentLoop.speculativeWarm(partialText).catch(() => {});
      }
    });

    // --- HARD INTERRUPT (UI button/hotkey — always hard abort) ---
    this.ui.on("interrupt", () => {
      logger.warn(`[CoreKernel] 🛑 HARD INTERRUPT from UI. Kill LLM + TTS + VRAM.`);
      this.voiceEngine?.preempt?.();
      this.agentLoop.bargeIn();
      this.whisperNode.flush();
      this.ui.broadcastUIEvent("audio_ducking", { volume: 1.0 });
    });

    // --- [PILLAR 1] STAGE 2: SEMANTIC BARGE-IN (Brain Verification) ---
    // When transcription arrives, classify: backchannel → resume, real speech → hard abort.
    this.whisperNode.on("transcription_ready", async (text: string) => {
      // Import backchannel detector
      const { isBackchannel } = await import("../utils/BackchannelDetector");

      // Sanitize STT feedback contamination
      let sanitized = text
          .replace(/[,\s]*(Dạ|dạ|Em|em|Ạ|ạ)[,\s]*$/gi, '')
          .trim();
      sanitized = sanitized
          .replace(/^(Dạ[,\s]+em|Dạ)[,\s]+/gi, '')
          .replace(/[,\s]+(Dạ[,\s]+em|Dạ|ạ|em|nhé|nha|ạ)[,\s]*$/gi, '')
          .trim();

      if (!sanitized) {
        // Empty after sanitization → restore volume, skip
        this.ui.broadcastUIEvent("audio_ducking", { volume: 1.0 });
        return;
      }

      // [STAGE 2] Backchannel Check
      if (isBackchannel(sanitized)) {
        // "ừm", "ok", cough → restore TTS volume, AI continues speaking
        logger.info(`[v23 Stage 2] 🔊 Backchannel detected: "${sanitized}" → Resume TTS (no abort)`);
        this.ui.broadcastUIEvent("audio_ducking", { volume: 1.0 });
        return;
      }

      // Real speech detected → HARD ABORT
      logger.info(`[v23 Stage 2] 🛑 Real speech detected: "${sanitized.substring(0, 50)}" → Hard Abort`);
      this.voiceEngine?.preempt?.();
      this.agentLoop.bargeIn();
      this.ui.broadcastUIEvent("audio_ducking", { volume: 1.0 });

      await this.#dispatch("agent_input", sanitized);
    });

    this.agentLoop.Orchestrator.on("suspend_peripherals", () => {
      logger.warn(`[Z-MAS] 🛑 Singularit Mode! Đóng băng Thanh quản và Mắt để tối ưu 100% VRAM cho 26B!`);
      this.voiceEngine?.preempt?.();
      this.whisperNode.flush();
    });

    this.agentLoop.Orchestrator.on("resume_peripherals", () => {
      logger.info(`[Z-MAS] 🟢 Expert đã xả VRAM. Kích hoạt lại Thanh quản và Lỗ tai...`);
    });



    this.voiceEngine?.on("audio_base64", (base64: string) => {
      this.ui.broadcastUIEvent("ai_audio_chunk", { audio: base64 });
    });

    // --- DASHBOARD EVENT HANDLERS (Multi-Window Support) ---
    this.ui.on("get_skills_list", (ws: any) => {
      const whitelistData = this.registry.whitelist.getAll();
      const skills = this.registry.getAllSkills().map(s => {
        const isOpen = this.registry.circuitBreaker.getOpenCircuits().has(s.name);
        const errorMsg = isOpen ? this.registry.circuitBreaker.getCircuitError(s.name) : null;
        const wlEntry = whitelistData[s.name];
        const isEnabled = wlEntry ? wlEntry.enabled : true; // Default: enabled
        return {
          name: s.name,
          description: s.description,
          isCoreSkill: s.isCoreSkill || false,
          category: s.category || (s.isCoreSkill ? "Core" : "Extension"),
          status: !isEnabled ? "disabled" : isOpen ? "error" : "active",
          enabled: isEnabled,
          errorMsg: errorMsg
        };
      });
      this.ui.sendSkillsList(ws, skills);
    });

    this.ui.on("test_skill", async (ws: any, payload: { name: string }) => {
      // Simulate a ping or clear the circuit breaker to force it closed for testing
      logger.info(`[UI] Testing skill ${payload.name}... resetting circuit breaker.`);
      this.registry.circuitBreaker.recordSuccess(payload.name); // Reset state to CLOSED
      
      const whitelistData = this.registry.whitelist.getAll();
      const skills = this.registry.getAllSkills().map(s => {
        const isOpen = this.registry.circuitBreaker.getOpenCircuits().has(s.name);
        const wlEntry = whitelistData[s.name];
        const isEnabled = wlEntry ? wlEntry.enabled : true;
        return {
          name: s.name,
          description: s.description,
          isCoreSkill: s.isCoreSkill || false,
          category: s.category || (s.isCoreSkill ? "Core" : "Extension"),
          status: !isEnabled ? "disabled" : isOpen ? "error" : "active",
          enabled: isEnabled,
          errorMsg: isOpen ? this.registry.circuitBreaker.getCircuitError(s.name) : null
        };
      });
      this.ui.sendSkillsList(ws, skills);
    });

    // --- SKILL WHITELIST TOGGLE ---
    this.ui.on("toggle_skill", async (ws: any, payload: { name: string; enabled: boolean }) => {
      logger.info(`[UI] Toggling skill ${payload.name}: ${payload.enabled ? "ENABLED" : "DISABLED"}`);
      this.registry.whitelist.setEnabled(payload.name, payload.enabled);
      // Respond with updated list
      this.ui.emit("get_skills_list", ws);
    });

    this.ui.on("toggle_all_skills", async (ws: any, payload: { enabled: boolean }) => {
      logger.info(`[UI] Bulk toggle all skills: ${payload.enabled ? "ENABLED" : "DISABLED"}`);
      const allSkills = this.registry.getAllSkills();
      this.registry.whitelist.bulkSet(allSkills.map(s => ({ name: s.name, enabled: payload.enabled })));
      this.ui.emit("get_skills_list", ws);
    });

    // --- TASK MANAGER EVENTS ---
    this.ui.on("get_tasks", (ws: any) => {
      const sm = this.memory.getStructuredMemoryInstance();
      if (!sm) { this.ui.sendTasksList(ws as import("ws").WebSocket, []); return; }
      const tasks = sm.getTasks();
      this.ui.sendTasksList(ws as import("ws").WebSocket, tasks);
    });

    this.ui.on("add_task", (ws: any, payload: any) => {
      const sm = this.memory.getStructuredMemoryInstance();
      if (!sm) return;
      const id = `task_${Date.now()}_${Math.random().toString(36).substring(2, 8)}`;
      sm.addTask({ id, title: payload.title, description: payload.description, priority: payload.priority });
      // Respond with updated list
      this.ui.sendTasksList(ws as import("ws").WebSocket, sm.getTasks());
      
      // Auto-trigger inline planning if description exists
      if (payload.description?.trim()) {
        this.ui.emit("task_plan_chat", ws, { taskId: id, message: payload.description });
      }
    });

    // ═══════════════════════════════════════════════════════
    //  [v25] Inline Task Planning Chat — Self-contained mini-LLM conversations
    //  Each task gets its own conversation history (isolated from main AgentLoop).
    //  AI asks clarifying questions in Dashboard → user answers → AI auto-updates task.
    // ═══════════════════════════════════════════════════════
    const taskPlanHistories = new Map<string, Array<{ role: string; content: string }>>();

    this.ui.on("task_plan_chat", async (ws: any, payload: { taskId: string; message: string }) => {
      const { taskId, message } = payload;
      if (!taskId || !message?.trim()) return;

      const sm = this.memory.getStructuredMemoryInstance();
      if (!sm) return;
      const tasks = sm.getTasks();
      const task = tasks.find((t: any) => t.id === taskId);
      if (!task) return;

      // Init or retrieve conversation history for this task
      if (!taskPlanHistories.has(taskId)) {
        taskPlanHistories.set(taskId, []);
      }
      const history = taskPlanHistories.get(taskId)!;

      // Add user message
      history.push({ role: "user", content: message });

      // Lấy ngôn ngữ người dùng
      const userProfile = await this.memory.getUserProfile() || {};
      const userLang = userProfile.language || "vi-VN";

      // Build system prompt for planning
      const now = new Date();
      const systemPrompt = `Bạn là trợ lý lập kế hoạch của người dùng. Nhiệm vụ: hỗ trợ lên lịch trình chi tiết.
Thời gian hiện tại: ${now.toLocaleString(userLang, { timeZone: "Asia/Ho_Chi_Minh" })}
Kế hoạch: "${task.title}"
${task.description ? `Mô tả ban đầu: ${task.description}` : ""}

QUY TẮC:
1. Nếu thiếu thông tin quan trọng (thời gian cụ thể, địa điểm, ngân sách, phương tiện, v.v.), hãy HỎI NGẮN GỌN (1-2 câu).
2. Khi đã đủ thông tin, hãy tóm tắt kế hoạch chi tiết theo dạng timeline/bullet points và kết thúc bằng dòng:
   [PLAN_COMPLETE]
   (theo sau bởi nội dung kế hoạch hoàn chỉnh)
3. TRẢ LỜI BẰNG NGÔN NGỮ: ${userLang}. Ngắn gọn, thân thiện.
4. KHÔNG bao giờ bịa thông tin — chỉ dùng thông tin người dùng cung cấp.`;

      const messages = [
        { role: "system", content: systemPrompt },
        ...history
      ];

      try {
        let aiReply = "Xin lỗi, tôi không thể trả lời lúc này.";
        const USE_NATIVE_IPC = process.env.LIVA_USE_NATIVE === "true";
        
        if (USE_NATIVE_IPC) {
          const { NativeIPCClient } = await import("../utils/NativeIPCClient");
          // messages is {role:"system",content:string}|HistoryItem[] — cast needed to satisfy ChatMessage[]
          const client = new NativeIPCClient();
          const completion = await client.chat.completions.create({
            model: "local-ghost-router",
            messages: messages as any,
            temperature: 0.4,
            max_tokens: 800,
            stream: false,
          });
          aiReply = (completion as NativeIPCChatResponse).choices[0]?.message?.content?.trim() || aiReply;
        } else {
          // Lightweight LLM call (reuse same local model, no AgentLoop overhead)
          const OpenAI = (await import("openai")).default;
          const port = this.agentLoop.Orchestrator.routerPort;
          const client = new OpenAI({
            baseURL: `http://127.0.0.1:${port}/v1`,
            apiKey: "local-ghost-router",
            timeout: 15000,
            maxRetries: 1
          });

          const completion = await client.chat.completions.create({
            model: "local-ghost-router",
            messages: messages as any,
            temperature: 0.4,
            max_tokens: 800,
            stream: false,
          });
          aiReply = completion.choices[0]?.message?.content?.trim() || aiReply;
        }

        history.push({ role: "assistant", content: aiReply });

        // Check if AI decided the plan is complete
        if (aiReply.includes("[PLAN_COMPLETE]")) {
          const planContent = aiReply.split("[PLAN_COMPLETE]").pop()?.trim() || aiReply.replace("[PLAN_COMPLETE]", "").trim();
          const cleanReply = aiReply.replace("[PLAN_COMPLETE]", "").trim();
          
          // Auto-update the task with the finalized plan
          sm.updateTask(taskId, { description: planContent, status: "pending" });
          
          // Clean up conversation history
          taskPlanHistories.delete(taskId);
          
          // Send final reply + updated tasks list
          if (ws.readyState === 1) {
            ws.send(JSON.stringify({ event: "task_plan_reply", payload: { taskId, message: cleanReply, done: true } }));
          }
          this.ui.sendTasksList(ws as import("ws").WebSocket, sm.getTasks());
        } else {
          // Send AI question back to Dashboard
          if (ws.readyState === 1) {
            ws.send(JSON.stringify({ event: "task_plan_reply", payload: { taskId, message: aiReply, done: false } }));
          }
        }
      } catch (e: any) {
        logger.warn(`[TaskPlanner] LLM call failed: ${e.message}`);
        if (ws.readyState === 1) {
          ws.send(JSON.stringify({ event: "task_plan_reply", payload: { taskId, message: "⚠️ Không thể kết nối AI. Vui lòng thử lại.", done: false } }));
        }
      }
    });

    this.ui.on("update_task", (ws: any, payload: any) => {
      const sm = this.memory.getStructuredMemoryInstance();
      if (!sm) return;
      sm.updateTask(payload.id, payload.updates || {});
      this.ui.sendTasksList(ws as import("ws").WebSocket, sm.getTasks());
    });

    this.ui.on("delete_task", (ws: any, payload: any) => {
      const sm = this.memory.getStructuredMemoryInstance();
      if (!sm) return;
      sm.deleteTask(payload.id);
      this.ui.sendTasksList(ws as import("ws").WebSocket, sm.getTasks());
    });

    this.ui.on("execute_task", (ws: any, payload: any) => {
      const sm = this.memory.getStructuredMemoryInstance();
      if (!sm) return;
      sm.updateTask(payload.id, { status: "in-progress" });
      // Send the task to the AgentLoop as a user command
      this.ui.emit("user_input", payload.title);
      this.ui.sendTasksList(ws as import("ws").WebSocket, sm.getTasks());
    });

    let cachedStaticStats: Record<string, unknown> | null = null;

    this.ui.on("get_system_status", async (ws: unknown) => {
      let networkStatus = "Disconnected";
      
      try {
          const os = await import('os');

          if (!cachedStaticStats) {
              cachedStaticStats = { cpuModel: "Đang quét...", totalRamGB: 0, diskInfo: "Đang quét..." };
              const cpus = os.cpus();
              if (cpus && cpus.length > 0) cachedStaticStats.cpuModel = cpus[0].model.trim();
              cachedStaticStats.totalRamGB = Math.round(os.totalmem() / 1024 / 1024 / 1024);
              
              if (os.platform() === 'win32') {
                  import('child_process').then(cp => {
                      cp.exec('wmic diskdrive get model,size /format:csv', { timeout: 2000 }, (err, stdout) => {
                          if (!err && stdout) {
                              const lines = stdout.toString().split('\n').map(l => l.trim()).filter(l => l.length > 0 && !l.toLowerCase().includes('model,size') && l.includes(','));
                              const disks = lines.map(l => {
                                  const parts = l.split(',');
                                  if (parts.length >= 3) {
                                      const model = parts[1].trim();
                                      const sizeStr = parts[2].trim();
                                      const sizeGB = Math.round(parseInt(sizeStr) / 1024 / 1024 / 1024);
                                      return `${model} (${sizeGB}GB)`;
                                  }
                                  return '';
                              }).filter(d => d.length > 0);
                              
                              if (disks.length > 0) cachedStaticStats!.diskInfo = `${disks.length} Ổ cứng: ` + disks.join(', ');
                          }
                      });
                  }).catch(() => {});
              }
          }
          
          const nets = os.networkInterfaces();
          const active: string[] = [];
          for (const [name, interfaces] of Object.entries(nets)) {
              if (!interfaces) continue;
              for (const net of interfaces) {
                  if (!net.internal && net.family === 'IPv4') active.push(`${name}`);
              }
          }
          if (active.length > 0) networkStatus = "Online (" + active.join(', ') + ")";
      } catch (e) {
          // Ignore stats errors
      }

      // ═══════════════════════════════════════════════════════
      //  DEEP HEALTH PROBES — Active ping each subsystem
      // ═══════════════════════════════════════════════════════
      const isNativeMode = String(process.env.LIVA_USE_NATIVE).trim().toLowerCase() === "true";
      const orchestratorStatus = this.agentLoop.Orchestrator.getStatus();
      const processMemory = process.memoryUsage();

      // Helper: TCP port check with latency
      const tcpPing = async (port: number, host = "127.0.0.1", timeoutMs = 1500): Promise<{ ok: boolean; latencyMs: number }> => {
          const net = await import("net");
          const start = Date.now();
          return new Promise(resolve => {
              const sock = net.createConnection({ port, host, timeout: timeoutMs }, () => {
                  sock.destroy();
                  resolve({ ok: true, latencyMs: Date.now() - start });
              });
              sock.on("error", () => resolve({ ok: false, latencyMs: Date.now() - start }));
              sock.on("timeout", () => { sock.destroy(); resolve({ ok: false, latencyMs: Date.now() - start }); });
          });
      };

      // --- Probe 1: AI Engine (gRPC or HTTP) ---
      let aiEngineHealth: { status: string; latencyMs: number; detail: string; modelLoaded?: string } = { status: "offline", latencyMs: -1, detail: "" };
      try {
          const aiStart = Date.now();
          if (isNativeMode) {
              const aiRes = await safeFetch("http://127.0.0.1:8100/health", {}, 2000).catch(() => null);
              if (aiRes && aiRes.ok) {
                  aiEngineHealth = { status: "online", latencyMs: Date.now() - aiStart, detail: "Native gRPC (HTTP health OK)" };
              } else {
                  const tcp = await tcpPing(8100);
                  aiEngineHealth = {
                      status: tcp.ok ? "online" : "offline",
                      latencyMs: tcp.latencyMs,
                      detail: tcp.ok ? "Native gRPC (TCP OK)" : "gRPC port 8100 unreachable"
                  };
              }
          } else {
              const port = orchestratorStatus.routerPort || 8000;
              const res = await safeFetch(`http://127.0.0.1:${port}/v1/models`, {}, 2000);
              const body = await res.json() as Record<string, unknown>;
              const models = Array.isArray(body.data) ? body.data : [];
              const modelId = (models[0] as Record<string, unknown>)?.id || "unknown";
              aiEngineHealth = {
                  status: "online",
                  latencyMs: Date.now() - aiStart,
                  detail: `llama-server (port ${port})`,
                  modelLoaded: String(modelId)
              };
          }
      } catch {
          aiEngineHealth.detail = isNativeMode ? "gRPC port 8100 unreachable" : "llama-server not responding";
      }

      // --- Probe 2: Voice Engine (port 8002) ---
      let voiceHealth: { status: string; latencyMs: number; detail: string } = { status: "offline", latencyMs: -1, detail: "" };
      try {
          const tcp = await tcpPing(8002);
          voiceHealth = {
              status: tcp.ok ? "online" : "offline",
              latencyMs: tcp.latencyMs,
              detail: tcp.ok ? "Edge-TTS Python" : "Port 8002 unreachable"
          };
      } catch {
          voiceHealth.detail = "Port 8002 check failed";
      }

      // --- Probe 3: Gateway internals ---
      const gatewayHealth = {
          status: "online" as const,
          latencyMs: 0,
          detail: "WebSocket Server",
          wsClients: this.ui.connectedClientCount,
          skillsLoaded: this.registry.getAllSkills().length,
      };

      // --- Probe 4: ModelOrchestrator ready state ---
      const orchestratorReady = this.agentLoop.Orchestrator.isReady();
      const orchestratorHealth = {
          status: orchestratorReady ? "online" : "offline",
          detail: orchestratorReady
              ? `Ready (port ${orchestratorStatus.routerPort})`
              : "NOT READY — AgentLoop blocked!",
      };

      // --- Probe 5: VRAMGuard state ---
      const vramGuardHealth = {
          status: this.vramGuard.isYielded ? "degraded" : "online",
          isYielded: this.vramGuard.isYielded,
          detail: this.vramGuard.isYielded ? "VRAM yielded to external app" : "VRAM available for AI",
      };

      // --- Probe 6: Memory (SQLite) ---
      let memoryHealth: { status: string; detail: string } = { status: "offline", detail: "" };
      try {
          const sm = this.memory.getStructuredMemoryInstance();
          const factCount = sm.count;
          memoryHealth = { status: "online", detail: `SQLite OK (${factCount} facts)` };
      } catch (e: unknown) {
          const errMsg = e instanceof Error ? e.message : String(e);
          memoryHealth = { status: "offline", detail: `SQLite: ${errMsg.substring(0, 60)}` };
      }

      // --- Probe 7: Whisper STT ---
      const whisperHealth = {
          status: this.whisperNode ? "online" : "offline",
          detail: this.whisperNode ? "WhisperNode active" : "Not initialized",
      };

      // --- Probe 8: Remote Control Channels ---
      const remoteControlEnabled = this.securityGateway.isRemoteControlEnabled();
      const telegramConfigured = !!process.env.TELEGRAM_BOT_TOKEN;
      const zaloConfigured = !!process.env.ZALO_OA_ACCESS_TOKEN && !process.env.ZALO_OA_ACCESS_TOKEN.includes("NHẬP_TOKEN");
      const remoteHealth = {
          enabled: remoteControlEnabled,
          telegram: {
              configured: telegramConfigured,
              status: remoteControlEnabled && telegramConfigured ? "online" : telegramConfigured ? "standby" : "not_configured",
          },
          zalo: {
              configured: zaloConfigured,
              status: zaloConfigured ? "online" : "not_configured",
          },
      };

      const status = {
        model: process.env.EXPERT_MODEL_NAME || "Unknown",
        provider: process.env.AI_PROVIDER || "local",
        engineMode: isNativeMode ? "native_grpc" : "llama_http",
        uptime: process.uptime(),
        memoryUsage: processMemory.heapUsed,
        rssMemory: processMemory.rss,
        externalMemory: processMemory.external,
        telemetry: this.telemetryLogs,
        osStats: {
            cpuModel: cachedStaticStats?.cpuModel || "Đang quét...",
            totalRamGB: cachedStaticStats?.totalRamGB || 0,
            networkStatus,
            diskInfo: cachedStaticStats?.diskInfo || "Đang quét..."
        },
        healthChecks: {
            aiEngine: aiEngineHealth,
            voiceEngine: voiceHealth,
            gateway: gatewayHealth,
            orchestrator: orchestratorHealth,
            vramGuard: vramGuardHealth,
            memory: memoryHealth,
            whisper: whisperHealth,
            remoteControl: remoteHealth,
        }
      };
      this.ui.sendSystemStatus(ws as import("ws").WebSocket, status);
    });

    // [P5] Memory Reset — Dashboard triggers full memory wipe
    this.ui.on("reset_memory", async (ws: any) => {
      logger.warn("[CoreKernel] 🧹 Nhận lệnh RESET MEMORY từ Dashboard!");
      const result = await this.memory.resetAllMemory();
      if (ws.readyState === 1) { // WebSocket.OPEN
        ws.send(JSON.stringify({
          event: "memory_reset_result",
          payload: result,
        }));
      }
      if (result.success) {
        this.ui.broadcastUIEvent("memory_reset_complete", {});
      }
    });

    // --- CAMERA VISION (Webcam → AI Multimodal) ---
    this.ui.on("camera_frame", (payload: { image: string; timestamp: number }) => {
      // Store latest frame — will be injected into next AI conversation as visual context
      this.#latestCameraFrame = payload.image;
      logger.info(`[Camera] 📸 Nhận frame webcam (${Math.round(payload.image.length / 1024)}KB)`);
    });

    this.#setupReactiveSync();
  }

  // V14 Hot-Swap File Watcher
  #watchSkillMutations() {
    import('fs').then(fs => {
       import('path').then(path => {
          const skillsDir = path.join(process.cwd(), "src", "skills");
/* istanbul ignore next */
          if (!fs.existsSync(skillsDir)) return;

          let debounceTimer: NodeJS.Timeout | null = null;

          // 🔒 [Memory Fix #3] Lưu handle vào #fileWatcher để có thể close() sau này
          /* istanbul ignore next */
          this.#fileWatcher = fs.watch(skillsDir, (eventType: string, filename: string | null) => {
             if (filename && (filename.endsWith('.ts') || filename.endsWith('.js'))) {

                 // [v25 FIX] Skip base config files to prevent false "mutation" alarms at boot
                 if (['SkillMetadata.ts', 'index.ts', 'BaseSkill.ts'].includes(filename) || filename.includes('.test.')) {
                     return;
                 }

                 if (debounceTimer) clearTimeout(debounceTimer);
                 debounceTimer = setTimeout(() => {
                     logger.warn(`🔥 [DNA Hot-Swap] Phát hiện Thể Đột Biến kỹ năng (${filename}) do AI Singularity sinh ra!`);
                     this.registry.registerLocalSkills().catch(e => logger.error("Lỗi:", e));
                 }, 1000);
             }
          });
       });
    }).catch(/* istanbul ignore next */ e => logger.error("Lỗi import FS trong File Watcher", e));
  }

  #startGarbageCollection() {
    this.#gcIntervalId = setInterval(() => {
      const now = Date.now();
      let cleanedCount = 0;

      for (const [id, schema] of this.#transitionSchema.entries()) {
        if (schema.token.__expiresAt < now) {
          this.#transitionSchema.delete(id);
          cleanedCount++;
        }
      }

      /* istanbul ignore next */
      if (cleanedCount > 0) {
        logger.info(`[GC] Cleaned ${cleanedCount} expired CommandTokens from CoreKernel.`);
      }

      // V14: Lò đốt rác Tẩy Não (Ép Node.js V8 Engine Dọn Dẹp định kỳ)
/* istanbul ignore next */
      if (global.gc) {
          global.gc();
      }
    }, 60000); // V14: Đã tăng chu kỳ lên 60s để nhường CPU cho Garbage Collector
    this.#gcIntervalId.unref(); // Don't prevent process exit
  }

  #registerAuthorityTransition<T extends string, Status extends string>(id: string, schema: TransitionSchema<T, Status>) {
    this.#transitionSchema.set(id, schema);
  }

  /**
   * [v5.0] Get default Telegram sender ID for forwarding CDP approvals.
   * Uses first entry from TELEGRAM_ALLOWED_IDS.
   */
  #getDefaultRemoteSenderId(): string {
    const ids = (process.env.TELEGRAM_ALLOWED_IDS || "").split(",").map(s => s.trim()).filter(Boolean);
    return ids[0] || "";
  }

  async #dispatch(id: string, payload: any) {
    const transition = this.#transitionSchema.get(id);
/* istanbul ignore next */
    if (transition) {
      if (transition.token.__authority && transition.token.__expiresAt > Date.now()) {
        await transition.execute(payload);
/* istanbul ignore next */
      } else if (transition.token.__expiresAt <= Date.now()) {
        logger.error(`❌ [Authority Violation] Token for command: ${id} has expired.`);
      } else {
        /* istanbul ignore next */
        logger.error(`❌ [Authority Violation] Forged token detected for command: ${id}`);
      }
    } else {
      /* istanbul ignore next */
      logger.error(`❌ [Authority Violation] Attempted to dispatch unregistered handle: ${id}`);
    }
  }

  #setupReactiveSync() {
    wireReactiveSync({
        agentLoop: this.agentLoop,
        ui: this.ui,
        getVoiceEngine: () => this.voiceEngine,
        setVoiceEngine: (engine) => { this.voiceEngine = engine; },
        whisperNode: this.whisperNode,
        dispatch: (id, payload) => this.#dispatch(id, payload),
        addTelemetryLog: (level, message) => this.addTelemetryLog(level, message),
        isTtsFallbackActive: () => this.#isTtsFallbackActive,
        setTtsFallbackActive: (active) => { this.#isTtsFallbackActive = active; },
        createFallbackVoiceEngine: () => new KokoroVoiceEngine(),
        onFallbackVoiceEngineCreated: (engine) => {
            engine.on("audio_base64", (base64: string) => {
                this.ui.broadcastUIEvent("ai_audio_chunk", { audio: base64 });
            });
        },
    });
  }

  public async bootstrap() {
    logger.info("🚀 [Orchestrator] Starting Async Distributed Boot Sequence...");
    await Promise.all([
      this.memory.initialize(),
      this.registry.registerLocalSkills(),
      this.registry.whitelist.load()
    ]);
    logger.info("⏳ [Micro-Kernel] Loading Llamas.cpp backend (Distributed Engine)...");
    await this.agentLoop.initModels();
    
    // [DevSecOps] Kích hoạt tiến trình Self-Healing
    this.agentLoop.Orchestrator.startAnomalyDetection();
    
    // [LIVA-UHM] Initialize background memory daemons (ReflectionDaemon + ConsolidationCron)
    try {
        const AI_PROVIDER = process.env.AI_PROVIDER?.toLowerCase() || "local";
        const routerPort = this.agentLoop.Orchestrator.routerPort;
        const uhmClient = new OpenAI({
            baseURL: AI_PROVIDER === "cloud"
/* istanbul ignore next */
                ? (process.env.AI_BASE_URL || "")
                : `http://127.0.0.1:${routerPort}/v1`,
            apiKey: AI_PROVIDER === "cloud"
                ? (process.env.AI_API_KEY || "")
                : "local-ghost-uhm",
            timeout: 30000,
            maxRetries: 1,
        });
        this.memory.initUHM(uhmClient);
        logger.info("[CoreKernel] 🧠 LIVA-UHM daemons initialized (ReflectionDaemon + ConsolidationCron).");
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
        /* istanbul ignore next */
        logger.warn(`[CoreKernel] UHM init failed (non-critical): ${errMsg}`);
    }

    // [v25] Pre-load path/fs for VAD initialization (shared across SmartTurnVAD + VADWorkerBridge)
    const path = await import('path');
    const fs = await import('fs');

    try {
        const modelPath = path.join(process.cwd(), "models", "silero_vad.onnx");
/* istanbul ignore next */
        if (fs.existsSync(modelPath)) {
            this.smartTurnVAD = new SmartTurnVAD();
            await this.smartTurnVAD.initialize(modelPath);
            /* istanbul ignore next */
            logger.info("[CoreKernel] 🎙️ SmartTurnVAD (Edge VAD) initialized successfully.");
        }
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
        /* istanbul ignore next */
        logger.warn(`[CoreKernel] SmartTurnVAD init failed: ${errMsg}`);
    }

    // [v25] Initialize VADWorkerBridge for neural VAD (primary path for speech detection)
    // This replaces the legacy silence-timer approach that caused Whisper spam
    try {
        const { VADWorkerBridge } = await import("../services/VADWorkerBridge");
        const vadModelPath = path.join(process.cwd(), "models", "silero_vad.onnx");
        if (fs.existsSync(vadModelPath)) {
            this.vadBridge = new VADWorkerBridge();
            await this.vadBridge.initialize(vadModelPath);
            logger.info("[CoreKernel] 🎙️ VADWorkerBridge (Neural VAD) initialized successfully.");
        }
    } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
        /* istanbul ignore next */
        logger.warn(`[CoreKernel] VADWorkerBridge init failed (falling back to legacy): ${errMsg}`);
    }

    // Bật App Watcher để LIVA nhận thức được phần mềm cài trên máy
    this.appWatcher.start();

    // Kích hoạt tiến trình quét Semantic GitNexus chạy ngầm
    // ⚡ Boot-time: chỉ chạy analyze cơ bản, KHÔNG --embeddings (opt-in để tránh nghẽn boot)
    this.gitNexusIndexer.triggerIndex();

    // Khởi động Email Client Daemon
    this.emailManager.startIdling().catch(e => logger.error(`[EmailClient] Khởi động thất bại: ${e.message}`));

    // [v24] Pillar 1: Start VRAM Guard + Event Wiring
    await this.vramGuard.loadCustomApps();
    this.vramGuard.start();
    // [v25 FIX] Inject AgentLoop busy checker to prevent VRAMGuard from killing llama-server
    // while AI is actively generating tokens (GPU utilization is naturally high during inference)
    this.vramGuard.setAgentBusyCheck(() => this.agentLoop.isBusy);
    // [v24] Inject VRAMGuard state into AgentLoop → SemanticRouter L0.5 cache
    this.agentLoop.setVramGuardCheck(() => this.vramGuard.isYielded);
    // [v25] Inject VRAMGuard state into EmbeddingService → blocks gRPC embed when GPU yielded
    const { EmbeddingService } = await import("../services/EmbeddingService");
    EmbeddingService.getInstance().setVramGuardCheck(() => this.vramGuard.isYielded);
    this.vramGuard.on("yield_vram", async (payload: { reason: string; appName?: string }) => {
        logger.warn(`[v24 VRAMGuard] 🎮 YIELDING VRAM: ${payload.reason}`);
        this.addTelemetryLog("warn", `VRAM Yielded: ${payload.reason}`);
        this.ui.broadcastUIEvent("system_notification", {
            title: "🎮 VRAM Yielded",
            body: `LIVA đã nhường GPU cho ${payload.appName || "ứng dụng nặng"}. Chuyển sang Cloud AI.`,
            type: "info"
        });
        // Kill local LLM to free VRAM
        await this.agentLoop.Orchestrator.killLlamaServer();
    });
    this.vramGuard.on("reclaim_vram", async (payload: { reason: string }) => {
        logger.info(`[v24 VRAMGuard] ✅ RECLAIMING VRAM: ${payload.reason}`);
        this.addTelemetryLog("info", `VRAM Reclaimed: ${payload.reason}`);
        this.ui.broadcastUIEvent("system_notification", {
            title: "✅ VRAM Reclaimed",
            body: "Game/app đã tắt. Đang hâm nóng lại AI cục bộ...",
            type: "info"
        });
        // Re-warm local model
        await this.agentLoop.Orchestrator.restartRouter();
    });
    this.appWatcher.setCallback(async (appName, skillData) => {
        // Chủ động đánh thức LIVA bằng cách đẩy một system command giả lập
        await this.#dispatch("agent_input", `[System Cognitive Event]: Người dùng vừa cài đặt ứng dụng '${appName}' lên máy tính. Bạn vừa được nạp kỹ năng điều khiển '${skillData.type}' (${skillData.description}). Hãy RẤT HÀO HỨNG khoe với người dùng rằng bạn đã biết họ cài app mới và đề xuất một hành động ngay lập tức! (Không cần xưng hô System)`);
    });

    // Bật nhịp đập tự trị sau khi boot xong
    this.heartbeat.start();

    // --- [v5.0] Remote Control Hub Boot ---
    if (this.securityGateway.isRemoteControlEnabled()) {
      logger.info("📡 [RemoteControl] REMOTE_CONTROL_ENABLED=true — Kích hoạt hệ thống điều khiển từ xa...");

      // 🔒 [Audit C-5] Channel adapters already registered in constructor (line 153-154)
      // Removed duplicate: this.channelRouter.register(this.telegram);

      // Connect Telegram (Long-polling)
      this.telegram.startPolling();

      // Connect Meta (Webhook Server)
      this.meta.startWebhookServer().catch(e => {
        logger.warn(`[RemoteControl] MetaBridge server start failed: ${e.message}`);
      });

      // Connect CDP Bridge to Antigravity (non-blocking, auto-reconnects)
      this.cdpBridge.connect().then(() => {
        logger.info("🔗 [RemoteControl] CDP Bridge connected to Antigravity IDE.");
        this.cdpBridge.watchForApprovalButtons().catch(e =>
          logger.warn(`[CDP] MutationObserver setup failed: ${e.message}`)
        );
      }).catch(e => {
        logger.warn(`[RemoteControl] CDP Bridge initial connect failed (will auto-retry): ${e.message}`);
      });

      // Connect VS Code Bridge (non-blocking, auto-reconnects)
      this.vscodeBridge.connect().then(() => {
        logger.info("🔗 [RemoteControl] VSCode Bridge connected.");
      }).catch(e => {
        logger.warn(`[RemoteControl] VSCode Bridge initial connect failed (will auto-retry): ${e.message}`);
      });

      logger.info(`📡 [RemoteControl] Channels: ${this.channelRouter.getRegisteredChannels().join(", ")}`);
    } else {
      logger.info("🔒 [RemoteControl] Disabled (REMOTE_CONTROL_ENABLED ≠ true). Chỉ sử dụng giao diện cục bộ.");
    }

    logger.info(
      "✅ [Async Distributed Orchestration Kernel] Fully operational. Awaiting Liva connection...",
    );
  }

  /**
   * 🛡️ [Security Hardening] IP Geolocation — OPT-IN only.
   * Set LIVA_GEOLOCATION_ENABLED=true in .env to activate.
   * Default: DISABLED — no external network request is made.
   * 
   * Rationale: Automatic IP geolocation leaks the user's approximate
   * location to a third-party API (ip-api.com) on every boot.
   * This must be an explicit user choice, not a silent default.
   */
  public async fetchSystemLocation() {
    let isGeoEnabled = process.env.LIVA_GEOLOCATION_ENABLED === 'true';

    // 1. NON-BLOCKING I/O & FALLBACK (Rule 4.3)
    try {
        const fsp = await import('node:fs/promises');
        const configPath = await import('node:path').then(p => p.join(process.cwd(), "..", "data", "liva-config.json"));
        const raw = await fsp.readFile(configPath, 'utf-8');
        const config = JSON.parse(raw);
        if (config?.system?.geolocationEnabled !== undefined) {
            isGeoEnabled = Boolean(config.system.geolocationEnabled);
        }
        // Initialize Shadow Digest based on initial config
        this.#handleConfigUpdated(config);
    } catch (e: unknown) {
        const isENOENT = e instanceof Error && 'code' in e && (e as NodeJS.ErrnoException).code === 'ENOENT';
        if (!isENOENT) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.warn(`⚠️ [CoreKernel] Không thể đọc liva-config.json, dự phòng về biến môi trường ENV: ${errMsg}`);
        }
    }

    if (!isGeoEnabled) {
        logger.info("🔒 [System] IP Geolocation is DISABLED (opt-in). Set LIVA_GEOLOCATION_ENABLED=true or enable in Dashboard.");
        return null;
    }

    // 2. SAFE NETWORKING (Rule 4.1 & Rule 6)
    try {
        logger.info("🌍 [System] Performing distributed IP geolocation lookup...");
        const start = Date.now();
        
        // Bắt buộc có timeout 5000ms để không làm treo boot chuỗi
        const ipRes = await safeFetch("http://ip-api.com/json/", { method: 'GET' }, 5000);
        const ipData = await ipRes.json();
        
        this.#currentLatency = Date.now() - start;
        this.#orchestrationTensor.updateWeights([this.#currentLatency]);

        if (ipData && ipData.status === "success") {
/* istanbul ignore next */
          const loc = `City: ${ipData.city || ipData.regionName}, ${ipData.country} (Coords: ${ipData.lat}, ${ipData.lon})`;
          const tz = ipData.timezone || "Asia/Ho_Chi_Minh";
          await this.agentLoop.setSystemLocation(loc, tz);
          logger.info(`📍 [System] Location locked via distributed lookup: ${loc} (${tz})`);
          return ipData;
        } else {
          logger.warn("⚠️ [System] Geolocation failed. Using fallback defaults.");
          return null;
        }
    } catch (e: unknown) {
        // Trích xuất thông báo lỗi bị ẩn của native fetch
        const errMsg = e instanceof Error ? ((e.cause instanceof Error ? e.cause.message : null) || e.message) : String(e);
        logger.error(`⚠️ [System] Không thể kết nối đến máy chủ định vị: ${errMsg}`);
        return null;
    }
  }

  /**
   * Khởi tạo hoặc cập nhật cấu hình của ProactiveDaemon (Shadow Digest)
   */
  #handleConfigUpdated(config: any) {
      if (!config?.system) return;
      
      const setupDaemon = (
        daemon: ProactiveDaemon | null, 
        enabled: boolean, 
        hour: number, 
        minute: number, 
        topicGetter: () => Promise<{ interests: string[], focus: string[] }>,
        deliverUI: boolean,
        deliverTelegram: boolean,
        deliverZalo: boolean,
        deliverEmail: boolean,
        label: string
      ): ProactiveDaemon | null => {
          if (daemon) {
              daemon.dispose();
          }
          if (!enabled) return null;

          const newDaemon = new ProactiveDaemon({
              getTopics: topicGetter,
              isAgentBusy: () => this.agentLoop.isBusy,
              saveBriefing: (briefing) => {
                  const sm = this.memory.getStructuredMemoryInstance();
                  if (sm) sm.saveBriefing(briefing);
              },
              getUnreadCount: () => {
                  const sm = this.memory.getStructuredMemoryInstance();
                  return sm ? sm.getUnreadBriefings().length : 0;
              },
              cleanExpired: () => {
                  const sm = this.memory.getStructuredMemoryInstance();
                  return sm ? sm.cleanExpiredBriefings() : 0;
              },
              pushNotification: (title, body) => {
                  if (deliverUI !== false) {
                      this.ui.broadcastUIEvent("push_notification", { title, body });
                  }
              },
              pushEgress: (content) => {
                  if (deliverTelegram !== false) {
                      const adminId = process.env.TELEGRAM_ADMIN_ID || "";
                      if (adminId) {
                          this.telegram.sendText(adminId, content).catch(() => {});
                      }
                  }
                  if (deliverEmail) {
                      logger.info(`[ProactiveDaemon] 📧 Yêu cầu gửi ${label} qua Email`);
                  }
                  if (deliverZalo) {
                      logger.info(`[ProactiveDaemon] 💬 Yêu cầu gửi ${label} qua Zalo`);
                  }
              },
              isUserOnline: () => this.ui.connectedClientCount > 0
          }, { 
              scheduleHour: Number(hour) || 7, 
              scheduleMinute: Number(minute) || 0 
          });

          newDaemon.start();
          logger.info(`[CoreKernel] 📰 ${label} đã bật (${hour}:${minute})`);
          return newDaemon;
      };

      const {
          digestInterestsEnabled, digestInterestsHour, digestInterestsMinute,
          digestInterestsDeliverUI, digestInterestsDeliverTelegram, digestInterestsDeliverZalo, digestInterestsDeliverEmail,
          digestFocusEnabled, digestFocusHour, digestFocusMinute,
          digestFocusDeliverUI, digestFocusDeliverTelegram, digestFocusDeliverZalo, digestFocusDeliverEmail,
          digestFocusTopics
      } = config.system;

      // 1. Setup Interests Daemon
      this.proactiveInterestsDaemon = setupDaemon(
          this.proactiveInterestsDaemon,
          digestInterestsEnabled, digestInterestsHour, digestInterestsMinute,
          async () => {
              let interests: string[] = [];
              try {
                  const profile = await this.memory.getUserProfile();
                  if (profile?.hobbies?.trim()) {
                      interests.push(...profile.hobbies.split(',').map((s: string) => s.trim()));
                  }
              } catch (e) {
                  logger.warn(`[ProactiveDaemon] Không đọc được User Profile: ${e}`);
              }
              if (interests.length === 0) {
                  const sm = this.memory.getStructuredMemoryInstance();
                  if (sm) {
                      const facts = sm.getAllFacts();
                      interests = facts.filter((f: any) => (f.memoryStrength ?? 1.0) > 0.2).map((f: any) => f.content);
                  }
              }
              return { interests, focus: [] };
          },
          digestInterestsDeliverUI, digestInterestsDeliverTelegram, digestInterestsDeliverZalo, digestInterestsDeliverEmail,
          "Bản tin Sở thích"
      );

      // 2. Setup Focus Daemon
      this.proactiveFocusDaemon = setupDaemon(
          this.proactiveFocusDaemon,
          digestFocusEnabled, digestFocusHour, digestFocusMinute,
          async () => {
              const focus: string[] = [];
              if (digestFocusTopics?.trim()) {
                  focus.push(...digestFocusTopics.split(',').map((s: string) => s.trim()));
              } else {
                  // Fallback for focus is also from L3
                  const sm = this.memory.getStructuredMemoryInstance();
                  if (sm) {
                      const facts = sm.getAllFacts();
                      focus.push(...facts.filter((f: any) => (f.memoryStrength ?? 1.0) > 0.2).map((f: any) => f.content));
                  }
              }
              return { interests: [], focus };
          },
          digestFocusDeliverUI, digestFocusDeliverTelegram, digestFocusDeliverZalo, digestFocusDeliverEmail,
          "Bản tin Mối quan tâm"
      );
  }

  public async shutdown() {
    const safeExecAsync = async (fn: () => any) => { try { await fn(); } catch (e) { void e; } };
    
    // 🚨 BƯỚC 1 (IMMEDIATE): Trảm llama-server.exe để nhả 100% VRAM (Chống Zombie)!
    await safeExecAsync(() => this.agentLoop.Orchestrator.stopRouter());

    // Dọn sạch GC Interval
/* istanbul ignore next */
    if (this.#gcIntervalId) {
      clearInterval(this.#gcIntervalId);
      this.#gcIntervalId = null;
    }
    // 🔒 [Memory Fix #3] Đóng FileWatcher để trả lại system file handle
/* istanbul ignore next */
    if (this.#fileWatcher) {
      await safeExecAsync(() => this.#fileWatcher!.close());
      this.#fileWatcher = null;
      logger.info("[CoreKernel] 🧹 FileWatcher đã được đóng an toàn.");
    }
    await safeExecAsync(() => this.zalo.stop());
    await safeExecAsync(() => this.heartbeat.stop());
    await safeExecAsync(() => this.appWatcher.stop());
    await safeExecAsync(() => this.voiceEngine?.destroy());
    await safeExecAsync(() => this.whisperNode.flush());
    await safeExecAsync(() => this.whisperNode.destroy());
    await safeExecAsync(() => this.smartTurnVAD?.dispose());
    await safeExecAsync(() => this.vadBridge?.dispose());
    await safeExecAsync(() => this.memory.dispose());
    await safeExecAsync(() => SensoryManager.getInstance().dispose());
    await safeExecAsync(() => EmbeddingService.getInstance().dispose());
    await safeExecAsync(() => this.emailManager.dispose());
    await safeExecAsync(() => this.gitNexusIndexer.dispose());
    await safeExecAsync(() => this.proactiveInterestsDaemon?.dispose());
    await safeExecAsync(() => this.proactiveFocusDaemon?.dispose());
    // [v24] VRAM Guard cleanup
    await safeExecAsync(() => this.vramGuard.dispose());
    // 🔒 [Audit H-4] HeraCompass — dispose saveTimeout timer to prevent leak
    await safeExecAsync(() => HeraCompass.getInstance().dispose());
    // [v5.0] Remote Control Hub — Cleanup
    await safeExecAsync(() => this.telegram.stop());
    await safeExecAsync(() => this.meta.stop());
    await safeExecAsync(() => this.cdpBridge.dispose());
    await safeExecAsync(() => this.approvalEngine.dispose());
    await safeExecAsync(() => this.vscodeBridge.dispose());
    await safeExecAsync(() => this.sessions.dispose());
    await safeExecAsync(() => this.registry.whitelist.dispose());
    await safeExecAsync(() => this.agentLoop.shutdown());
    logger.info("[CoreKernel] Hệ thống đã shutdown sạch sẽ.");
  }
}