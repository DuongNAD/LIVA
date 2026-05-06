import { z } from "zod";
import { logger } from "@utils/logger";
import { HITLGuard } from "@security/HITLGuard";
import { exec } from "node:child_process";
import { promisify } from "node:util";

const execAsync = promisify(exec);

const WorkspaceSchema = z.object({
  action: z.enum(["minimize_all", "lock_screen", "focus_mode", "shutdown", "restart", "sleep"]).describe("Hành động cần thực hiện trên máy tính người dùng")
});

export const metadata = {
  name: "workspace_manager",
  description: "Điều khiển cấp OS (Windows). Hỗ trợ: Thu nhỏ tất cả cửa sổ, Focus Mode, Khóa màn hình (Lock), Sleep, Restart, và Shutdown (Tắt máy).",
  kit: "PERSONAL_KIT",
  parameters: {
    type: "object",
    properties: {
      action: { type: "string", enum: ["minimize_all", "lock_screen", "focus_mode", "shutdown", "restart", "sleep"] }
    },
    required: ["action"],
  },
};

export const execute = async (argsObj: any): Promise<string> => {
    try {
        const parsed = WorkspaceSchema.parse(argsObj);

        if (["lock_screen", "shutdown", "restart", "sleep"].includes(parsed.action)) {
            // Write/Destructive Action -> Cần HITL Guard rất nghiêm ngặt
            logger.info(`[WorkspaceManager] Yêu cầu can thiệp hệ thống: ${parsed.action}. Chờ HITL phê duyệt...`);
            try {
                await HITLGuard.requestApproval({
                    toolName: "workspace_manager",
                    args: { action: parsed.action },
                    reason: `CẢNH BÁO: LIVA đang yêu cầu thực hiện hành động cấp hệ thống (${parsed.action}) trên máy tính của bạn!`
                });
            } catch (error: unknown) {
            const errMsg = error instanceof Error ? error.message : String(error);
                return `[WORKSPACE BLOCKED] Yêu cầu ${parsed.action} bị từ chối: ${errMsg}`;
            }

            if (parsed.action === "lock_screen") {
                await execAsync("rundll32.exe user32.dll,LockWorkStation");
                logger.info(`[WorkspaceManager] Đã khóa màn hình.`);
                return `[WORKSPACE SUCCESS] Đã thực hiện khóa màn hình thành công.`;
            }

            if (parsed.action === "shutdown") {
                // Tắt máy sau 10 giây (để user có thể huỷ lệnh bằng shutdown -a nếu cần)
                await execAsync("shutdown /s /t 10");
                logger.warn(`[WorkspaceManager] ĐÃ KÍCH HOẠT TẮT MÁY (Shutdown)!`);
                return `[WORKSPACE SUCCESS] Hệ thống sẽ TẮT (Shutdown) sau 10 giây. Để hủy, hãy chạy lệnh 'shutdown -a'.`;
            }

            if (parsed.action === "restart") {
                await execAsync("shutdown /r /t 10");
                logger.warn(`[WorkspaceManager] ĐÃ KÍCH HOẠT KHỞI ĐỘNG LẠI (Restart)!`);
                return `[WORKSPACE SUCCESS] Hệ thống sẽ KHỞI ĐỘNG LẠI (Restart) sau 10 giây. Để hủy, hãy chạy lệnh 'shutdown -a'.`;
            }

            if (parsed.action === "sleep") {
                // Đưa máy vào chế độ ngủ (Hibernate/Sleep)
                await execAsync("rundll32.exe powrprof.dll,SetSuspendState 0,1,0");
                logger.info(`[WorkspaceManager] Đã đưa máy vào chế độ Ngủ (Sleep).`);
                return `[WORKSPACE SUCCESS] Đã đưa máy tính vào chế độ Ngủ (Sleep).`;
            }
        }
        
        if (parsed.action === "minimize_all") {
            // Lệnh PowerShell thu nhỏ tất cả cửa sổ (Minimize All)
            const script = `(New-Object -ComObject Shell.Application).MinimizeAll()`;
            await execAsync(`powershell.exe -Command "${script}"`);
            logger.info(`[WorkspaceManager] Đã thu nhỏ mọi cửa sổ.`);
            return `[WORKSPACE SUCCESS] Đã thu nhỏ tất cả các cửa sổ để dọn dẹp màn hình Desktop.`;
        }

        if (parsed.action === "focus_mode") {
            // Giả lập Focus Mode: Minimize All rồi gửi IPC hiển thị thông báo
            const script = `(New-Object -ComObject Shell.Application).MinimizeAll()`;
            await execAsync(`powershell.exe -Command "${script}"`);
            
            // Bắn IPC Toast
            const ipcMessage = JSON.stringify({
                event: "SHOW_TOAST",
                payload: {
                    title: "Focus Mode Activated",
                    message: "Đã thu nhỏ các cửa sổ gây xao nhãng. Chúc bạn làm việc hiệu quả!",
                    type: "success",
                    duration: 5000
                }
            });
            process.stdout.write(ipcMessage + "\n");
            
            logger.info(`[WorkspaceManager] Đã kích hoạt Focus Mode.`);
            return `[WORKSPACE SUCCESS] Đã kích hoạt Focus Mode (Thu nhỏ cửa sổ và hiện thông báo).`;
        }

        return "Hành động không hợp lệ.";
    } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
        logger.error(`[WorkspaceManager] Lỗi: ${errMsg}`);
        if (error instanceof z.ZodError) {
            return `[WORKSPACE ERROR] Sai định dạng: ${error.issues.map(e => e.message).join(", ")}`;
        }
        return `[WORKSPACE ERROR] Lỗi hệ thống: ${errMsg}`;
    }
};
