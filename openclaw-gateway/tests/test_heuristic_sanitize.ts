import { ToolExecutionOrchestrator } from "../src/core";

function main() {
    console.log("=== TEST THUẬT TOÁN SANITIZE O(1) ===");
    
    // Mock các module truyền vào
    const orchestrator = new ToolExecutionOrchestrator({} as any, {} as any);

    // --- TEST 1: CHUỖI JSON MẢNG KHỔNG LỒ ---
    const bigJsonArray = Array.from({ length: 100 }, (_, i) => ({ 
        id: i, 
        name: `User ${i}`,
        email: `user${i}@example.com`,
        notes: `This is a very long string to simulate heavy JSON payload. Number ${i}` 
    }));
    const jsonStr = JSON.stringify(bigJsonArray, null, 2);
    
    console.log(`\n[Test 1] Đầu vào: Chuỗi JSON Mảng (${jsonStr.length} ký tự, 100 phần tử)`);
    const sanitizedJson = orchestrator.heuristicSanitize(jsonStr, 2500);
    console.log("Kết quả sau Sanitize:\n");
    console.log(sanitizedJson);

    // --- TEST 2: VĂN BẢN (TEXT) DÀI QUÁ CỠ ---
    const headText = "ĐÂY LÀ PHẦN ĐẦU VĂN BẢN RẤT QUAN TRỌNG\n".repeat(50);
    const middleText = "ĐÂY LÀ PHẦN GIỮA VÔ NGHĨA SẼ BỊ CẮT\n".repeat(100);
    const tailText = "ĐÂY LÀ PHẦN CUỐI VĂN BẢN RẤT QUAN TRỌNG\n".repeat(50);
    
    const bigText = headText + middleText + tailText;
    console.log(`\n[Test 2] Đầu vào: Văn bản thô (${bigText.length} ký tự)`);
    const sanitizedText = orchestrator.heuristicSanitize(bigText, 1000);
    
    console.log("Kết quả sau Sanitize:\n");
    console.log(sanitizedText);
}

main();
