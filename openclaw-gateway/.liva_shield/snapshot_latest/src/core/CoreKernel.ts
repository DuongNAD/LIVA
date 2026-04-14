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

    // --- CENTRALIZED AUTHORITY REGISTRATION ---
    this.#registerAuthorityTransition<"ui_broadcast", "ACTIVE">(
      "ui_broadcast", 
      {
        token: this.#mintCommandToken<"ui_broadcast", "ACTIVE">("ui_broadcast"),
        execute: async (event: { name: string; data?: any }) => {
          await this.ui.broadcastUIEvent(event.name, event.data);
        }
      }
    );

    this.#registerAuthorityTransition<"agent_input", "ACTIVE">(
      "agent_input", 
      {
        token: this.#mintCommandToken<"agent_input", "ACTIVE">("agent_input"),
        execute: async (text: string) => {
          await this.agentLoop.handleUserInput(text);
        }
      }
    );

    // --- MICRO-TASK ORCHESTRATION FLOW ---
    this.ui.on("user_input", async (userText: string) => {
      const weight = this.#orchestrationTensor.getWeight(this.#currentLatency);
      if (weight > 0.2) {
        await this.#dispatch<"agent_input", "ACTIVE">("agent_input", userText);
      } else {
        logger.warn("⚠️ [Orchestrator] High latency detected. Throttling branded transition.");
      }
    });

    this.zalo.on("zal_incoming", async (userText: string) => {
      await this.#dispatch<"agent_input", "ACTIVE">("agent_input", userText);
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
      await this.#dispatch<"agent_input", "ACTIVE">("agent_input", text);
    });

    this.voiceEngine.on("audio_chunk", (buffer: Buffer) => {
      this.ui.broadcastAudioChunk(buffer);
    });

    this.#setupReactiveSync();
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
    }, 30000);
  }

  #registerAuthorityTransition<T extends string, Status extends string>(id: string, schema: TransitionSchema<T, Status>) {
    this.#transitionSchema.set(id, schema);
  }

  async #dispatch<T extends string, Status extends string>(id: string, payload: any) {
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
      await this.#dispatch<"ui_broadcast", "ACTIVE">("ui_broadcast", { name: "ai_thinking_start" });
    };

    this.agentLoop.onThinkingEnd = async () => {
      await this.#dispatch<"ui_broadcast", "ACTIVE">("ui_broadcast", { name: "ai_thinking_end" });
    };

    this.agentLoop.onStreamChunk = (chunk: string) => {
      this.voiceEngine.pushTokens(chunk);
    };

    this.agentLoop.onSpokenResponse = async (text: string) => {
      await this.#dispatch<"ui_broadcast", "ACTIVE">("ui_broadcast", { 
        name: "ai_spoken_response", 
        data: { text } 
      });
    };

    this.agentLoop.onStreamStart = async () => {
      await this.#dispatch<"ui_broadcast", "ACTIVE">("ui_broadcast", { name: "ai_stream_start" });
    };

    this.agentLoop.onStreamChunk = async (chunk: string) => {
      await this.#dispatch<"ui_broadcast", "ACTIVE">("ui_broadcast", { 
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
    if (this.#gcIntervalId) {
      clearInterval(this.#gcIntervalId);
    }
  }
}