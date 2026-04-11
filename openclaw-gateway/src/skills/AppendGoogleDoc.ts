import { google } from "googleapis";
import { getGoogleAuthClient } from "../utils/googleAuth";

export const metadata = {
  name: "append_google_doc",
  search_keywords: ["append_google_doc","append google doc","google","driver","sheet"],
  description:
    "Thêm nội dung văn bản (append text) vào cuối một tài liệu Google Docs có sẵn.",
  parameters: {
    type: "object",
    properties: {
      documentId: {
        type: "string",
        description:
          "ID của tài liệu Google Docs (Lấy được từ search_google_drive hoặc URL).",
      },
      text: {
        type: "string",
        description:
          "Đoạn văn bản muốn ghi tiếp vào cuối tài liệu. Sử dụng dấu \\n để xuống dòng.",
      },
    },
    required: ["documentId", "text"],
  },
};

export const execute = async (args: any) => {
  try {
    console.log("[Skill: append_google_doc] Đang khởi động tiến trình...");
    const auth = await getGoogleAuthClient();
    const docs = google.docs({ version: "v1", auth: auth as any });

    // Lấy thông tin tài liệu hiện tại để tìm vị trí index cuối cùng
    const docInfo = await docs.documents.get({
      documentId: args.documentId,
    });

    const content = docInfo.data.body?.content;
    if (!content) return "Tài liệu trống hoặc không hợp lệ.";

    let endIndexOfDoc = 1;
    // Điểm index cuối cùng của text element cuối cùng
    const lastElement = content[content.length - 1];
    if (lastElement && lastElement.endIndex) {
      endIndexOfDoc = lastElement.endIndex;
    }

    // Chèn đoạn text vào cuối
    await docs.documents.batchUpdate({
      documentId: args.documentId,
      requestBody: {
        requests: [
          {
            insertText: {
              location: {
                // -1 để nằm gọn trong thẻ section cuối
                index: endIndexOfDoc - 1,
              },
              text: "\n" + args.text,
            },
          },
        ],
      },
    });

    const url = `https://docs.google.com/document/d/${args.documentId}/edit`;
    return `✅ Đã thêm nội dung vào cuối tài liệu.\nLink: ${url}`;
  } catch (e: any) {
    return `❌ Lỗi khi ghi thêm nội dung vào Google Doc: ${e.message}`;
  }
};
