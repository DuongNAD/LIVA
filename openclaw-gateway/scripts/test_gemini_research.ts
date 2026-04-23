import { execute } from "../src/skills/GeminiSurfer.js";

async function main() {
    console.log("🚀 Bắt đầu quá trình AI tự động Research (Dùng GeminiSurfer để duyệt web)...");
    try {
        const result = await execute({
            query: "Tìm kiếm và nghiên cứu các đánh giá, thông số kỹ thuật chi tiết của Oppo A77s",
            useDeepResearch: true
        });
        console.log("\n================ KẾT QUẢ NGHIÊN CỨU ================\n");
        console.log(result);
        console.log("\n====================================================\n");
    } catch (e) {
        console.error("Test failed:", e);
    }
}

main();
