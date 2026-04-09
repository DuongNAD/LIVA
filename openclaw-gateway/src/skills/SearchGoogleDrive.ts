import { google } from 'googleapis';
import { getGoogleAuthClient } from '../utils/googleAuth';

export const metadata = {
    name: "search_google_drive",
    description: "Tìm kiếm tệp trên hệ thống đám mây Google Drive (tìm files, sheets, docs, v.v).",
    parameters: {
        type: "object",
        properties: {
            query: { type: "string", description: "Câu truy vấn theo chuẩn Drive API, ví dụ: name contains 'Báo cáo' hoặc mimeType='application/vnd.google-apps.spreadsheet'" }
        },
        required: ["query"]
    }
};

export const execute = async (args: any) => {
    try {
        const auth = await getGoogleAuthClient();
        const drive = google.drive({ version: 'v3', auth: auth as any });

        const response = await drive.files.list({
            q: args.query,
            fields: "files(id, name, mimeType)",
            pageSize: 10,
        });

        const files = response.data.files;
        if (!files || files.length === 0) {
            return `Không tìm thấy file nào khớp với truy vấn: ${args.query}`;
        }

        let output = "[Kết quả tìm kiếm trên Drive]:\n";
        files.forEach(file => {
            output += `- Tên file: ${file.name} | ID: ${file.id} | Loại: ${file.mimeType}\n`;
        });
        output += `\n(Ghi chú cho AI: Bạn có thể lấy ID ở trên và nạp vào các tool như read_google_sheet hoặc append_google_doc).`;

        return output;
    } catch (e: any) {
        return `❌ Lỗi khi tìm kiếm trên Google Drive: ${e.message}`;
    }
};
