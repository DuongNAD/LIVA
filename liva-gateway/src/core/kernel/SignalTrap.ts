import type { CoreKernel } from "../CoreKernel";
import { logger } from "../../utils/logger";

export class SignalTrap {
  public static listen(kernel: CoreKernel): void {
    process.stdin.resume(); // Keep stdin stream open
    process.stdin.on("end", () => {
      logger.warn("🛑 Nhận tín hiệu EOF từ Stdio (Frontend đã đóng). Thực thi Auto-Kill Sidecar...");
      SignalTrap.shutdownGracefully(kernel);
    });

    process.on("SIGINT", () => {
      logger.warn("🛑 Nhận tín hiệu SIGINT (Ctrl+C). Đang đóng các file an toàn...");
      SignalTrap.shutdownGracefully(kernel);
    });

    process.on("SIGTERM", () => {
      logger.warn("🛑 Nhận tín hiệu SIGTERM. Đang đóng các file an toàn...");
      SignalTrap.shutdownGracefully(kernel);
    });
  }

  private static async shutdownGracefully(kernel: CoreKernel): Promise<void> {
    logger.warn("⏳ [Data Loss Prevention] Bắt đầu ép xả (Force Flush) Write-Behind Cache...");
    try {
      await kernel.shutdown();
    } catch (e) {
      logger.error(`Error during graceful shutdown: ${e instanceof Error ? e.message : String(e)}`);
    }
    logger.info("✅ [Data Loss Prevention] Đã xả đệm an toàn. Tắt tiến trình.");
    process.exit(0);
  }
}
