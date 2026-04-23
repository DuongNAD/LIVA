import { execute } from '../src/skills/GeminiSurfer.js';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

async function runAdvancedTest() {
    console.log("\n=== TEST KỸ NĂNG GEMINI SURFER (ADVANCED) ===");
    console.log("⚠️ Đảm bảo bạn đã tắt hết Chrome và mở lại bằng shortcut có cờ: --remote-debugging-port=9222\n");

    // Lấy đường dẫn tuyệt đối của file package.json để test upload
    const testFilePath = path.resolve(__dirname, '../package.json');

    console.log(`Đang gửi câu hỏi tới Gemini với Model: Tư duy (Thinking)`);
    console.log(`Đính kèm file: ${testFilePath}`);
    console.log(`Bật Deep Research: True\n`);

    const result = await execute({
        query: "Please analyze this package.json file and suggest 3 high-performance libraries for an AI Gateway system. Respond concisely in English.",
        modelType: "thinking",
        useDeepResearch: true,
        files: [testFilePath]
    });

    console.log("\n================ KẾT QUẢ TỪ GEMINI ================\n");
    console.log(result);
    console.log("\n===================================================\n");
}

runAdvancedTest().catch(console.error);
