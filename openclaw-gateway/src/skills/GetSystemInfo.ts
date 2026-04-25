import * as os from "node:os";
import { logger } from "../utils/logger";

export const metadata = {
  name: "get_system_info",
  search_keywords: ["get_system_info","get system info"],
  description:
    "Lấy thông tin cấu hình và trạng thái hiện tại của hệ thống máy tính (Hardware specs and system status), bao gồm CPU, RAM và Hệ điều hành.",
  parameters: {
    type: "object",
    properties: {},
    required: [],
  },
};

export const execute = async (): Promise<string> => {
  try {
    logger.info(
      `[Skill: get_system_info] Đang trích xuất thông tin phần cứng (Extracting hardware info)...`,
    );

    const platform = os.platform();
    const release = os.release();
    const arch = os.arch();
    const cpus = os.cpus();
    const totalMem = (os.totalmem() / 1024 ** 3).toFixed(2); // Đổi sang GB
    const freeMem = (os.freemem() / 1024 ** 3).toFixed(2); // Đổi sang GB

    const cpuModel = cpus.length > 0 ? cpus[0].model : "Unknown CPU";
    const cpuCores = cpus.length;

    const report = `
Báo cáo Hệ thống (System Report):
- Hệ điều hành (OS): ${platform} ${release} (${arch})
- Bộ vi xử lý (CPU): ${cpuModel} (${cpuCores} cores)
- Tổng dung lượng RAM (Total Memory): ${totalMem} GB
- RAM đang trống (Free Memory): ${freeMem} GB
        `;
    return report.trim();
  } catch (error: any) {
    return `Lỗi khi lấy thông tin hệ thống (System info error): ${error.message}`;
  }
};
