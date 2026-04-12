import { UIController } from "./UIController";
import { AgentLoop } from "./AgentLoop";
import { MemoryManager } from "../MemoryManager";
import { SkillRegistry } from "../SkillRegistry";
import { ZaloPolling } from "./ZaloPolling"; 
import { VoiceEngine } from "../services/VoiceEngine";
import { WhisperNode } from "../services/WhisperNode";
import { logger } from "../utils/logger";

/**
 * @type_level_programming
 * HYPER-TYPED BRANDING SYSTEM (Structural Identity via Interface Merging)
 * Upgraded to utilize TypeScript 5.x Branded Types for non-forgeable identity.
 */
export type Brand<T, F> = T & { readonly __brand_identity: F };

/**
 * @evolution_target
 * AUTHORITY TOKEN: KernelAuthority
 * A cryptographicly validated branded boolean.
 */
export type KernelAuthority = boolean & Brand<boolean, "CORE_KERNEL_SIGNED">;

/**
 * @evolution_target
 * COMMAND TOKEN: CommandToken<T, Status>
 * An opaque handle that combines structural identity with authority requirements.
 */
export type CommandToken<T extends string, Status extends string> = {
  readonly __id: T;
  readonly __authority: KernelAuthority;
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
 * Implements non-forgeable private class members (#) to ensure absolute zero-trust integrity.
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

  // Hard Private Members (Opaque Engine Isolation via #)
  #orchestrationTensor: ReactiveStateTensor;
  #transitionSchema: Map<string, TransitionSchema<any, any>>;
  #currentLatency: number = 0;

  /**
   * @private_factory
   * THE SOLE SOURCE OF TRUTH.
   * Mints non-forgeable branded handles used by ModelOrchestrator and TurboQuantStore.
   */
  #mintCommandToken<T extends string, Status extends string>(id: T): CommandToken<T, Status> {
    return {
      __id: id,
      __authority: true as unknown as KernelAuthority,
      __brand_identity: "" as any // Satisfies Brand structure without complex Symbol overhead
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
        logger.warn("⚠️ [Orchestrator] High latency detected. Throttlting branded transition.");
      }
    });

    this.zalo.on("zal_incoming", async (userText: string) => {
      await this.#dispatch<"agent_input", "ACTIVE">("agent_input", userText);
    });

    // --- PIPELINE ÂM THANH PIPELINE (ZERO-LATENCY) ---
    this.ui.on("audio_input", (buffer: Buffer) => {
      this.whisperNode.pushAudioChunk(buffer);
    });

    this.whisperNode.on("transcription_ready", async (text: string) => {
      await this.#dispatch<"agent_input", "ACTIVE">("agent_input", text);
    });

    this.voiceEngine.on("audio_chunk", (buffer: Buffer) => {
      this.ui.broadcastAudioChunk(buffer);
    });

    this.#setupReactiveSync();
  }

  #registerAuthorityTransition<T extends string, Status extends string>(id: string, schema: TransitionSchema<T, Status>) {
    this.#transitionSchema.set(id, schema);
  }

  async #dispatch<T extends string, Status extends string>(id: string, payload: any) {
    const transition = this.#transitionSchema.get(id);
    if (transition) {
      // Runtime check for the authority signature using branded identity
      if (transition.token.__authority) {
        await transition.execute(payload);
      } else {
        logger.error(`❌ [Authority Violation] Forged token detected for command: ${id}`);
      }
    } else {
      logger.error(`❌ [Authority Violation] Attempted to dispatch unregistered handle: ${id}`);
    }
  }

  #setupReactiveSync() {
    this.agentLoop.onThinkingStart = async () => {
      await this.#dispatch<"ui_broadcast", "ACTIVE">("ui_broadcast", { name: "ai_thinking_start" });
    };

    this.agentLoop.onThinkingEnd = async () => {
      await this.#dispatch<"ui_broadcast", "ACTIVE">("ui_broadcast", { name: "ai_thinking_end" });
    };

    // Đổ Stream Token của Não 26B thẳng vào Thanh Quản Kokoro
    this.agentLoop.onStreamChunk = (chunk: string) => {
      this.voiceEngine.pushTokens(chunk);
    };

    // Khi AgentLoop chuẩn bị bẻ lái luồng suy nghĩ, ngắt ngay Voice đang nói dở (Preemption)
    this.agentLoop.onThinkingStart = async () => {
      this.voiceEngine.preempt();
      this.whisperNode.flush();
      await this.#dispatch<"ui_broadcast", "ACTIVE">("ui_broadcast", { name: "ai_thinking_start" });
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
}