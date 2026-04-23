import { google } from "googleapis";
import { logger } from "../utils/logger";
import { getGoogleAuthClient } from "../utils/googleAuth";

import { logger } from "../utils/logger";
export const metadata = {
  name: "read_google_sheet",
  search_keywords: ["read_google_sheet","read google sheet","google","driver","sheet"],
  description:
    "Đọc dữ liệu từ bảng tính Google Sheets trong một vùng (range) nhất định đã định sẵn, hỗ trợ phân tích dữ liệu mảng.",
  parameters: {
    type: "object",
    properties: {
      spreadsheetId: {
        type: "string",
        description: "ID của bảng tính (từ URL).",
      },
      range: {
        type: "string",
        description:
          "Vùng dữ liệu cần đọc, VD: 'Sheet1!A1:D10' hoặc chỉ tên trang tính 'Trang tính1'.",
      },
    },
    required: ["spreadsheetId", "range"],
  },
};

export const execute = async (args: any) => {
  try {
    logger.info("[Skill: read_google_sheet] Đang khởi động tiến trình...");
    const auth = await getGoogleAuthClient();
    const sheets = google.sheets({ version: "v4", auth: auth as any });

    const response = await sheets.spreadsheets.values.get({
      spreadsheetId: args.spreadsheetId,
      range: args.range,
    });

    const rows = response.data.values;
    if (!rows || rows.length === 0) {
      return `Không tìm thấy dữ liệu nào trong khoảng ${args.range}.`;
    }

    let output = `[Bảng dữ liệu từ ${args.range}]:\n`;
    rows.forEach((row, index) => {
      output += `Dòng ${index + 1}: ${row.join(" | ")}\n`;
    });

    return output;
  } catch (e: any) {
    return `❌ Lỗi khi đọc Google Sheet: ${e.message}`;
  }
};
