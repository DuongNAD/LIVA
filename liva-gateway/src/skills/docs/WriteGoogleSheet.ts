import { google } from "googleapis";
import { logger } from "@utils/logger";
import { getGoogleAuthClient } from "@utils/googleAuth";

export const metadata = {
  name: "write_google_sheet",
  search_keywords: ["write_google_sheet","write google sheet","google","drive","sheet","sheets","excel","bảng tính","ghi sheet","nhập sheet"],
  description:
    "[ASK_FIRST] Append one or more data rows to a Google Sheet.",
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
          "Target range or sheet name (e.g., 'Sheet1!A1'). System auto-finds next empty row.",
      },
      values: {
        type: "array",
        items: {
          type: "array",
          items: { type: "string" },
        },
        description:
          "2D array of row data, e.g., [['Col 1', 'Col 2'], ['Val 1', 'Val 2']]",
      },
    },
    required: ["spreadsheetId", "range", "values"],
  },
};

export const execute = async (args: any) => {
  try {
    logger.info("[Skill: write_google_sheet] Đang khởi động tiến trình...");
    const auth = await getGoogleAuthClient();
    const sheets = google.sheets({ version: "v4", auth });

    // Sử dụng append thay vì update để không chèn lên dữ liệu cũ
    const response = await sheets.spreadsheets.values.append({
      spreadsheetId: args.spreadsheetId,
      range: args.range,
      valueInputOption: "USER_ENTERED",
      insertDataOption: "INSERT_ROWS",
      requestBody: {
        values: args.values,
      },
    });

    const updatedCells = response.data.updates?.updatedCells;
    return `✅ Đã thêm ${updatedCells} ô dữ liệu vào bảng tính ${args.spreadsheetId}.`;
  } catch (e: unknown) {
    const errMsg = e instanceof Error ? e.message : String(e);
    return `❌ Lỗi khi ghi dữ liệu vào Google Sheet: ${errMsg}`;
  }
};
