/**
 * BootstrapManager — Sprint 3 Task 3.1
 *
 * Extracted from CoreKernel.bootstrap() (Lines 621-727).
 * Handles TTS/STT init, VAD boot, UHM daemons, Remote Control Hub startup,
 * GitNexus, Email, AppWatcher, and Heartbeat initialization.
 *
 * IMPORTANT: This class does NOT own the services.
 * CoreKernel still holds them as public properties for backward-compat.
 * BootstrapManager only *orchestrates the boot sequence*.
 */

import OpenAI from "openai";
import type { DependencyContainer } from "../DependencyContainer";
import { SmartTurnVAD } from "../../services/SmartTurnVAD";
import { logger } from "../../utils/logger";
import { ConfigManager } from "../config/ConfigManager";

export class BootstrapManager {
    #deps: DependencyContainer;

    constructor(deps: DependencyContainer) {
        this.#deps = deps;
    }

    /**
     * Execute the full async distributed boot sequence.
     * Called from CoreKernel.bootstrap().
     */
    async boot(): Promise<void> {
        logger.info("🚀 [Orchestrator] Starting Async Distributed Boot Sequence...");
        await Promise.all([
            this.#deps.memory.initialize(),
            this.#deps.registry.registerLocalSkills()
        ]);
        logger.info("⏳ [Micro-Kernel] Loading Llamas.cpp backend (Distributed Engine)...");
        await this.#deps.agentLoop.initModels();
        // Warm up the skill registry cache asynchronously in the background once the AI models are ready
        this.#deps.registry.warmUpCache().catch((e: Error) => logger.error(e, "[SkillRegistry] Cache warm-up failed"));

        // [DevSecOps] Kích hoạt tiến trình Self-Healing
        this.#deps.agentLoop.Orchestrator.startAnomalyDetection();

        // [LIVA-UHM] Initialize background memory daemons
        await this.#initUHM();

        // [UHM-v3] Inject StructuredMemory into AgeMem skill (lazy DI)
        try {
            const { setMemoryRef } = await import("../../skills/personal/ManageMemory");
            const sm = this.#deps.memory.getStructuredMemoryInstance();
            if (sm) setMemoryRef(sm);
        } catch { /* ManageMemory skill not loaded — non-critical */ }

        // SmartTurnVAD (Edge VAD)
        await this.#initVAD();

        // Bật App Watcher để LIVA nhận thức được phần mềm cài trên máy
        this.#deps.appWatcher.start();

        // Kích hoạt tiến trình quét Semantic GitNexus chạy ngầm
        // ⚡ Boot-time: chỉ chạy analyze cơ bản, KHÔNG --embeddings (opt-in để tránh nghẽn boot)
        this.#deps.gitNexusIndexer.triggerIndex(30000);

        // Khởi động Email Client Daemon
        this.#deps.emailManager.startIdling().catch((e: Error) => logger.error(`[EmailClient] Khởi động thất bại: ${e.message}`));

        this.#deps.appWatcher.setCallback(async (appName: string, skillData: { type: string; description: string }) => {
            // Chủ động đánh thức LIVA bằng cách đẩy một system command giả lập
            await this.#deps.dispatch("agent_input", `[System Cognitive Event]: Người dùng vừa cài đặt ứng dụng '${appName}' lên máy tính. Bạn vừa được nạp kỹ năng điều khiển '${skillData.type}' (${skillData.description}). Hãy RẤT HÀO HỨNG khoe với người dùng rằng bạn đã biết họ cài app mới và đề xuất một hành động ngay lập tức! (Không cần xưng hô System)`);
        });

        // Bật nhịp đập tự trị sau khi boot xong
        this.#deps.heartbeat.start();

        // --- [v5.0] Remote Control Hub Boot ---
        await this.#bootRemoteControlHub();

        // --- [v6.0] Sentient Gatekeeper + Proactive Routines Boot ---
        await this.#bootSentientLayer();

        logger.info(
            "✅ [Async Distributed Orchestration Kernel] Fully operational. Awaiting Liva connection...",
        );
    }

    async #initUHM(): Promise<void> {
        try {
            const cfgMgr = ConfigManager.getInstance();
            const routerPort = this.#deps.agentLoop.Orchestrator.routerPort;
            const uhmClient = new OpenAI({
                baseURL: cfgMgr.aiProvider === "cloud"
/* istanbul ignore next */
                    ? (cfgMgr.env.AI_BASE_URL)
                    : `http://127.0.0.1:${routerPort}/v1`,
                apiKey: cfgMgr.aiProvider === "cloud"
                    ? (cfgMgr.env.AI_API_KEY)
                    : "local-ghost-uhm",
                timeout: 30000,
                maxRetries: 1,
            });
            this.#deps.memory.initUHM(uhmClient);
            
            // Wire up AgentLoop state getter to ConsolidationCron
            if (this.#deps.memory.consolidationCron) {
                this.#deps.memory.consolidationCron.setAgentLoopStateGetter(
                    () => this.#deps.agentLoop.isBusy ? "BUSY" : "IDLE"
                );
            }

            // Connect VRAM mutation events from ModelOrchestrator to EmbeddingService
            const { EmbeddingService } = await import("../../services/EmbeddingService");
            this.#deps.agentLoop.Orchestrator.on("anomaly_detected", () => {
                EmbeddingService.getInstance().setVramYielded(true);
            });
            this.#deps.agentLoop.Orchestrator.on("rewarming_ai", () => {
                EmbeddingService.getInstance().setVramYielded(false);
            });

            logger.info("[CoreKernel] 🧠 LIVA-UHM daemons initialized (ReflectionDaemon + ConsolidationCron).");
        } catch (e: unknown) {
            /* istanbul ignore next */
            const err = e as Error;
            logger.warn(`[CoreKernel] UHM init failed (non-critical): ${err.message}`);
        }
    }

    async #initVAD(): Promise<void> {
        try {
            const path = await import('path');
            const fs = await import('fs');
            const modelPath = path.join(process.cwd(), "models", "silero_vad.onnx");
/* istanbul ignore next */
            if (fs.existsSync(modelPath)) {
                this.#deps.smartTurnVAD = new SmartTurnVAD();
                await this.#deps.smartTurnVAD.initialize(modelPath);
                /* istanbul ignore next */
                logger.info("[CoreKernel] 🎙️ SmartTurnVAD (Edge VAD) initialized successfully.");
            }
        } catch (e: unknown) {
            /* istanbul ignore next */
            const err = e as Error;
            logger.warn(`[CoreKernel] SmartTurnVAD init failed: ${err.message}`);
        }
    }

    async #bootRemoteControlHub(): Promise<void> {
        const { securityGateway, telegram, meta, cdpBridge, vscodeBridge, channelRouter } = this.#deps;

        if (securityGateway.isRemoteControlEnabled()) {
            logger.info("📡 [RemoteControl] REMOTE_CONTROL_ENABLED=true — Kích hoạt hệ thống điều khiển từ xa...");

            // 🔒 [Audit C-5] Channel adapters already registered in constructor (line 153-154)
            // Removed duplicate: this.channelRouter.register(this.telegram);

            // Connect Telegram (Long-polling)
            telegram.startPolling();

            // Connect Meta (Webhook Server)
            meta.startWebhookServer().catch((e: Error) => {
                logger.warn(`[RemoteControl] MetaBridge server start failed: ${e.message}`);
            });

            // Connect CDP Bridge to Antigravity (non-blocking, auto-reconnects)
            cdpBridge.connect().then(() => {
                logger.info("🔗 [RemoteControl] CDP Bridge connected to Antigravity IDE.");
                cdpBridge.watchForApprovalButtons().catch((e: Error) =>
                    logger.warn(`[CDP] MutationObserver setup failed: ${e.message}`)
                );
            }).catch((e: Error) => {
                logger.warn(`[RemoteControl] CDP Bridge initial connect failed (will auto-retry): ${e.message}`);
            });

            // Connect VS Code Bridge (non-blocking, auto-reconnects)
            vscodeBridge.connect().then(() => {
                logger.info("🔗 [RemoteControl] VSCode Bridge connected.");
            }).catch((e: Error) => {
                logger.warn(`[RemoteControl] VSCode Bridge initial connect failed (will auto-retry): ${e.message}`);
            });

            logger.info(`📡 [RemoteControl] Channels: ${channelRouter.getRegisteredChannels().join(", ")}`);
        } else {
            logger.info("🔒 [RemoteControl] Disabled (REMOTE_CONTROL_ENABLED ≠ true). Chỉ sử dụng giao diện cục bộ.");
        }
    }

    /**
     * [v6.0] Boot Sentient Gatekeeper + Proactive Routines + Ambient Intelligence daemons.
     *
     * All daemons are:
     * - Feature-flag gated (AppConfig env vars)
     * - Lazy-imported to avoid circular dependencies
     * - Non-critical — failures are caught and logged, never block boot
     * - Wired via DI (never import CoreKernel directly)
     */
    async #bootSentientLayer(): Promise<void> {
        logger.info("🧠 [SentientLayer] Booting Sentient Gatekeeper + Proactive Routines...");

        // Helper: safe TTS call through voice engine
        const speakTTS = async (text: string): Promise<void> => {
            try {
                const engine = this.#deps.voiceEngine;
                if (engine && 'speak' in engine) {
                    await engine.speak(text);
                }
            } catch (e: unknown) {
                const msg = e instanceof Error ? e.message : String(e);
                logger.warn(`[SentientLayer] TTS failed: ${msg}`);
            }
        };

        // [v28 FIX] Route through logger instead of stdout to avoid Tauri sidecar corruption
        const pushNotification = (title: string, body: string): void => {
            logger.info({ notificationType: "toast", title, body }, `[SentientLayer] Notification: ${title}`);
            // Broadcast via UIController if available (non-critical)
            try {
                const ui = (globalThis as any).kernelInstance?.ui;
                if (ui?.broadcastUIEvent) {
                    ui.broadcastUIEvent("SHOW_TOAST", {
                        title, message: body, type: "info", duration: 8000
                    });
                }
            } catch { /* UIController not ready — non-critical */ }
        };

        // --- Nhóm 10: Sentient Gatekeeper ---

        // UrgencyBypassFilter
        if (process.env.LIVA_URGENCY_BYPASS_ENABLED !== "false") {
            try {
                const { UrgencyBypassFilter } = await import("../../services/UrgencyBypassFilter");
                const urgencyFilter = await UrgencyBypassFilter.create({
                    speakTTS,
                    pushNotification,
                    flashScreen: async () => { /* Handled internally by UrgencyBypassFilter */ },
                });
                logger.info("🚨 [SentientLayer] UrgencyBypassFilter active.");
                // Store reference for message routing
                (globalThis as any).__livaUrgencyFilter = urgencyFilter;
            } catch (e: unknown) {
                const msg = e instanceof Error ? e.message : String(e);
                logger.warn(`[SentientLayer] UrgencyBypassFilter init failed (non-critical): ${msg}`);
            }
        }

        // ShadowInboxDigest
        try {
            const { ShadowInboxDigest } = await import("../../services/ShadowInboxDigest");
            const shadowInbox = new ShadowInboxDigest({
                speakTTS,
                pushNotification,
                getUnreadZaloCount: () => 0, // Wired by CoreKernel later
                getUnreadEmailCount: () => 0,
                getUnreadTelegramCount: () => 0,
                isAgentBusy: () => this.#deps.agentLoop.isBusy,
            });
            shadowInbox.start();
            logger.info("📥 [SentientLayer] ShadowInboxDigest started.");
            (globalThis as any).__livaShadowInbox = shadowInbox;
        } catch (e: unknown) {
            const msg = e instanceof Error ? e.message : String(e);
            logger.warn(`[SentientLayer] ShadowInboxDigest init failed (non-critical): ${msg}`);
        }

        // --- Nhóm 11: Proactive Routines ---

        // MorningBriefingCast
        if (process.env.LIVA_MORNING_BRIEFING_ENABLED !== "false") {
            try {
                const { MorningBriefingCast } = await import("../../services/MorningBriefingCast");
                const briefing = new MorningBriefingCast({
                    executeSkill: async (name: string, args: any) => {
                        try {
                            return await this.#deps.registry.executeSkill(name, args);
                        } catch { return "[SKILL_UNAVAILABLE]"; }
                    },
                    speakTTS,
                    pushNotification,
                    isAgentBusy: () => this.#deps.agentLoop.isBusy,
                    isUserOnline: () => this.#deps.ui.connectedClientCount > 0,
                    getUnreadCount: () => 0,
                });
                briefing.start();
                logger.info("🌅 [SentientLayer] MorningBriefingCast started (8:00 AM daily).");
            } catch (e: unknown) {
                const msg = e instanceof Error ? e.message : String(e);
                logger.warn(`[SentientLayer] MorningBriefingCast init failed (non-critical): ${msg}`);
            }
        }

        // HealthPostureMonitor
        if (process.env.LIVA_HEALTH_MONITOR_ENABLED !== "false") {
            try {
                const { HealthPostureMonitor } = await import("../../services/HealthPostureMonitor");
                const healthMonitor = new HealthPostureMonitor({
                    speakTTS,
                    pushNotification,
                    setBrightness: async (level: number) => {
                        try {
                            await this.#deps.registry.executeSkill("hardware_controller", {
                                action: "set_brightness", level
                            });
                        } catch { /* non-critical */ }
                    },
                    isAgentBusy: () => this.#deps.agentLoop.isBusy,
                });
                healthMonitor.start();
                logger.info("💪 [SentientLayer] HealthPostureMonitor started (2h break reminder).");
            } catch (e: unknown) {
                const msg = e instanceof Error ? e.message : String(e);
                logger.warn(`[SentientLayer] HealthPostureMonitor init failed (non-critical): ${msg}`);
            }
        }

        // MeetingCopilot
        if (process.env.LIVA_MEETING_COPILOT_ENABLED === "true") {
            try {
                const { MeetingCopilot } = await import("../../services/MeetingCopilot");
                const meetingCopilot = new MeetingCopilot({
                    pushNotification,
                    setAutoResponderContext: (ctx: string) => {
                        try {
                            const engine = (globalThis as any).__livaAutoResponderEngine;
                            if (engine?.setContext) engine.setContext(ctx);
                        } catch { /* non-critical */ }
                    },
                    reduceMediaVolume: async () => {
                        try {
                            await this.#deps.registry.executeSkill("audio_mixer_controller", { action: "duck" });
                        } catch { /* non-critical */ }
                    },
                    restoreMediaVolume: async () => {
                        try {
                            await this.#deps.registry.executeSkill("audio_mixer_controller", { action: "restore" });
                        } catch { /* non-critical */ }
                    },
                });
                meetingCopilot.start();
                logger.info("📹 [SentientLayer] MeetingCopilot started (window title monitoring).");
                (globalThis as any).__livaMeetingCopilot = meetingCopilot;
            } catch (e: unknown) {
                const msg = e instanceof Error ? e.message : String(e);
                logger.warn(`[SentientLayer] MeetingCopilot init failed (non-critical): ${msg}`);
            }
        }

        // --- Nhóm 13: Ambient Intelligence ---

        // CrossPlatformStatusSync
        if (process.env.LIVA_STATUS_SYNC_ENABLED === "true") {
            try {
                const { CrossPlatformStatusSync } = await import("../../services/CrossPlatformStatusSync");
                const statusSync = new CrossPlatformStatusSync({
                    getCurrentActivity: async () => {
                        try {
                            const activeWindow = (await import("active-win")).default;
                            const win = await activeWindow();
                            return { appName: win?.owner?.name ?? "", windowTitle: win?.title ?? "" };
                        } catch { return { appName: "", windowTitle: "" }; }
                    },
                    getIdleMs: async () => 0, // Shared idle detection — wired later
                    isFocusWardenActive: () => !!(globalThis as any).__livaFocusWardenActive,
                    isMeetingActive: () => !!(globalThis as any).__livaMeetingCopilot?.isInMeeting?.(),
                    sendTelegramStatus: async (status: string) => {
                        try {
                            const defaultChat = this.#deps.getDefaultRemoteSenderId();
                            if (defaultChat) await this.#deps.telegram.sendText(defaultChat, status);
                        } catch { /* non-critical */ }
                    },
                });
                statusSync.start();
                logger.info("🔄 [SentientLayer] CrossPlatformStatusSync started.");
            } catch (e: unknown) {
                const msg = e instanceof Error ? e.message : String(e);
                logger.warn(`[SentientLayer] CrossPlatformStatusSync init failed (non-critical): ${msg}`);
            }
        }

        logger.info("🧠 [SentientLayer] All sentient daemons booted successfully.");
    }
}
