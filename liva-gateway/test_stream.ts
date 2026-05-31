import { Scheduler } from "./src/kernel/Scheduler";
import { SyscallPriority } from "./src/kernel/SyscallInterface";

async function runStreamTest() {
    const scheduler = Scheduler.getInstance();
    console.log("🚀 Testing Syscall Infer with AsyncIterable (Stream) streaming...");

    // Giả lập OpenAI Stream
    async function* mockStream() {
        yield { choices: [{ delta: { content: "Xin " }, finish_reason: null }] };
        await new Promise(r => setTimeout(r, 10));
        yield { choices: [{ delta: { content: "chào " }, finish_reason: null }] };
        await new Promise(r => setTimeout(r, 10));
        yield { choices: [{ delta: { content: "Sếp!" }, finish_reason: "stop" }] };
    }

    try {
        const stream: any = await scheduler.emitSyscall({
            type: "syscall_infer",
            priority: SyscallPriority.SRT,
            payload: {
                client: { 
                    chat: { 
                        completions: { 
                            create: async () => mockStream() 
                        } 
                    } 
                },
                usingTarget: "mock",
                localMsgs: []
            }
        });

        console.log("🔄 Resolved stream object. Now iterating over AsyncIterable...");
        let fullContent = "";
        
        for await (const chunk of stream) {
            const token = chunk.choices[0]?.delta?.content || "";
            fullContent += token;
            process.stdout.write(token);
        }

        console.log("\n✅ Test Streaming hoàn tất. Nội dung: " + fullContent);
        
        if (fullContent === "Xin chào Sếp!") {
            console.log("✅ Promise Resolution cho AsyncIterable hoàn hảo.");
        } else {
            console.log("❌ Lỗi dữ liệu Stream.");
        }
    } catch (e) {
        console.error("❌ Lỗi Crash:", e);
    }
}

runStreamTest();
