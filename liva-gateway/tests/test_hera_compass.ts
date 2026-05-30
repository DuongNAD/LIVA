import { HeraCompass } from "../src/memory/HeraCompass.js";

async function main() {
    console.log("=== TEST NÃO BỘ HERA COMPASS ===");
    await HeraCompass.create();
    const hera = HeraCompass.getInstance();

    console.log("\n[Test 1] Trích xuất kinh nghiệm từ Database:");
    // Thử truy vấn 1 lỗi bất kỳ, ví dụ "not found" hoặc "error"
    const insights = hera.getRelatedInsight("error not found", "read_local_file", { limit: 3, minScore: -10 });
    
    if (insights.length > 0) {
        console.log(`✅ Đã tìm thấy ${insights.length} bài học kinh nghiệm:`);
        insights.forEach((i, idx) => {
            console.log(`  ${idx + 1}. [Score: ${i.utility_score}] Lệnh: "${i.actionable_rule}"`);
        });
    } else {
        console.log("⚠️ Không tìm thấy bài học nào phù hợp (Có thể Database Hera đang trống).");
    }

    console.log("\n(Ghi chú: Để test tính năng Học Hỏi ngầm, vui lòng chạy lệnh AgentLoop đầy đủ vì cần gọi LLM API).");
}

main().catch(console.error);
