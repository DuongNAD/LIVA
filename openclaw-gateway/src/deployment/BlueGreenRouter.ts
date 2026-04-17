import * as fs from "fs";
import * as path from "path";

/**
 * Blue-Green Router (Van An Toàn Của Sinh Mệnh Dự Án) - V7 ATOMIC BATCH DEPLOY
 * Xử lý Deploy ứng viên từ Sandbox ra Ánh sáng (Host gốc).
 * Auto-Rollback toàn bộ thư mục về .bak trong 1ms nếu gặp lỗi.
 */
export class BlueGreenRouter {
    private hostWorkspace: string;

    constructor(workspace: string) {
         this.hostWorkspace = workspace;
    }

    /**
     * Deploy Biến thể sống sót (cả thư mục) vào vùng GREEN một cách nguyên tử.
     */
    public async deployToGreenBatch(sandboxRoot: string): Promise<boolean> {
         const originalSrcPath = path.join(this.hostWorkspace, "src");
         const sandboxSrcPath = path.join(sandboxRoot, "src");
         const backupSrcPath = path.join(this.hostWorkspace, ".src.blue.bak");

         try {
             // 1. PHASE 1: Tạo điểm Snapshot BLUE toàn bộ thư mục src
             if (fs.existsSync(backupSrcPath)) {
                 fs.rmSync(backupSrcPath, { recursive: true, force: true });
             }
             if (fs.existsSync(originalSrcPath)) {
                 fs.cpSync(originalSrcPath, backupSrcPath, { recursive: true });
             }

             // 2. PHASE 2: Chuyển toàn bộ Sandbox (đã sửa/thêm) đè lên GREEN
             fs.cpSync(sandboxSrcPath, originalSrcPath, { recursive: true, force: true });
             
             // 3. Dọn dẹp Sandbox
             if (fs.existsSync(sandboxRoot)) fs.rmSync(sandboxRoot, { recursive: true, force: true });
             
             console.log(`\n🟢 [Deployer] Khởi chạy GREEN: Ghi mã thành công cấu trúc toàn diện từ Sandbox!`);
             return true;
         } catch(e: any) {
             console.error("🔴 Lỗi Deploy Green Router (ATOMIC TRIGGERED):", e.message);
             // Tự động kích hoạt Rollback do bị EPERM / Lỗi bất ngờ
             await this.autoRollbackBatch();
             return false;
         }
    }

    /**
     * Nút Giáng Khẩn Cấp (Panic Fallback) cấp Thư mục
     * Kéo mã từ BLUE Snapshot về lại gốc nếu Hệ sinh thái sụp đổ.
     */
    public async autoRollbackBatch(): Promise<boolean> {
        const originalSrcPath = path.join(this.hostWorkspace, "src");
        const backupSrcPath = path.join(this.hostWorkspace, ".src.blue.bak");

        try {
            if (fs.existsSync(backupSrcPath)) {
                // Xóa thư mục src hiện tại (bị hỏng)
                if (fs.existsSync(originalSrcPath)) {
                    fs.rmSync(originalSrcPath, { recursive: true, force: true });
                }
                // Phục hồi từ bak
                fs.cpSync(backupSrcPath, originalSrcPath, { recursive: true });
                console.log(`\n🔴 [Hệ Miễn Dịch Deployer] AUTO-ROLLBACK KÍCH HOẠT! Toàn bộ thư mục src/ đã được cứu sống về BLUE.`);
                return true;
            }
            return false;
        } catch (e: any) {
            console.error("🔴 Fatal Error khi Rollback Toàn Khu:", e.message);
            return false;
        }
    }

    // Giữ lại hàm cũ phòng trường hợp có chỗ gọi lẻ tẻ, trỏ nó về fail an toàn
    public async autoRollback(originalFilePath: string): Promise<boolean> {
        return this.autoRollbackBatch();
    }
}
