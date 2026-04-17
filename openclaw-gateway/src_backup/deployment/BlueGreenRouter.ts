import * as fs from "fs";
import * as path from "path";

/**
 * Blue-Green Router (Van An Toàn Của Sinh Mệnh Dự Án)
 * Xử lý Deploy ứng viên Pareto từ Bóng tối (Shadow) ra Ánh sáng (Host gốc).
 * Auto-Rollback về .bak trong 1ms nếu Test Network/Traffic vọt OOM.
 */
export class BlueGreenRouter {
    private hostWorkspace: string;

    constructor(workspace: string) {
         this.hostWorkspace = workspace;
    }

    /**
     * Deploy Biến thể sống sót GEPA vào vùng GREEN
     */
    public async deployToGreen(shadowFilePath: string, originalFilePath: string): Promise<boolean> {
         try {
             const originalAbsPath = path.isAbsolute(originalFilePath) 
                ? originalFilePath 
                : path.join(this.hostWorkspace, originalFilePath);

             // 1. Tạo điểm Snapshot BLUE (Mã gốc cũ trước khi sửa)
             if (fs.existsSync(originalAbsPath)) {
                 const backupPath = `${originalAbsPath}.blue.bak`;
                 fs.copyFileSync(originalAbsPath, backupPath);
             }

             // 2. Chuyển Shadow Pareto lên GREEN
             fs.copyFileSync(shadowFilePath, originalAbsPath);
             // 3. Dọn dẹp xác chết Shadow để không làm rác thư mục mã nguồn
             if (fs.existsSync(shadowFilePath)) fs.unlinkSync(shadowFilePath);
             
             console.log(`\n🟢 [Deployer] Khởi chạy GREEN: Ghi mã thành công lên ${path.basename(originalAbsPath)}`);
             return true;
         } catch(e: any) {
             console.error("🔴 Lỗi Deploy Green Router:", e.message);
             return false;
         }
    }

    /**
     * Nút Giáng Khẩn Cấp (Panic Fallback 1ms)
     * Kéo mã từ BLUE Snapshot về lại gốc nếu Hệ sinh thái sụp đổ.
     */
    public async autoRollback(originalFilePath: string): Promise<boolean> {
        try {
            const originalAbsPath = path.isAbsolute(originalFilePath) 
                ? originalFilePath 
                : path.join(this.hostWorkspace, originalFilePath);
            const backupPath = `${originalAbsPath}.blue.bak`;

            if (fs.existsSync(backupPath)) {
                fs.copyFileSync(backupPath, originalAbsPath);
                console.log(`\n🔴 [Hệ Miễn Dịch Deployer] AUTO-ROLLBACK KÍCH HOẠT! Tệp ${path.basename(originalFilePath)} đã được cứu sống về BLUE.`);
                return true;
            }
            return false;
        } catch (e: any) {
            console.error("🔴 Fatal Error khi Rollback:", e.message);
            return false;
        }
    }
}
