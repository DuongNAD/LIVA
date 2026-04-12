import { MemoryManager } from "./MemoryManager";

async function runPlayground() {
  const memory = new MemoryManager("core_agent");
  await memory.initialize();

  // 1. Tương tác ngắn hạn (Short-term context)
  await memory.addMessage(
    "user",
    "Anh rất thích uống nước cùi mía và giải toán ma trận.",
  );
  await memory.addMessage(
    "assistant",
    "Em nhớ rồi ạ, sở thích ẩm thực và học tập của Anh rất thú vị!",
  );

  // 2. Giả lập tác vụ Background Job trích xuất thông tin (Entity Extraction)
  // Trong thực tế, LLM sẽ tự động đọc đoạn chat trên và gọi hàm này
  const extractedFacts = [
    "Người dùng thích đồ uống sáng tạo: nước cùi mía.",
    "Người dùng có đam mê với Toán học, đặc biệt là đại số tuyến tính (ma trận).",
  ];

  // 3. Ghi vào trí nhớ dài hạn (Memory Compaction)
  await memory.updateLongTermMemory(
    "Thói quen & Sở thích (Habits & Preferences)",
    extractedFacts,
  );

  // 4. In ra kết quả để kiểm tra
  const longTermData = await memory.getLongTermContext();
  console.log("\n--- Nội dung tệp Markdown (Long-term Memory) ---");
  console.log(longTermData);
}

runPlayground();
