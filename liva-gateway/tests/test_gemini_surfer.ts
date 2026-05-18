import { execute as runGemini } from "../src/skills/GeminiSurfer.js";

async function main() {
    console.log("=== TEST KỸ NĂNG GEMINI SURFER ===");
    console.log("⚠️ Đảm bảo bạn đã tắt hết Chrome và mở lại bằng shortcut có cờ: --remote-debugging-port=9222\n");
    
    try {
        const query = "Theo góc độ của một AI, cách tốt nhất để tự tối ưu hóa một hệ thống Agentic AI hoạt động hoàn toàn ở Local là gì?";
        console.log(`Đang gửi câu hỏi tới Gemini: "${query}"\n`);
        
        console.log("Đang gọi thư viện Playwright CDP...");
        const result = await runGemini({ query });
        
        console.log("\n================ KẾT QUẢ TỪ GEMINI ================\n");
        console.log(result);
        console.log("\n===================================================\n");
    } catch (e: any) {
        console.error("❌ LỖI:", e.message);
    }
}

main();
