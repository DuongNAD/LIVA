import { UIController } from "../core/UIController";
import { AgentLoop } from "../core/AgentLoop";
import { MemoryManager } from "../MemoryManager";
import { SkillRegistry } from "../SkillRegistry";
import { ZaloPolling } from "../core/ZaloPolling"; 
import { VoiceEngine } from "../services/VoiceEngine";
import { WhisperNode } from "../services/WhisperNode";
import { logger } from "../utils/logger";

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
  public voiceEngine: VoiceEngine;
  public whisperNode: WhisperNode;

  // Hard Private Members (Opague Engine Isolation via #)
  #orchestrationTensor: ReactiveStateTensor;
  /** @evolution_target O(1) Dispatch Map */
  #transitionSchema: Map<string, TransitionSchema<any, any>>;
  #currentLatency: number = 0;
  /** @evolution_target Garbage Collection Interval */
  #gcIntervalId: NodeJS.Timeout | null = null;
  // 🔒 [Memory Fix #3] Lưu handle FileWatcher để close() khi shutdown (tránh rò rỉ fs handle)
  #fileWatcher: ReturnType<typeof import('fs').watch> | null = null;
  readonly DEFAU_TTL = 60000; // 60 seconds default

  /**
   * @private_factory
   * Mints non-forgeable branded handles with TTL.
   */
  #mintCommandToken<T extends string, Status extends string>(id: T, ttl: number = this.DEFAU_TTL): CommandToken<T, Status> {
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
    this.zalo = new ZaloPolling();
    this.voiceEngine = new VoiceEngine();
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
      this.voiceEngine.pushTokens(chunk); // TTS feed
      await this.#dispatch("ui_broadcast", { 
        name: "ai_stream_chunk", 
        data: { textChunk: chunk } 
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
    logger.info(
      "✅ [Async Distributed Orchestration Kernel] Fully operational. Awaiting Liva connection...",
    );
  }

  public async fetchSystemLocation() {
    try {
      logger.info("🌍 [System] Performing distributed IP geolocation lookup...");
      const start = Date.now();
      const ipRes = await fetch("http://ip-api.com/json/");
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
    // Dọn sạch GC Interval
    if (this.#gcIntervalId) {
      clearInterval(this.#gcIntervalId);
      this.#gcIntervalId = null;
    }
    // 🔒 [Memory Fix #3] Đóng FileWatcher để trả lại system file handle
    if (this.#fileWatcher) {
      this.#fileWatcher.close();
      this.#fileWatcher = null;
      logger.info("[CoreKernel] 🧹 FileWatcher đã được đóng an toàn.");
    }
    // 🔒 [Audit Fix] Dừng Zalo Polling loop để tránh zombie setTimeout
    this.zalo.stop();
    // 🔒 [Memory Fix] Gọi destroy() trên VoiceEngine để clear Zombie Timer
    this.voiceEngine.destroy();
    logger.info("[CoreKernel] Hệ thống đã shutdown sạch sẽ.");
  }
}