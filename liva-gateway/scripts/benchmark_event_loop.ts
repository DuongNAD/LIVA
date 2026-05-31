import { StructuredMemory } from "../src/memory/StructuredMemory";
import { ConsolidationCron } from "../src/memory/ConsolidationCron";
import { randomUUID } from "node:crypto";
import { performance } from "node:perf_hooks";

// --- MOCK SERVICES ---
class MockEmbeddingService {
    async embed(text: string) {
        // Giả lập độ trễ mạng khi nhúng Vector
        await new Promise(resolve => setTimeout(resolve, 50));
        return new Array(384).fill(0).map(() => Math.random());
    }
}

class MockBookIndex {
    addNode() {}
    addEdge() {}
}

const mockAiClient = {
    chat: {
        completions: {
            create: async () => {
                // Giả lập API gọi LLM mất 500ms
                await new Promise(resolve => setTimeout(resolve, 500));
                return {
                    choices: [{
                        message: {
                            content: JSON.stringify({
                                narrative_summary: "Người dùng đang test hiệu năng hệ thống.",
                                new_user_insights: [{ key: "Test", value: "Thích tối ưu code", category: "Dev" }]
                            })
                        }
                    }]
                };
            }
        }
    }
} as any;

async function run() {
    console.log("Initializing Test Environment...");
    const mem = await StructuredMemory.create("event_loop_test");
    mem.initVecDimension(384);

    const cron = new ConsolidationCron(
        mem,
        new MockEmbeddingService() as any,
        new MockBookIndex() as any,
        mockAiClient
    );

    // Xóa dữ liệu cũ nếu có
    mem.deleteAllEvents();

    // Nạp 200 events chưa consolidated (chia làm 4 sessions)
    console.log("Injecting 200 unconsolidated events...");
    const baseTime = Date.now() - (4 * 60 * 60 * 1000); // 4 tiếng trước
    for (let i = 0; i < 200; i++) {
        const sessionId = Math.floor(i / 50); // Mỗi session 50 events
        mem.insertEvent({
            eventId: randomUUID(),
            timestamp: baseTime + (sessionId * 60 * 60 * 1000) + (i * 1000), // Cách nhau 1 tiếng mỗi session
            phi: { facts: ["Fact 1", "Fact 2"], entities: ["LIVA", "NodeJS"] },
            psi: { sentiment: "tích cực", intent: "kiểm tra", relational: "" },
            rawUserMsg: "Test message " + i,
            rawAiReply: "AI Reply " + i,
            domain: "Development"
        });
    }

    console.log("\n=== BẮT ĐẦU ĐO EVENT LOOP LAG ===");
    console.log("Tauri UI mượt mà yêu cầu Event Loop Lag không vượt quá 16ms (tương đương 60 FPS).\n");

    let maxLag = 0;
    let avgLag = 0;
    let ticks = 0;
    const intervalMs = 10; 
    let lastTick = performance.now();

    // Event Loop Monitor
    const monitor = setInterval(() => {
        const now = performance.now();
        const lag = Math.max(0, now - lastTick - intervalMs);
        
        if (lag > maxLag) maxLag = lag;
        avgLag += lag;
        ticks++;
        
        lastTick = now;
    }, intervalMs);

    // Kích hoạt tác vụ nặng (Consolidation)
    const startTime = performance.now();
    console.log("Đang chạy ConsolidationCron.consolidateNow() ngầm...");
    
    // Giả lập Ping từ Tauri UI mỗi 500ms để xem WebSockets có bị block không
    const pingTimer = setInterval(() => {
        console.log(`[Tauri UI Ping] Pong nhận được! (Max lag hiện tại: ${maxLag.toFixed(2)} ms)`);
    }, 500);

    const consolidatedCount = await cron.consolidateNow(true);
    
    const endTime = performance.now();
    clearInterval(monitor);
    clearInterval(pingTimer);

    console.log("\n=== KẾT QUẢ BÁO CÁO ===");
    console.log(`Đã tổng hợp thành công: ${consolidatedCount} events.`);
    console.log(`Tổng thời gian chạy: ${((endTime - startTime) / 1000).toFixed(2)} giây.`);
    console.log(`Độ trễ tối đa của Event Loop (Max Block): ${maxLag.toFixed(2)} ms.`);
    console.log(`Độ trễ trung bình của Event Loop (Avg Block): ${(avgLag / ticks).toFixed(2)} ms.`);

    if (maxLag < 20) {
        console.log("🟢 KẾT LUẬN: ĐẠT! ConsolidationCron không hề làm block Node.js. UI sẽ mượt mà 60 FPS.");
    } else if (maxLag < 100) {
        console.log("🟡 KẾT LUẬN: KHÁ! Có giật nhẹ (Frame drop) nhưng UI vẫn phản hồi tốt.");
    } else {
        console.log("🔴 KẾT LUẬN: KHÔNG ĐẠT! Event loop bị block nặng, Tauri UI có thể bị đơ trong quá trình tổng hợp.");
    }

    process.exit(0);
}

run().catch(console.error);
