import { Scheduler } from "./src/kernel/Scheduler";
import { SyscallPriority } from "./src/kernel/SyscallInterface";

async function runTest() {
    const scheduler = Scheduler.getInstance();
    console.log("🚀 Bắt đầu test AIOS Scheduler...");

    const results: string[] = [];

    // Tạm đình chỉ Queue để nạp cùng lúc 3 Task với 3 Priority khác nhau
    scheduler.suspend();

    console.log("📥 Đang nạp tác vụ DT (Delay-Tolerant - Ghi nhớ ngầm)...");
    scheduler.emitSyscall({
        type: "syscall_vector_search",
        priority: SyscallPriority.DT,
        payload: {}
    }).then(() => results.push("DT_DONE"));

    console.log("📥 Đang nạp tác vụ SRT (Soft Real-Time - LLM Chat)...");
    scheduler.emitSyscall({
        type: "syscall_infer",
        priority: SyscallPriority.SRT,
        payload: {
            client: { 
                chat: { 
                    completions: { 
                        create: async () => { 
                            await new Promise(r => setTimeout(r, 100)); 
                            results.push("SRT_RUNNING"); 
                            return {}; 
                        } 
                    } 
                } 
            },
            usingTarget: "mock",
            localMsgs: []
        }
    }).then(() => results.push("SRT_DONE"));

    console.log("📥 Đang nạp tác vụ HRT (Hard Real-Time - Khẩn cấp)...");
    scheduler.emitSyscall({
        type: "syscall_execute_tool",
        priority: SyscallPriority.HRT,
        payload: {
            toolOrchestrator: { 
                executeWithReflection: async () => { 
                    await new Promise(r => setTimeout(r, 50)); 
                    results.push("HRT_RUNNING"); 
                    return { success: true }; 
                } 
            }
        }
    }).then(() => results.push("HRT_DONE"));

    console.log("🟢 Khôi phục Scheduler để bắt đầu chạy Queue...");
    scheduler.resume();

    // Đợi 500ms để đảm bảo Scheduler chạy xong
    await new Promise(r => setTimeout(r, 500));

    console.log("\n================ KẾT QUẢ ==================");
    console.log("Thứ tự nạp vào: DT -> SRT -> HRT");
    console.log("Thứ tự thi hành thực tế:", results);
    
    if (results[0] === "HRT_RUNNING" && results[1] === "HRT_DONE" && results[2] === "SRT_RUNNING") {
        console.log("✅ THÀNH CÔNG: Scheduler hoạt động hoàn hảo! Ưu tiên tác vụ Khẩn Cấp (HRT) dù được nạp sau cùng.");
    } else {
        console.log("❌ THẤT BẠI: Lỗi điều phối Priority.");
    }
}

runTest();
