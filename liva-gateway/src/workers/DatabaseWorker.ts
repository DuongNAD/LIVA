import { parentPort } from "node:worker_threads";
import { DatabaseSync } from "node:sqlite";

let db: DatabaseSync | null = null;

// Giao tiếp qua parentPort
parentPort?.on("message", (msg) => {
    const { id, type, payload } = msg;
    try {
        if (type === "init") {
            // Khởi tạo database
            const { storePath } = payload;
            db = new DatabaseSync(storePath, { allowExtension: true });
            
            // Bắt buộc cấu hình độ an toàn dữ liệu
            db.exec(`PRAGMA journal_mode = WAL;`);
            db.exec(`PRAGMA synchronous = NORMAL;`);
            
            // Load extensions if needed (sqlite-vec handled by repo usually, but we init here)
            // LIVA logic cho FTS5 và sqlite-vec
            
            parentPort?.postMessage({ id, success: true, type: "log", level: "info", message: "[DatabaseWorker] Đã khởi tạo thành công node:sqlite với WAL mode." });
            parentPort?.postMessage({ id, success: true });
        } else if (type === "prepareAll") {
            if (!db) throw new Error("Database not initialized");
            const stmt = db.prepare(payload.sql);
            const result = stmt.all(...(payload.params || []));
            parentPort?.postMessage({ id, success: true, data: result });
        } else if (type === "prepareGet") {
            if (!db) throw new Error("Database not initialized");
            const stmt = db.prepare(payload.sql);
            const result = stmt.get(...(payload.params || []));
            parentPort?.postMessage({ id, success: true, data: result });
        } else if (type === "prepareRun") {
            if (!db) throw new Error("Database not initialized");
            const stmt = db.prepare(payload.sql);
            const result = stmt.run(...(payload.params || []));
            parentPort?.postMessage({ id, success: true, data: result });
        } else if (type === "exec") {
            if (!db) throw new Error("Database not initialized");
            db.exec(payload.sql);
            parentPort?.postMessage({ id, success: true });
        } else if (type === "close") {
            if (db) {
                db.close();
                db = null;
            }
            parentPort?.postMessage({ id, success: true });
        }
    } catch (err: unknown) {
        const errMsg = err instanceof Error ? err.message : String(err);
        // Gửi log lỗi về Main Thread thay vì dùng console.log
        parentPort?.postMessage({ 
            id, 
            success: false, 
            error: errMsg, 
            isBusy: errMsg.includes("busy") || errMsg.includes("locked") 
        });
    }
});
