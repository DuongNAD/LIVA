import { google } from "googleapis";
import { logger } from "@utils/logger";
import { getGoogleAuthClient } from "@utils/googleAuth";

export const metadata = {
  name: "create_google_doc",
  search_keywords: ["create_google_doc","create google doc","google","drive","docs","doc","tài liệu","văn bản"],
  description:
    "[ASK_FIRST] Create a new Google Docs document with given title and content. Returns the document link.",
  parameters: {
    type: "object",
    properties: {
      title: {
        type: "string",
        description:
          "Document title. Example: 'Business Plan'.",
      },
      content: {
        type: "string",
        description: "Text content to write into the document.",
      },
    },
    required: ["title", "content"],
  },
};

export const execute = async (args: any) => {
  try {
    logger.info("[Skill: create_google_doc] Đang khởi động tiến trình...");
    const auth = await getGoogleAuthClient();
    const docs = google.docs({ version: "v1", auth });

    // Tạo tài liệu trống với title
    const createResponse = await docs.documents.create({
      requestBody: {
        title: args.title,
      },
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
              text: args.content,
            },
          },
        ],
      },
    });

    const url = `https://docs.google.com/document/d/${documentId}/edit`;
    return `✅ Đã tạo tài liệu thành công.\nTên file: ${args.title}\nLink truy cập: ${url}`;
  } catch (e: unknown) {
    const errMsg = e instanceof Error ? e.message : String(e);
    return `❌ Lỗi khi tạo Google Doc: ${errMsg}`;
  }
};
