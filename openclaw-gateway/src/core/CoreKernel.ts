import { UIController } from "./UIController";
import { AgentLoop } from "./AgentLoop";
import { MemoryManager } from "../MemoryManager";
import { SkillRegistry } from "../SkillRegistry";
import { ZaloPolling } from "./ZaloPolling"; 
import { VoiceEngine } from "../services/VoiceEngine";
import { KokoroVoiceEngine } from "../services/KokoroVoiceEngine";
import { WhisperNode } from "../services/WhisperNode";
import { WhisperJSNode } from "../services/WhisperJSNode";
import { EmbeddingService } from "../services/EmbeddingService";
import { SensoryManager } from "../memory/SensoryManager";
import { safeFetch } from "../utils/HttpClient";
import { logger } from "../utils/logger";
import { HeartbeatManager } from "./HeartbeatManager";
import { AppWatcherService } from "../services/AppWatcherService";

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
  public voiceEngine: VoiceEngine | KokoroVoiceEngine;
  public whisperNode: WhisperNode | WhisperJSNode;
  public heartbeat: HeartbeatManager;
  public appWatcher: AppWatcherService;

  // Hard Private Members (Opague Engine Isolation via #)
  #orchestrationTensor: ReactiveStateTensor;
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

  /**
   * @private_factory
   * Mints non-forgeable branded handles with TTL.
   */
  #mintCommandToken<T extends string, Status extends string>(id: T, ttl: number = this.DEFAULT_TTL): CommandToken<T, Status> {
    return {
      __id: id,
      __authority: true as unknown as KernelAuthority,
      __expiresAt: Date.now() + ttl,
      __brand_identity: "" as any 
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
    // TTS Engine Selection: Kokoro-JS (zero-Python) > Python Edge-TTS
    const forceMode = process.env.LIVA_TTS_ENGINE;
    if (forceMode === 'python') {
      logger.info(`🗣️ [CoreKernel] TTS Engine: Python Edge-TTS (forced via LIVA_TTS_ENGINE=python)`);
      this.voiceEngine = new VoiceEngine();
    } else {
      logger.info(`🗣️ [CoreKernel] TTS Engine: Kokoro-JS (ONNX, zero-Python). Set LIVA_TTS_ENGINE=python to override.`);
      this.voiceEngine = new KokoroVoiceEngine();
    }
    // STT Engine Selection: WhisperJS (zero-Python) > WhisperNode (HTTP)
    const sttMode = process.env.LIVA_STT_ENGINE;
    if (sttMode === 'http') {
      logger.info(`👂 [CoreKernel] STT Engine: WhisperNode HTTP (forced via LIVA_STT_ENGINE=http)`);
      this.whisperNode = new WhisperNode();
    } else {
      logger.info(`👂 [CoreKernel] STT Engine: WhisperJS (ONNX, zero-Python). Set LIVA_STT_ENGINE=http to override.`);
      this.whisperNode = new WhisperJSNode();
    }

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

    // --- MICRO-TASK ORCHESTRATION FLOW ---
    this.ui.on("user_input", async (userText: string) => {
      const weight = this.#orchestrationTensor.getWeight(this.#currentLatency);
      await this.#dispatch("agent_input", userText);
      if (weight <= 0.2) {
        logger.warn(`⚠️ [Orchestrator] High latency (${this.#currentLatency}ms). Proceeding anyway.`);
      }
    });

    this.zalo.on("zalo_incoming", async (userText: string) => {
      await this.#dispatch("agent_input", userText);
    });

    // --- AUDIO PIPELINE (ZERO-LATENCY) ---
    this.ui.on("audio_input", (buffer: Buffer) => {
      this.whisperNode.pushAudioChunk(buffer);
    });

    this.ui.on("interrupt", () => {
      logger.warn(`[CoreKernel] 🛑 Bắt lệnh NGẮT LỜI từ UI. Đóng băng Thanh quản và rỗng não!`);
      this.voiceEngine.preempt();
      this.whisperNode.flush();
    });

    // --- Z-MAS EVENT PIPELINE ---
    this.agentLoop.Orchestrator.on("suspend_peripherals", () => {
      logger.warn(`[Z-MAS] 🛑 Singularit Mode! Đóng băng Thanh quản và Mắt để tối ưu 100% VRAM cho 26B!`);
      this.voiceEngine.preempt();
      this.whisperNode.flush();
    });

    this.agentLoop.Orchestrator.on("resume_peripherals", () => {
      logger.info(`[Z-MAS] 🟢 Expert đã xả VRAM. Kích hoạt lại Thanh quản và Lỗ tai...`);
    });

    this.whisperNode.on("transcription_ready", async (text: string) => {
      await this.#dispatch("agent_input", text);
    });

    this.voiceEngine.on("audio_base64", (base64: string) => {
      this.ui.broadcastUIEvent("ai_audio_chunk", { audio: base64 });
    });

    // --- DASHBOARD EVENT HANDLERS (Multi-Window Support) ---
    this.ui.on("get_skills_list", (ws: any) => {
      const skills = this.registry.getAllSkills().map(s => ({
        name: s.name,
        description: s.description,
        isCoreSkill: s.isCoreSkill || false,
      }));
      this.ui.sendSkillsList(ws, skills);
    });

    this.ui.on("get_system_status", (ws: any) => {
      const status = {
        model: process.env.ROUTER_MODEL_NAME || "Unknown",
        provider: process.env.AI_PROVIDER || "local",
        uptime: process.uptime(),
        memoryUsage: process.memoryUsage().heapUsed,
      };
      this.ui.sendSystemStatus(ws, status);
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
          if (!fs.existsSync(skillsDir)) return;

          let debounceTimer: NodeJS.Timeout | null = null;

          // 🔒 [Memory Fix #3] Lưu handle vào #fileWatcher để có thể close() sau này
          this.#fileWatcher = fs.watch(skillsDir, (eventType: string, filename: string | null) => {
             if (filename && (filename.endsWith('.ts') || filename.endsWith('.js'))) {
                 if (debounceTimer) clearTimeout(debounceTimer);
                 debounceTimer = setTimeout(() => {
                     logger.warn(`🔥 [DNA Hot-Swap] Phát hiện Thể Đột Biến kỹ năng (${filename}) do AI Singularity sinh ra!`);
                     this.registry.registerLocalSkills().catch(e => logger.error("Lỗi:", e));
                 }, 1000);
             }
          });
       });
    }).catch(e => logger.error("Lỗi import FS trong File Watcher", e));
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

      if (cleanedCount > 0) {
        logger.info(`[GC] Cleaned ${cleanedCount} expired CommandTokens from CoreKernel.`);
      }

      // V14: Lò đốt rác Tẩy Não (Ép Node.js V8 Engine Dọn Dẹp định kỳ)
      if (global.gc) {
          global.gc();
      }
    }, 60000); // V14: Đã tăng chu kỳ lên 60s để nhường CPU cho Garbage Collector
  }

  #registerAuthorityTransition<T extends string, Status extends string>(id: string, schema: TransitionSchema<T, Status>) {
    this.#transitionSchema.set(id, schema);
  }

  async #dispatch(id: string, payload: any) {
    const transition = this.#transitionSchema.get(id);
    if (transition) {
      if (transition.token.__authority && transition.token.__expiresAt > Date.now()) {
        await transition.execute(payload);
      } else if (transition.token.__expiresAt <= Date.now()) {
        logger.error(`❌ [Authority Violation] Token for command: ${id} has expired.`);
      } else {
        logger.error(`❌ [Authority Violation] Forged token detected for command: ${id}`);
      }
    } else {
      logger.error(`❌ [Authority Violation] Attempted to dispatch unregistered handle: ${id}`);
    }
  }

  #setupReactiveSync() {
    this.agentLoop.onThinkingStart = async () => {
      this.voiceEngine.preempt();
      this.whisperNode.flush();
      await this.#dispatch("ui_broadcast", { name: "ai_thinking_start" });
    };

    this.agentLoop.onThinkingEnd = async () => {
      await this.#dispatch("ui_broadcast", { name: "ai_thinking_end" });
    };

    this.agentLoop.onSpokenResponse = async (text: string) => {
      // Bắt và triệt tiêu chuỗi HEARTBEAT_OK
      if (text.trim() === "HEARTBEAT_OK" || text.includes("HEARTBEAT_OK")) {
          logger.info(`[Heartbeat] 🤫 Nhịp đập ổn định. Đã triệt tiêu âm thanh.`);
          return;
      }
      await this.#dispatch("ui_broadcast", { 
        name: "ai_spoken_response", 
        data: { text } 
      });
    };

    this.agentLoop.onStreamStart = async () => {
      await this.#dispatch("ui_broadcast", { name: "ai_stream_start" });
    };

    // Gộp voiceEngine.pushTokens + UI broadcast vào 1 handler duy nhất
    // (trước đây bị gán 2 lần, handler sau override handler đầu → TTS bị câm)
    this.agentLoop.onStreamChunk = async (chunk: string) => {
      if (chunk.includes("HEARTBEAT_OK")) return;
      this.voiceEngine.pushTokens(chunk); // TTS feed
      await this.#dispatch("ui_broadcast", { 
        name: "ai_stream_chunk", 
        data: { textChunk: chunk } 
      });
    };

    // [Z-MAS ZERO-TRUST] Exec Approval Wiring
    this.agentLoop.onExecApprovalRequired = (toolName, command, reason) => {
      return new Promise((resolve) => {
        const approvalId = Date.now().toString() + Math.random().toString(36).substring(7);
        
        // Timeout 30s: Tự động từ chối nếu không có phản hồi
        const timeout = setTimeout(() => {
          this.ui.removeListener("exec_approval_response", handler);
          logger.warn(`[Zero-Trust] Quá thời gian 30s. Tự động TỪ CHỐI lệnh: ${toolName}`);
          resolve({ approved: false });
        }, 30000);

        const handler = (payload: any) => {
          if (payload.approvalId === approvalId) {
            clearTimeout(timeout);
            this.ui.removeListener("exec_approval_response", handler);
            resolve({ 
              approved: payload.approved === true, 
              editedCommand: payload.editedCommand 
            });
          }
        };

        this.ui.on("exec_approval_response", handler);

        // Phát tín hiệu ra UI
        this.#dispatch("ui_broadcast", { 
          name: "exec_approval_required", 
          data: { approvalId, toolName, command, reason } 
        }).catch(e => {
            logger.error(`[Zero-Trust] Lỗi khi gửi broadcast phê duyệt:`, e);
        });
      });
    };
  }

  public async bootstrap() {
    logger.info("🚀 [Orchestrator] Starting Async Distributed Boot Sequence...");
    await Promise.all([
      this.memory.initialize(),
      this.registry.registerLocalSkills()
    ]);
    logger.info("⏳ [Micro-Kernel] Loading Llamas.cpp backend (Distributed Engine)...");
    await this.agentLoop.initModels();
    
    // Bật App Watcher để LIVA nhận thức được phần mềm cài trên máy
    this.appWatcher.start();
    this.appWatcher.setCallback(async (appName, skillData) => {
        // Chủ động đánh thức LIVA bằng cách đẩy một system command giả lập
        await this.#dispatch("agent_input", `[System Cognitive Event]: Người dùng vừa cài đặt ứng dụng '${appName}' lên máy tính. Bạn vừa được nạp kỹ năng điều khiển '${skillData.type}' (${skillData.description}). Hãy RẤT HÀO HỨNG khoe với người dùng rằng bạn đã biết họ cài app mới và đề xuất một hành động ngay lập tức! (Không cần xưng hô System)`);
    });

    // Bật nhịp đập tự trị sau khi boot xong
    this.heartbeat.start();

    logger.info(
      "✅ [Async Distributed Orchestration Kernel] Fully operational. Awaiting Liva connection...",
    );
  }

  public async fetchSystemLocation() {
    try {
      logger.info("🌍 [System] Performing distributed IP geolocation lookup...");
      const start = Date.now();
      const ipRes = await safeFetch("http://ip-api.com/json/", {}, 5000);
      const ipData = await ipRes.json();
      
      this.#currentLatency = Date.now() - start;
      this.#orchestrationTensor.updateWeights([this.#currentLatency]);

      if (ipData && ipData.status === "success") {
        const loc = `City: ${ipData.city || ipData.regionName}, ${ipData.country} (Coords: ${ipData.lat}, ${ipData.lon})`;
        await this.agentLoop.setSystemLocation(loc);
        logger.info(`📍 [System] Location locked via distributed lookup: ${loc}`);
      } else {
        logger.warn("⚠️ [System] Geolocation failed. Using fallback defaults.");
      }
    } catch (e: any) {
      logger.warn(`⚠️ [System] Distributed location error: ${e.message}`);
    }
  }

  public shutdown() {
    const safeExec = (fn: () => void) => { try { fn(); } catch (e) { void e; } };
    // Dọn sạch GC Interval
    if (this.#gcIntervalId) {
      clearInterval(this.#gcIntervalId);
      this.#gcIntervalId = null;
    }
    // 🔒 [Memory Fix #3] Đóng FileWatcher để trả lại system file handle
    if (this.#fileWatcher) {
      safeExec(() => this.#fileWatcher!.close());
      this.#fileWatcher = null;
      logger.info("[CoreKernel] 🧹 FileWatcher đã được đóng an toàn.");
    }
    safeExec(() => this.zalo.stop());
    safeExec(() => this.heartbeat.stop());
    safeExec(() => this.appWatcher.stop());
    safeExec(() => this.voiceEngine.destroy());
    safeExec(() => this.whisperNode.flush());
    safeExec(() => this.whisperNode.destroy());
    safeExec(() => this.memory.dispose());
    safeExec(() => SensoryManager.getInstance().dispose());
    safeExec(() => EmbeddingService.getInstance().dispose());
    logger.info("[CoreKernel] Hệ thống đã shutdown sạch sẽ.");
  }
}