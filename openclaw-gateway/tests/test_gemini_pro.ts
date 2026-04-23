import { execute } from '../src/skills/GeminiSurfer.js';

async function runProTest() {
    console.log("\n=== TEST GEMINI SURFER (PRO MODEL) ===");
    console.log("⚠️ Đảm bảo bạn đã tắt hết Chrome và mở lại bằng shortcut có cờ: --remote-debugging-port=9222\n");

    const query = "Từ góc độ một kiến trúc sư phần mềm, hãy viết một đoạn mã Python sử dụng Design Pattern 'Strategy' để tính phí giao hàng (Shipping) cho 3 loại dịch vụ: Giao chuẩn, Giao nhanh và Giao siêu tốc.";
    
    console.log(`Đang gửi câu hỏi tới Gemini: "${query}"`);
    console.log(`Bật Model: Pro`);
    
    const result = await execute({
        query: query,
        modelType: "pro"
    });

    console.log("\n================ KẾT QUẢ TỪ GEMINI ================\n");
    console.log(result);
    console.log("\n===================================================\n");
}

runProTest().catch(console.error);
