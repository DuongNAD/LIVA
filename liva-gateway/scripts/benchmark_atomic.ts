import { StructuredMemory } from "../src/memory/StructuredMemory";
import { spawn } from "node:child_process";
import { randomUUID } from "node:crypto";
import { promises as fsp } from "node:fs";
import * as path from "node:path";

const AGENT_ID = "atomic_test";

async function runChild() {
    console.log("[Child] Bắt đầu khởi tạo database...");
    const mem = await StructuredMemory.create(AGENT_ID);
    await mem.initVecDimension(384);
    
    console.log("[Child] Bắt đầu vòng lặp GHI DỮ LIỆU CƯỜNG ĐỘ CAO (Heavy Writes)...");
    
    let batchIndex = 0;
    while (true) {
        batchIndex++;
        const dummyVector = new Array(384).fill(0).map(() => Math.random());
        
        // Tạo một transaction lớn
        mem.db.exec("BEGIN");
        
        try {
            for (let i = 0; i < 5000; i++) {
                const id = randomUUID();
                
                // Ghi vào bảng events
                mem.db.prepare(`
                    INSERT INTO events 
                    (eventId, timestamp, phi_facts, phi_entities, psi_sentiment, psi_intent, psi_relational, rawUserMsg, rawAiReply, consolidated, domain, category, trace_keywords, last_accessed_at, consolidation_status, retry_count)
                    VALUES (?, ?, '[]', '[]', 'neutral', 'inform', 'none', ?, 'AI Reply', 0, 'General', 'Uncategorized', '[]', 0, 'pending', 0)
                `).run(id, Date.now(), "Nội dung chat test " + id);
            }
            
            // Cố tình tạo một vòng lặp giả lập xử lý vector chèn thêm thời gian
            const vectorBatch = Array.from({length: 100}, () => ({
                vecId: "vec_" + randomUUID(),
                type: "AXIOM",
                content: "Content " + randomUUID(),
                vector: dummyVector.map(v => v * Math.random()),
            }));
            
            mem.db.exec("COMMIT");
            
            // Ghi vectors (dùng transaction riêng trong hàm batch)
            await mem.upsertVectorsBatch(vectorBatch);
            
            console.log(`[Child] Đã ghi xong Batch #${batchIndex}...`);
        } catch (e) {
            mem.db.exec("ROLLBACK");
            console.error("[Child] Lỗi ghi:", e);
        }
    }
}

async function runParent() {
    console.log("=== BẮT ĐẦU KIỂM TRA TÍNH TOÀN VẸN (ATOMIC WRITE & INTEGRITY TEST) ===");
    
    // Dọn dẹp DB cũ
    const dbPath = path.join(process.cwd(), "data", "agents", AGENT_ID);
    await fsp.rm(dbPath, { recursive: true, force: true }).catch(() => {});
    
    console.log("[Parent] Đã xóa DB cũ. Khởi tạo Child Process để ghi dữ liệu liên tục...");
    
    // Khởi chạy tiến trình con
    const child = spawn(process.execPath, ["--import", "tsx", "scripts/benchmark_atomic.ts", "--child"], {
        stdio: 'pipe',
        shell: false,
        env: { ...process.env, LIVA_ENCRYPTION_KEY: '12345678901234567890123456789012' }
    });
    
    child.stdout.on('data', (data) => process.stdout.write(`  ${data}`));
    child.stderr.on('data', (data) => process.stderr.write(`  ${data}`));

    // Chờ 2.5 giây cho tiến trình con đang ghi điên cuồng
    await new Promise(resolve => setTimeout(resolve, 2500));
    
    console.log("\n[Parent] ⚠️ PHÁT HIỆN SỰ CỐ! ÉP TẮT (KILL) TIẾN TRÌNH CON NGAY LÚC ĐANG GHI...");
    
    // Dùng SIGKILL (force kill), tiến trình con sẽ bị chết ngay lập tức, 
    // không kịp gọi các hàm cleanup (onExit, close DB...)
    child.kill('SIGKILL');
    
    // Wait for child to actually die before checking
    await new Promise((resolve) => child.on('exit', resolve));
    
    console.log("[Parent] Tiến trình con đã bị tiêu diệt hoàn toàn.");
    console.log("[Parent] Bắt đầu khởi động lại Database và kiểm tra lỗi Corrupt...\n");

    // Chờ 1 chút để OS nhả file lock
    await new Promise(resolve => setTimeout(resolve, 1000));
    
    try {
        const mem = await StructuredMemory.create(AGENT_ID);
        await mem.initVecDimension(384);
        
        const count = await mem.getUnconsolidatedCount();
        const vecCount = await mem.getVectorCount();
        console.log(`🟢 [Phục hồi thành công] Đọc được ${count} events và ${vecCount} vectors.`);
        
        // Chạy kiểm tra tính toàn vẹn của SQLite
        console.log("🟢 [PRAGMA] Đang chạy lệnh quét lỗi hệ thống file SQLite (PRAGMA integrity_check)...");
        const integrityCheck = mem.db.prepare("PRAGMA integrity_check").all() as any[];
        
        const isOk = integrityCheck.every(row => row.integrity_check === "ok");
        if (isOk) {
            console.log("\n✅ KẾT LUẬN: ĐẠT! Cơ chế Atomic Write và WAL Mode của SQLite đã bảo vệ file dữ liệu hoàn hảo. Không có byte nào bị Corrupt!");
        } else {
            console.log("\n❌ KẾT LUẬN: KHÔNG ĐẠT! Dữ liệu đã bị lỗi Corrupt!");
            console.log(integrityCheck);
        }
        
        process.exit(0);
    } catch (error) {
        console.error("\n❌ FATAL: Không thể phục hồi DB, file đã bị hỏng hoàn toàn (Corrupted)!");
        console.error(error);
        process.exit(1);
    }
}

if (process.argv.includes("--child")) {
    runChild().catch(e => {
        console.error(e);
        process.exit(1);
    });
} else {
    runParent().catch(e => {
        console.error(e);
        process.exit(1);
    });
}
