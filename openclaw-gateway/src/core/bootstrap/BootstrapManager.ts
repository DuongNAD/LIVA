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
        this.#deps.gitNexusIndexer.triggerIndex();

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

        logger.info(
            "✅ [Async Distributed Orchestration Kernel] Fully operational. Awaiting Liva connection...",
        );
    }

    async #initUHM(): Promise<void> {
        try {
            const AI_PROVIDER = process.env.AI_PROVIDER?.toLowerCase() || "local";
            const routerPort = this.#deps.agentLoop.Orchestrator.routerPort;
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
            this.#deps.memory.initUHM(uhmClient);
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
}
