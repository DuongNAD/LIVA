import { google } from 'googleapis';
import { getGoogleAuthClient } from '../utils/googleAuth';

export const metadata = {
    name: "write_google_sheet",
    description: "Thêm một hoặc nhiều dòng dữ liệu mới vào bảng tính Google Sheets.",
    parameters: {
        type: "object",
        properties: {
            spreadsheetId: { type: "string", description: "ID của bảng tính (từ URL)." },
            range: { type: "string", description: "Vùng muốn ghi hoặc tên sheet (VD: 'Sheet1!A1'). Hệ thống sẽ tự động tìm dòng trống tiếp theo để điền xuống dưới." },
            values: { 
                type: "array", 
                items: {
                    type: "array",
                    items: { type: "string" }
                },
                description: "Mảng hai chiều chứa dữ liệu các dòng, ví dụ: [['Cột 1', 'Cột 2'], ['Giá trị 1', 'Giá trị 2']]"
            }
        },
        required: ["spreadsheetId", "range", "values"]
    }
};

export const execute = async (args: any) => {
    try {
        const auth = await getGoogleAuthClient();
        const sheets = google.sheets({ version: 'v4', auth: auth as any });

        // Sử dụng append thay vì update để không chèn lên dữ liệu cũ
        const response = await sheets.spreadsheets.values.append({
            spreadsheetId: args.spreadsheetId,
            range: args.range,
            valueInputOption: 'USER_ENTERED',
            insertDataOption: 'INSERT_ROWS',
            requestBody: {
                values: args.values,
            },
        });

        const updatedCells = response.data.updates?.updatedCells;
        return `✅ Đã thêm ${updatedCells} ô dữ liệu vào bảng tính ${args.spreadsheetId}.`;
    } catch (e: any) {
        return `❌ Lỗi khi ghi dữ liệu vào Google Sheet: ${e.message}`;
    }
};
