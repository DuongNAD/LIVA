import * as fsp from 'node:fs/promises';
import * as fs from "node:fs";
import * as path from "node:path";
import { logger } from "./logger";

const SHIELD_DIR = path.join(process.cwd(), ".liva_shield");
const SNAPSHOT_DIR = path.join(SHIELD_DIR, "snapshot_latest");

export class ShieldGuard {
    public static async deploy() {
        try {
            if (!fs.existsSync(SHIELD_DIR)) {
                await fsp.mkdir(SHIELD_DIR);
            }
            
            // Xóa hầm an toàn cũ
            if (fs.existsSync(SNAPSHOT_DIR)) {
                await fsp.rm(SNAPSHOT_DIR, { recursive: true, force: true });
            }
            await fsp.mkdir(SNAPSHOT_DIR);

            // Copy src
            logger.info("🛡️ [ShieldGuard] Đang sao lưu Code lõi (src/) sang Vùng An Toàn...");
            await fsp.cp(path.join(process.cwd(), "src"), path.join(SNAPSHOT_DIR, "src"), { recursive: true });

            // Copy data
            logger.info("🛡️ [ShieldGuard] Đang sao lưu Trí nhớ (data/) sang Vùng An Toàn...");
            if (fs.existsSync(path.join(process.cwd(), "data"))) {
                await fsp.cp(path.join(process.cwd(), "data"), path.join(SNAPSHOT_DIR, "data"), { recursive: true });
            }

            logger.info("✅ [ShieldGuard] Hệ thống phòng ngự đã kích hoạt! Đã Snapshot thành công.");
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            logger.error("⛔ [ShieldGuard] Lỗi kích hoạt khiên:" + " " + errMsg);
        }
    }

    public static async rollback() {
        logger.warn("⚠️ [ShieldGuard] KHỞI CHẠY QUY TRÌNH PHỤC HỒI KÍ TỰ...");
        if (!fs.existsSync(SNAPSHOT_DIR)) {
            logger.error("⛔ [ShieldGuard] Không tìm thấy bản Snapshot nào!");
            return;
        }

        try {
            // Khôi phục src
            await fsp.cp(path.join(SNAPSHOT_DIR, "src"), path.join(process.cwd(), "src"), { recursive: true, force: true });
            
            // Khôi phục data
            const dataBackupPath = path.join(SNAPSHOT_DIR, "data");
            if (fs.existsSync(dataBackupPath)) {
                await fsp.cp(dataBackupPath, path.join(process.cwd(), "data"), { recursive: true, force: true });
            }
            
            logger.info("🎯 [ShieldGuard] Phục hồi THÀNH CÔNG! Mã nguồn đã quay về trạng thái ổn định gần nhất.");
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            logger.error("⛔ [ShieldGuard] Lỗi phục hồi:" + " " + errMsg);
        }
    }
}

// Hỗ trợ gọi từ CLI (npm run shield:restore)
const args = process.argv.slice(2);
if (args.includes("--rollback")) {
    ShieldGuard.rollback().then(() => process.exit(0));
}
