import { google } from "googleapis";
import { logger } from "@utils/logger";
import { getGoogleAuthClient } from "@utils/googleAuth";

export const metadata = {
  name: "read_google_sheet",
  search_keywords: ["read_google_sheet","read google sheet","google","drive","sheet","sheets","excel","bảng tính","đọc sheet"],
  description:
    "[AUTO_RUN] Read data from Google Sheets in a specified range, supports array data analysis.",
  parameters: {
    type: "object",
    properties: {
      spreadsheetId: {
        type: "string",
        description: "Spreadsheet ID (from URL).",
      },
      range: {
        type: "string",
        description:
          "Data range to read, e.g., 'Sheet1!A1:D10' or just sheet name.",
      },
    },
    required: ["spreadsheetId", "range"],
  },
};

export const execute = async (args: any) => {
  try {
    logger.info("[Skill: read_google_sheet] Đang khởi động tiến trình...");
    const auth = await getGoogleAuthClient();
    const sheets = google.sheets({ version: "v4", auth });

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
  } catch (e: unknown) {
    const errMsg = e instanceof Error ? e.message : String(e);
    return `❌ Lỗi khi đọc Google Sheet: ${errMsg}`;
  }
};
