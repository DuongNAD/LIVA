import { google } from 'googleapis';
import { getGoogleAuthClient } from '../utils/googleAuth';

export const metadata = {
    name: "create_google_doc",
    description: "Tạo một tài liệu Google Docs mới với tiêu đề và nội dung được cấp. Trả về đường link của tài liệu vừa tạo.",
    parameters: {
        type: "object",
        properties: {
            title: { type: "string", description: "Tiêu đề của tài liệu cần tạo. Ví dụ: 'Kế hoạch kinh doanh'." },
            content: { type: "string", description: "Nội dung văn bản muốn ghi vào tài liệu." }
        },
        required: ["title", "content"]
    }
};

export const execute = async (args: any) => {
    try {
        const auth = await getGoogleAuthClient();
        const docs = google.docs({ version: 'v1', auth });

        // Tạo tài liệu trống với title
        const createResponse = await docs.documents.create({
            requestBody: {
                title: args.title
            }
        });

        const documentId = createResponse.data.documentId;
        if (!documentId) return "Lỗi: Không thể lấy document ID lúc khởi tạo.";

        // Thêm nội dung vào tài liệu
        await docs.documents.batchUpdate({
            documentId: documentId,
            requestBody: {
                requests: [
                    {
                        insertText: {
                            location: {
                                index: 1,
                            },
                            text: args.content
                        }
                    }
                ]
            }
        });

        const url = `https://docs.google.com/document/d/${documentId}/edit`;
        return `✅ Đã tạo tài liệu thành công.\nTên file: ${args.title}\nLink truy cập: ${url}`;
    } catch (e: any) {
        return `❌ Lỗi khi tạo Google Doc: ${e.message}`;
    }
};
