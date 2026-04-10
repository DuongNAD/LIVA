import { UIController } from "./UIController";
import { AgentLoop } from "./AgentLoop";
import { MemoryManager } from "../MemoryManager";
import { SkillRegistry } from "../SkillRegistry";
import { logger } from "../utils/logger";

export class CoreKernel {
  public memory: MemoryManager;
  public registry: SkillRegistry;
  public ui: UIController;
  public agentLoop: AgentLoop;

  constructor() {
    this.memory = new MemoryManager("liva_core");
    this.registry = new SkillRegistry();
    this.ui = new UIController(8082);
    this.agentLoop = new AgentLoop(this.memory, this.registry);

    // Nối kết UI (Frontend) báo lệnh về -> AgentLoop (Backend / LLM) chạy
    this.ui.on("user_input", (userText: string) => {
      this.agentLoop.handleUserInput(userText);
    });

    // Nối kết AgentLoop (Backend / LLM) báo trạng thái ra -> UIController (Frontend) cập nhật UI
    this.agentLoop.onThinkingStart = () => {
      this.ui.broadcastUIEvent("ai_thinking_start");
    };

    this.agentLoop.onThinkingEnd = () => {
      this.ui.broadcastUIEvent("ai_thinking_end");
    };

    this.agentLoop.onSpokenResponse = (text: string) => {
      this.ui.broadcastUIEvent("ai_spoken_response", { text });
    };

    this.agentLoop.onStreamStart = () => {
      this.ui.broadcastUIEvent("ai_stream_start");
    };

    this.agentLoop.onStreamChunk = (chunk: string) => {
      this.ui.broadcastUIEvent("ai_stream_chunk", { textChunk: chunk });
    };
  }

  public async bootstrap() {
    await this.memory.initialize();
    await this.registry.registerLocalSkills();
    logger.info("⏳ Đang nạp hệ thống Llama.cpp backend (Local Engine)...");
    await this.agentLoop.initModels();
    logger.info(
      "✅ Lõi hệ thống (Core Kernel) đã khởi động toàn diện. Chờ Liva kết nối...",
    );
  }

  public async fetchSystemLocation() {
    try {
      logger.info(
        "🌍 [System] Đang dò tìm vị trí hiện tại của thiết bị qua IP...",
      );
      const ipRes = await fetch("http://ip-api.com/json/");
      const ipData = await ipRes.json();
      if (ipData && ipData.status === "success") {
        const loc = `Thành phố ${ipData.city || ipData.regionName}, ${ipData.country} (Tọa độ: ${ipData.lat}, ${ipData.lon})`;
        this.agentLoop.setSystemLocation(loc);
        logger.info(`📍 [System] Đã chốt vị trí thiết bị tại: ${loc}`);
      } else {
        logger.warn("⚠️ [System] Không thể lấy vị trí IP, sẽ dùng mặc định.");
      }
    } catch (e: any) {
      logger.warn(`⚠️ [System] Lỗi khi tra cứu IP định vị: ${e.message}`);
    }
  }
}
