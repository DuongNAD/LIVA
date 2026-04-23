import { config } from "dotenv";
config();
import { AgentLoop } from "../src/core";
import { MemoryManager } from "../src/MemoryManager.js";
import { SkillRegistry } from "../src/SkillRegistry.js";

async function testHandoff() {
  console.log("=== BẮT ĐẦU KIỂM TRA TẢI MODEL CHUYÊN GIA 12GB ===");
  try {
    const memory = new MemoryManager("test_user");
    const registry = new SkillRegistry();
    await registry.registerLocalSkills();

    // Để test EngineOrchestrator một cách gọn nhẹ, ta có thể inject trực tiếp nếu nó được export,
    // hoặc trigger AgentLoop xử lý 1 lệnh ép chuyển giao.
    
    // Vì EngineOrchestrator không được export riêng, ta sẽ tạo AgentLoop và dùng lệnh chuyển giao
    const agent = new AgentLoop(memory, registry);
    
    console.log("Đang giả lập nhận tin nhắn yêu cầu xử lý phức tạp...");
    
    // Gắn event tracking
    agent.onStreamChunk = (chunk) => process.stdout.write(chunk);
    agent.onSpokenResponse = (txt) => console.log("\n[LIVA Trả lời]:", txt);
    
    console.log("Đang tải Router Model E4B...");
    await agent.initModels();

    // Gửi lệnh mà chắn chắn con Router E4B sẽ không tự làm được mà gọi handoff_to_expert
    agent.handleUserInput("Đây là bài toán logic siêu cấp đòi hỏi khả năng tư duy cao và viết code phức tạp. Hãy sử dụng kỹ năng handoff_to_expert để gọi ngay mô hình Chuyên Gia (26B) để phân tích cho tôi ngay lập tức!");
    
    // Lệnh trên là bất đồng bộ (khờ chạy bên trong queue), nên ta móc thời gian đợi
    await new Promise(resolve => setTimeout(resolve, 180000));
    console.log("Hoàn tất bài thử nghiệm.");
    process.exit(0);
  } catch(e) {
    console.error("Lỗi:", e);
    process.exit(1);
  }
}

testHandoff();
