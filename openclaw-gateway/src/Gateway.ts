import * as dotenv from "dotenv";

// [STDOUT_GUARD] - Bảo vệ IPC Handshake khỏi rác Console
// eslint-disable-next-line no-console
const originalLog = console.log;
// eslint-disable-next-line no-console
console.log = (...args) => console.error('[STDOUT_GUARD]', ...args);
// eslint-disable-next-line no-console
console.warn = (...args) => console.error('[STDOUT_GUARD_WARN]', ...args);
// eslint-disable-next-line no-console
console.info = (...args) => console.error('[STDOUT_GUARD_INFO]', ...args);

import { CoreKernel } from "./core/CoreKernel";
import { logger } from "./utils/logger";
import { AutoGPUSetup } from "./scripts/AutoGPUSetup";

dotenv.config();

// [ANTI-ZOMBIE GUARD]
// Khi Tauri Frontend bị đóng, luồng process.stdin sẽ bị cắt (EOF).
process.stdin.resume(); // Giữ luồng mở
process.stdin.on('end', () => {
    logger.warn("🛑 Nhận tín hiệu EOF từ Stdio (Frontend đã đóng). Thực thi Auto-Kill Sidecar...");
    if ((global as any).kernelInstance) {
        (global as any).kernelInstance.shutdown();
    }
    process.exit(0);
});

async function start() {
  try {
    const kernel = new CoreKernel();
    (global as any).kernelInstance = kernel;
    
    await kernel.fetchSystemLocation();

    // Kích hoạt Auto GPU Setup ngầm TRƯỚC bootstrap
    // (WebSocket chưa mở nhưng setup chạy nhanh < 2s, kết quả lưu vào hardware_state.json)
    await AutoGPUSetup.runAutoSetupIfNeeded((msg) => {
        logger.info(`[AutoGPU] ${msg}`);
    });

    await kernel.bootstrap();
  } catch (e: any) {
    logger.error("System Fatal Error:", e.stack || e);
  }
}

start();
