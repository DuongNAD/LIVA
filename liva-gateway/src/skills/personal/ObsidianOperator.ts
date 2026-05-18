import { ObsidianVaultManager } from "@memory/ObsidianVaultManager";
import { logger } from "@utils/logger";
import path from "node:path";

// Khởi tạo vault manager - lấy đường dẫn từ biến môi trường hoặc thư mục data mặc định
const defaultVaultPath = process.env.OBSIDIAN_VAULT_PATH || path.join(process.cwd(), "data", "obsidian_vault");
const vaultManager = new ObsidianVaultManager(defaultVaultPath);

export const metadata = {
  name: "obsidian_operator",
  description: "[AUTO_RUN] Interact with the internal Obsidian Vault (read, create, overwrite notes) safely via O-NEXUS Guard.",
  kit: "OBSIDIAN_KIT",
  parameters: {
    type: "object",
    properties: {
      action: {
        type: "string",
        enum: ["read", "create", "append"],
        description: "Action: 'read' to read note content, 'create' to create/overwrite note, 'append' to add content to existing note.",
      },
      relativePath: {
        type: "string",
        description: "Relative path of the Markdown file (e.g., 'projects/Datathon.md').",
      },
      content: {
        type: "string",
        description: "Content to write (required for 'create' and 'append' actions). Supports YAML Frontmatter.",
      },
    },
    required: ["action", "relativePath"],
  },
};

export const execute = async (args: { action: "read" | "create" | "append"; relativePath: string; content?: string }): Promise<string> => {
  try {
    const { action, relativePath, content } = args;
    
    // Tự động gắn đuôi .md nếu LLM quên
    const safeExtPath = relativePath.endsWith(".md") ? relativePath : `${relativePath}.md`;

    if (action === "read") {
        logger.info(`[ObsidianOperator] Đang đọc note qua Proxy: ${safeExtPath}`);
        const result = await vaultManager.readNote(safeExtPath);
        return `[OBSIDIAN READ SUCCESS] File: ${safeExtPath}\nLast Modified: ${new Date(result.mtimeMs).toISOString()}\n\nContent:\n${result.content}`;
    }

    if (action === "create") {
        if (!content) throw new Error("Thiếu tham số 'content' cho thao tác create.");
        logger.info(`[ObsidianOperator] Tạo/Ghi đè note qua Proxy: ${safeExtPath}`);
        await vaultManager.createOrOverwriteNote(safeExtPath, content);
        return `[OBSIDIAN CREATE SUCCESS] File ${safeExtPath} đã được tạo thành công với kiến trúc Atomic Write.`;
    }

    if (action === "append") {
        if (!content) throw new Error("Thiếu tham số 'content' cho thao tác append.");
        logger.info(`[ObsidianOperator] Append note qua Proxy: ${safeExtPath}`);
        // Chèn vào cuối file (bỏ qua strict concurrency check do luồng agent là single-threaded theo lane)
        await vaultManager.safeAppendInsights(safeExtPath, content, Date.now() + 60000); 
        return `[OBSIDIAN APPEND SUCCESS] Đã chèn thêm nội dung vào cuối file ${safeExtPath} thành công.`;
    }

    return `Lỗi: Hành động '${action}' không được hỗ trợ.`;
  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    logger.error(`[ObsidianOperator] Thao tác thất bại: ${errMsg}`);
    return `[OBSIDIAN ERROR] Lỗi hệ thống: ${errMsg}. Chú ý: TUYỆT ĐỐI không cố gắng dùng 'execute_command' để sửa file này, hệ thống sẽ chặn!`;
  }
};
