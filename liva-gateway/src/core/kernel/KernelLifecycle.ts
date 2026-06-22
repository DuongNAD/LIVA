import type { CoreKernel } from "../CoreKernel";
import { logger } from "../../utils/logger";
import { ConfigManager } from "../config/ConfigManager";
import OpenAI from "openai";
import * as path from "node:path";
import * as fs from "node:fs";
import * as fsp from "node:fs/promises";
import { safeFetch } from "../../utils/HttpClient";
import { SensoryManager } from "../../memory/SensoryManager";
import { EmbeddingService } from "../../services/EmbeddingService";
import { TokenCompressionService } from "../../memory/TokenCompressionService";
import { HeraCompass } from "../../memory/HeraCompass";
import { ProactiveDaemon } from "../../services/ProactiveDaemon";
import type { StructuredFact } from "../../memory/StructuredMemory";

export class KernelLifecycle {
  public static async bootstrap(kernel: CoreKernel): Promise<void> {
    logger.info("🚀 [Orchestrator] Starting Async Distributed Boot Sequence...");
    await Promise.all([
      kernel.memory.initialize(),
      kernel.registry.registerLocalSkills(),
      kernel.registry.whitelist.load(),
    ]);
    logger.info("⏳ [Micro-Kernel] Loading Llamas.cpp backend (Distributed Engine)...");
    await kernel.agentLoop.initModels();

    // ⚡ [PERF M16] Fire immediately — models already loaded, no need to defer 5s
    setImmediate(() => {
      logger.info("[CoreKernel] Kích hoạt tiến trình nền: Nạp bộ nhớ đệm kỹ năng...");
      kernel.registry.warmUpCache().catch((e: Error) => logger.error(e, "[SkillRegistry] Cache warm-up failed"));
    });

    // [DevSecOps] Kích hoạt tiến trình Self-Healing
    kernel.agentLoop.Orchestrator.startAnomalyDetection();

    // [LIVA-UHM] Initialize background memory daemons (ReflectionDaemon + ConsolidationCron)
    try {
      const cfgMgr = ConfigManager.getInstance();
      const { livaEngine } = await import("../../utils/LivaEngine");
      const uhmClient =
        cfgMgr.aiProvider === "cloud"
          ? new OpenAI({
              baseURL: cfgMgr.env.AI_BASE_URL,
              apiKey: cfgMgr.env.AI_API_KEY,
              timeout: 30000,
              maxRetries: 1,
            })
          : (livaEngine as unknown as OpenAI);
      kernel.memory.initUHM(uhmClient);
      logger.info("[CoreKernel] 🧠 LIVA-UHM daemons initialized (ReflectionDaemon + ConsolidationCron).");
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      /* istanbul ignore next */
      logger.warn(`[CoreKernel] UHM init failed (non-critical): ${errMsg}`);
    }

    // [v25] Initialize VADWorkerBridge for neural VAD
    try {
      const { VADWorkerBridge } = await import("../../services/VADWorkerBridge");
      const vadModelPath = path.join(process.cwd(), "models", "nemotron-asr", "silero_vad.onnx");
      if (fs.existsSync(vadModelPath)) {
        const bridge = new VADWorkerBridge();
        await bridge.initialize(vadModelPath);
        kernel.vadBridge = bridge;
        kernel.eventRouter.registerVadBridge(bridge);
        logger.info("[CoreKernel] 🎙️ VADWorkerBridge (Neural VAD) initialized successfully.");
      }
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      /* istanbul ignore next */
      logger.warn(`[CoreKernel] VADWorkerBridge init failed (falling back to legacy): ${errMsg}`);
    }

    // Bật App Watcher để LIVA nhận thức được phần mềm cài trên máy
    kernel.appWatcher.start();

    // Kích hoạt tiến trình quét Semantic GitNexus chạy ngầm
    kernel.gitNexusIndexer.triggerIndex();

    // Khởi động Email Client Daemon
    kernel.emailManager.startIdling().catch((e: Error) => logger.error(`[EmailClient] Khởi động thất bại: ${e.message}`));

    // [v26 Phase 3] Hardware Decoupling: VRAMGuard logic moved to Python Daemon.
    kernel.appWatcher.setCallback(async (appName, skillData) => {
      // Chủ động đánh thức LIVA bằng cách đẩy một system command giả lập
      await kernel.dispatch(
        "agent_input",
        `[System Cognitive Event]: Người dùng vừa cài đặt ứng dụng '${appName}' lên máy tính. Bạn vừa được nạp kỹ năng điều khiển '${skillData.type}' (${skillData.description}). Hãy RẤT HÀO HỨNG khoe với người dùng rằng bạn đã biết họ cài app mới và đề xuất một hành động ngay lập tức! (Không cần xưng hô System)`
      );
    });

    // Bật nhịp đập tự trị sau khi boot xong
    kernel.heartbeat.start();
    kernel.powerMonitor.start();

    // --- [v5.0] Remote Control Hub Boot ---
    if (kernel.securityGateway.isRemoteControlEnabled()) {
      logger.info("📡 [RemoteControl] REMOTE_CONTROL_ENABLED=true — Kích hoạt hệ thống điều khiển từ xa...");

      // Connect Telegram (Long-polling)
      kernel.telegram.startPolling();

      // Connect Meta (Webhook Server)
      kernel.meta.startWebhookServer().catch((e: Error) => {
        logger.warn(`[RemoteControl] MetaBridge server start failed: ${e.message}`);
      });

      // Connect CDP Bridge to Antigravity (non-blocking, auto-reconnects)
      kernel.cdpBridge
        .connect()
        .then(() => {
          logger.info("🔗 [RemoteControl] CDP Bridge connected to Antigravity IDE.");
          kernel.cdpBridge.watchForApprovalButtons().catch((e: Error) =>
            logger.warn(`[CDP] MutationObserver setup failed: ${e.message}`)
          );
        })
        .catch((e: Error) => {
          logger.warn(`[RemoteControl] CDP Bridge initial connect failed (will auto-retry): ${e.message}`);
        });

      // Connect VS Code Bridge (non-blocking, auto-reconnects)
      kernel.vscodeBridge
        .connect()
        .then(() => {
          logger.info("🔗 [RemoteControl] VSCode Bridge connected.");
        })
        .catch((e: Error) => {
          logger.warn(`[RemoteControl] VSCode Bridge initial connect failed (will auto-retry): ${e.message}`);
        });

      logger.info(`📡 [RemoteControl] Channels: ${kernel.channelRouter.getRegisteredChannels().join(", ")}`);
    } else {
      logger.info("🔒 [RemoteControl] Disabled (REMOTE_CONTROL_ENABLED ≠ true). Chỉ sử dụng giao diện cục bộ.");
    }

    logger.info("✅ [Async Distributed Orchestration Kernel] Fully operational. Awaiting Liva connection...");
  }

  public static async yieldVRAM(kernel: CoreKernel): Promise<void> {
    if (kernel.isVramYielded) return;
    kernel.isVramYielded = true;
    logger.warn("[CoreKernel] 🔋 Battery mode active. Preemptively yielding VRAM...");
    await kernel.agentLoop.Orchestrator.killLlamaServer();
  }

  public static async reclaimVRAM(kernel: CoreKernel): Promise<void> {
    if (!kernel.isVramYielded) return;
    kernel.isVramYielded = false;
    logger.info("[CoreKernel] 🔌 AC power restored. Reclaiming VRAM...");
    await kernel.agentLoop.initModels();
  }

  public static async fetchSystemLocation(kernel: CoreKernel): Promise<unknown> {
    let isGeoEnabled = process.env.LIVA_GEOLOCATION_ENABLED === "true";

    try {
      const configPath = path.join(process.cwd(), "..", "data", "liva-config.json");
      const raw = await fsp.readFile(configPath, "utf-8");
      const config = JSON.parse(raw);
      if (config?.system?.geolocationEnabled !== undefined) {
        isGeoEnabled = Boolean(config.system.geolocationEnabled);
      }
      KernelLifecycle.handleConfigUpdated(kernel, config);
    } catch (e: unknown) {
      const isENOENT = e instanceof Error && "code" in e && (e as NodeJS.ErrnoException).code === "ENOENT";
      if (!isENOENT) {
        const errMsg = e instanceof Error ? e.message : String(e);
        logger.warn(`⚠️ [CoreKernel] Không thể đọc liva-config.json, dự phòng về biến môi trường ENV: ${errMsg}`);
      }
    }

    if (!isGeoEnabled) {
      logger.info("🔒 [System] IP Geolocation is DISABLED (opt-in). Set LIVA_GEOLOCATION_ENABLED=true or enable in Dashboard.");
      return null;
    }

    try {
      logger.info("🌍 [System] Performing distributed IP geolocation lookup...");
      const start = Date.now();

      const ipRes = await safeFetch("http://ip-api.com/json/", { method: "GET" }, 5000);
      const ipData = await ipRes.json();

      kernel.currentLatency = Date.now() - start;
      kernel.orchestrationTensor.updateWeights([kernel.currentLatency]);

      if (ipData && ipData.status === "success") {
        const loc = `City: ${ipData.city || ipData.regionName}, ${ipData.country} (Coords: ${ipData.lat}, ${ipData.lon})`;
        const tz = ipData.timezone || "Asia/Ho_Chi_Minh";
        await kernel.agentLoop.setSystemLocation(loc, tz);
        logger.info(`📍 [System] Location locked via distributed lookup: ${loc} (${tz})`);
        return ipData;
      } else {
        logger.warn("⚠️ [System] Geolocation failed. Using fallback defaults.");
        return null;
      }
    } catch (e: unknown) {
      const errMsg =
        e instanceof Error
          ? (e.cause instanceof Error ? e.cause.message : null) || e.message
          : String(e);
      logger.error(`⚠️ [System] Không thể kết nối đến máy chủ định vị: ${errMsg}`);
      return null;
    }
  }

  public static startGarbageCollection(kernel: CoreKernel): NodeJS.Timeout {
    const interval = setInterval(() => {
      const now = Date.now();
      let cleanedCount = 0;

      for (const [id, schema] of kernel.getTransitionSchema().entries()) {
        if (schema.token.__expiresAt < now) {
          kernel.getTransitionSchema().delete(id);
          cleanedCount++;
        }
      }

      if (cleanedCount > 0) {
        logger.info(`[GC] Cleaned ${cleanedCount} expired CommandTokens from CoreKernel.`);
      }

      if (global.gc) {
        global.gc();
      }
    }, 60000);
    interval.unref();
    return interval;
  }

  public static async prewarmBrowsers(_kernel: CoreKernel): Promise<void> {
    logger.info("🚀 [CoreKernel] Bắt đầu khởi động ngầm trình duyệt RPA Zalo và Messenger để tối ưu tốc độ / Starting background Playwright RPA pre-warming...");
    try {
      const { getOrCreateBrowser, getActivePage } = await import("../../utils/PlaywrightBrowser");

      Promise.all([
        getOrCreateBrowser("zalo")
          .then(async ({ context }) => {
            logger.info("[CoreKernel] 🌐 Trình duyệt Zalo đã khởi động. Đang tải trước Zalo Web / Zalo browser active. Pre-navigating to Zalo Web...");
            const page = await getActivePage(context, "zalo.me");
            if (!page.url().includes("zalo.me")) {
              await page.goto("https://chat.zalo.me/", { waitUntil: "domcontentloaded", timeout: 60000 }).catch(() => {});
            }
            logger.info("[CoreKernel] ✅ Đã tải trước Zalo Web / Zalo Web pre-loaded in background.");
            try {
              const cdp = await page.context().newCDPSession(page);
              const { windowId } = (await cdp.send("Browser.getWindowForTarget")) as { windowId: number };
              await cdp.send("Browser.setWindowBounds", { windowId, bounds: { windowState: "minimized" } });
            } catch (e) {
              void e;
            }
          })
          .catch((e: Error) => {
            logger.warn(`[CoreKernel] Không thể pre-warm Zalo browser: ${e.message}`);
          }),

        getOrCreateBrowser("messenger")
          .then(async ({ context }) => {
            logger.info("[CoreKernel] 🌐 Trình duyệt Messenger đã khởi động. Đang tải trước Messenger Web / Messenger browser active. Pre-navigating to Messenger Web...");
            const page = await getActivePage(context, "facebook.com/messages");
            if (!page.url().includes("facebook.com/messages")) {
              await page.goto("https://www.facebook.com/messages", { waitUntil: "domcontentloaded", timeout: 60000 }).catch(() => {});
            }
            logger.info("[CoreKernel] ✅ Đã tải trước Messenger Web / Messenger Web pre-loaded in background.");
            try {
              const cdp = await page.context().newCDPSession(page);
              const { windowId } = (await cdp.send("Browser.getWindowForTarget")) as { windowId: number };
              await cdp.send("Browser.setWindowBounds", { windowId, bounds: { windowState: "minimized" } });
            } catch (e) {
              void e;
            }
          })
          .catch((e: Error) => {
            logger.warn(`[CoreKernel] Không thể pre-warm Messenger browser: ${e.message}`);
          }),
      ]).catch(() => {});
    } catch (err: unknown) {
      const errMsg = err instanceof Error ? err.message : String(err);
      logger.error(`[CoreKernel] Pre-warming failed: ${errMsg}`);
    }
  }

  public static handleConfigUpdated(kernel: CoreKernel, config: Record<string, unknown> | null | undefined): void {
    if (!config?.system) return;

    const setupDaemon = (
      daemon: ProactiveDaemon | null,
      enabled: boolean,
      hour: number,
      minute: number,
      topicGetter: () => Promise<{ interests: string[]; focus: string[] }>,
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

      const newDaemon = new ProactiveDaemon(
        {
          getTopics: topicGetter,
          isAgentBusy: () => kernel.agentLoop.isBusy,
          saveBriefing: (briefing) => {
            const sm = kernel.memory.getStructuredMemoryInstance();
            if (sm) sm.saveBriefing(briefing);
          },
          getUnreadCount: () => {
            const sm = kernel.memory.getStructuredMemoryInstance();
            return sm ? sm.getUnreadBriefings().length : 0;
          },
          cleanExpired: () => {
            const sm = kernel.memory.getStructuredMemoryInstance();
            return sm ? sm.cleanExpiredBriefings() : 0;
          },
          pushNotification: (title, body) => {
            if (deliverUI !== false) {
              kernel.ui.broadcastUIEvent("push_notification", { title, body });
            }
          },
          pushEgress: (content) => {
            if (deliverTelegram !== false) {
              const adminId = process.env.TELEGRAM_ADMIN_ID || "";
              if (adminId) {
                kernel.telegram.sendText(adminId, content).catch(() => {});
              }
            }
            if (deliverEmail) {
              logger.info(`[ProactiveDaemon] 📧 Yêu cầu gửi ${label} qua Email`);
            }
            if (deliverZalo) {
              logger.info(`[ProactiveDaemon] 💬 Yêu cầu gửi ${label} qua Zalo`);
            }
          },
          isUserOnline: () => kernel.ui.connectedClientCount > 0,
        },
        {
          scheduleHour: Number(hour) || 7,
          scheduleMinute: Number(minute) || 0,
        }
      );

      newDaemon.start();
      logger.info(`[CoreKernel] 📰 ${label} đã bật (${hour}:${minute})`);
      return newDaemon;
    };

    const systemConfig = (config?.system || {}) as {
      digestInterestsEnabled?: boolean;
      digestInterestsHour?: number;
      digestInterestsMinute?: number;
      digestInterestsDeliverUI?: boolean;
      digestInterestsDeliverTelegram?: boolean;
      digestInterestsDeliverZalo?: boolean;
      digestInterestsDeliverEmail?: boolean;
      digestFocusEnabled?: boolean;
      digestFocusHour?: number;
      digestFocusMinute?: number;
      digestFocusDeliverUI?: boolean;
      digestFocusDeliverTelegram?: boolean;
      digestFocusDeliverZalo?: boolean;
      digestFocusDeliverEmail?: boolean;
      digestFocusTopics?: string;
    };

    const {
      digestInterestsEnabled = false,
      digestInterestsHour = 7,
      digestInterestsMinute = 0,
      digestInterestsDeliverUI = true,
      digestInterestsDeliverTelegram = true,
      digestInterestsDeliverZalo = true,
      digestInterestsDeliverEmail = true,
      digestFocusEnabled = false,
      digestFocusHour = 7,
      digestFocusMinute = 0,
      digestFocusDeliverUI = true,
      digestFocusDeliverTelegram = true,
      digestFocusDeliverZalo = true,
      digestFocusDeliverEmail = true,
      digestFocusTopics = "",
    } = systemConfig;

    // 1. Setup Interests Daemon
    kernel.proactiveInterestsDaemon = setupDaemon(
      kernel.proactiveInterestsDaemon,
      digestInterestsEnabled,
      digestInterestsHour,
      digestInterestsMinute,
      async () => {
        let interests: string[] = [];
        try {
          const profile = await kernel.memory.getUserProfile();
          if (profile?.hobbies && typeof profile.hobbies === "string" && profile.hobbies.trim()) {
            interests.push(...profile.hobbies.split(",").map((s: string) => s.trim()));
          }
        } catch (e) {
          logger.warn(`[ProactiveDaemon] Không đọc được User Profile: ${e}`);
        }
        if (interests.length === 0) {
          const sm = kernel.memory.getStructuredMemoryInstance();
          if (sm) {
            const facts = sm.getAllFacts();
            interests = facts
              .filter((f: StructuredFact) => (f.memoryStrength ?? 1.0) > 0.2)
              .map((f: StructuredFact) => f.value);
          }
        }
        return { interests, focus: [] };
      },
      digestInterestsDeliverUI,
      digestInterestsDeliverTelegram,
      digestInterestsDeliverZalo,
      digestInterestsDeliverEmail,
      "Bản tin Sở thích"
    );

    // 2. Setup Focus Daemon
    kernel.proactiveFocusDaemon = setupDaemon(
      kernel.proactiveFocusDaemon,
      digestFocusEnabled,
      digestFocusHour,
      digestFocusMinute,
      async () => {
        const focus: string[] = [];
        if (digestFocusTopics?.trim()) {
          focus.push(...digestFocusTopics.split(",").map((s: string) => s.trim()));
        } else {
          const sm = kernel.memory.getStructuredMemoryInstance();
          if (sm) {
            const facts = sm.getAllFacts();
            focus.push(
              ...facts
                .filter((f: StructuredFact) => (f.memoryStrength ?? 1.0) > 0.2)
                .map((f: StructuredFact) => f.value)
            );
          }
        }
        return { interests: [], focus };
      },
      digestFocusDeliverUI,
      digestFocusDeliverTelegram,
      digestFocusDeliverZalo,
      digestFocusDeliverEmail,
      "Bản tin Mối quan tâm"
    );
  }

  public static async shutdown(kernel: CoreKernel): Promise<void> {
    kernel.eventRouter.unregisterAll();

    const safeExecAsync = async (fn: () => unknown) => {
      try {
        await fn();
      } catch {
        /* ignore */
      }
    };

    // 🚨 BƯỚC 1 (IMMEDIATE): Trảm llama-server.exe để nhả 100% VRAM (Chống Zombie)!
    await safeExecAsync(() => kernel.agentLoop.Orchestrator.killLlamaServer());

    await safeExecAsync(() => kernel.presenceDetector.stop());

    // Dọn sạch GC Interval
    const gcIntervalId = kernel.getGcIntervalId();
    if (gcIntervalId) {
      clearInterval(gcIntervalId);
      kernel.setGcIntervalId(null);
    }

    // Đóng FileWatcher từ skillOrchestrator
    if (kernel.skillOrchestrator) {
      await safeExecAsync(() => kernel.skillOrchestrator.stopWatcher());
    }

    await safeExecAsync(() => kernel.zalo.stop());
    await safeExecAsync(() => kernel.heartbeat.stop());
    await safeExecAsync(() => kernel.appWatcher.stop());
    await safeExecAsync(() => kernel.powerMonitor.stop());
    await safeExecAsync(() => kernel.voiceOrchestrator.dispose());
    await safeExecAsync(() => kernel.memory.dispose());
    await safeExecAsync(() => SensoryManager.getInstance().dispose());
    await safeExecAsync(() => EmbeddingService.getInstance().dispose());
    await safeExecAsync(() => kernel.emailManager.dispose());
    await safeExecAsync(() => kernel.gitNexusIndexer.dispose());
    await safeExecAsync(() => kernel.proactiveInterestsDaemon?.dispose());
    await safeExecAsync(() => kernel.proactiveFocusDaemon?.dispose());
    await safeExecAsync(() => HeraCompass.getInstance().dispose());
    await safeExecAsync(() => TokenCompressionService.getInstance().dispose());
    await safeExecAsync(() => kernel.telegram.stop());
    await safeExecAsync(() => kernel.meta.stop());
    await safeExecAsync(() => kernel.cdpBridge.dispose());
    await safeExecAsync(() => kernel.approvalEngine.dispose());
    await safeExecAsync(() => kernel.vscodeBridge.dispose());
    await safeExecAsync(() => kernel.sessions.dispose());
    await safeExecAsync(() => kernel.registry.whitelist.dispose());
    await safeExecAsync(() => kernel.agentLoop.shutdown());
    logger.info("[CoreKernel] Hệ thống đã shutdown sạch sẽ.");
  }
}
