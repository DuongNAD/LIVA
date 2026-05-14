import { google } from "googleapis";
import { logger } from "@utils/logger";
import { getGoogleAuthClient } from "@utils/googleAuth";

export const metadata = {
  name: "append_google_doc",
  search_keywords: ["append_google_doc","append google doc","google","driver","sheet"],
  description:
    "[AUTO_RUN] Append text to the end of an existing Google Docs document.",
  parameters: {
    type: "object",
    properties: {
      documentId: {
        type: "string",
        description:
          "Google Docs document ID (from search_google_drive or URL).",
      },
      text: {
        type: "string",
        description:
          "Text to append to the document. Use \\n for line breaks.",
      },
    },
    required: ["documentId", "text"],
  },
};

export const execute = async (args: any) => {
  try {
    logger.info("[Skill: append_google_doc] Đang khởi động tiến trình...");
    const auth = await getGoogleAuthClient();
    const docs = google.docs({ version: "v1", auth });

    // Lấy thông tin tài liệu hiện tại để tìm vị trí index cuối cùng
    const docInfo = await docs.documents.get({
      documentId: args.documentId,
    });

    const content = docInfo.data.body?.content;
    if (!content) return "Tài liệu trống hoặc không hợp lệ.";

    let endIndexOfDoc = 1;
    // Điểm index cuối cùng của text element cuối cùng
    const lastElement = content[content.length - 1];
    if (lastElement?.endIndex) {
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
  } catch (e: unknown) {
    const errMsg = e instanceof Error ? e.message : String(e);
    return `❌ Lỗi khi ghi thêm nội dung vào Google Doc: ${errMsg}`;
  }
};
