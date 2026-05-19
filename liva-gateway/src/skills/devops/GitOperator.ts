import { z } from "zod";
import { logger } from "@utils/logger";
import { exec } from "node:child_process";
import { promisify } from "node:util";
import { HITLGuard } from "@security/HITLGuard";
import path from "node:path";

const execAsync = promisify(exec);

// Zod Schema cho Git actions
const GitOperatorSchema = z.object({
  action: z.enum(["status", "log", "diff", "commit", "push", "checkout", "pull", "add"]),
  args: z.array(z.string()).optional().default([]),
  repoPath: z.string().optional().default(".")
});

export const metadata = {
  name: "git_operator",
  search_keywords: ["git", "commit", "push", "pull", "branch", "checkout", "diff", "merge", "mã nguồn"],
  description: "[ASK_FIRST] Interact with local Git. Supports status, log, diff, and safe operations (commit, push, pull, checkout) - requires HITL approval for state-changing commands.",
  kit: "DEVOPS_KIT",
  parameters: {
    type: "object",
    properties: {
      action: {
        type: "string",
        enum: ["status", "log", "diff", "commit", "push", "checkout", "pull", "add"],
        description: "Git command to execute.",
      },
      args: {
        type: "array",
        items: { type: "string" },
        description: "Additional command-line args for git (e.g., ['-m', 'Fix bug'] for commit, or ['origin', 'main'] for push).",
      },
      repoPath: {
        type: "string",
        description: "Repository directory path (defaults to current working directory).",
      }
    },
    required: ["action"],
  },
};

export const execute = async (argsObj: any): Promise<string> => {
    try {
        const parsed = GitOperatorSchema.parse(argsObj);
        const { action, args, repoPath } = parsed;
        const cwd = path.resolve(process.cwd(), repoPath);

        // Security: Chặn command injection
        const safeArgs = args.map(arg => {
            // Loại bỏ các ký tự shell nguy hiểm
            const cleanArg = arg.replace(/[&|;$><`\\]/g, "");
            return `"${cleanArg.replace(/"/g, '\\"')}"`;
        }).join(" ");

        const fullCommand = `git ${action} ${safeArgs}`.trim();
        
        // Phân loại Write Actions cần HITL Guard
        const writeActions = ["commit", "push", "checkout", "pull", "add"];
        
        if (writeActions.includes(action)) {
            logger.info(`[GitOperator] Hành động Write (${action}) yêu cầu phê duyệt HITL...`);
            
            try {
                await HITLGuard.requestApproval({
                    toolName: "git_operator",
                    args: { command: fullCommand, repo: cwd },
                    reason: `LIVA muốn thực thi lệnh git thay đổi trạng thái: \`${fullCommand}\``
                });
                logger.info(`[GitOperator] ✅ HITL Approved cho lệnh: ${fullCommand}`);
            } catch (error: unknown) {
            const errMsg = error instanceof Error ? error.message : String(error);
                logger.warn(`[GitOperator] ❌ HITL Bị từ chối: ${errMsg}`);
                return `[GIT ACTION BLOCKED] Thao tác '${fullCommand}' đã bị từ chối bởi người dùng hoặc hệ thống: ${errMsg}`;
            }
        } else {
            logger.info(`[GitOperator] Chạy lệnh Read-Only: ${fullCommand}`);
        }

        const { stdout, stderr } = await execAsync(fullCommand, { cwd, timeout: 15000 });
        
        let output = stdout.trim();
        if (stderr.trim()) {
            output += `\n[STDERR]:\n${stderr.trim()}`;
        }
        
        // Cắt bớt output nếu quá dài (VD: diff hoặc log)
        if (output.length > 3000) {
            output = output.substring(0, 3000) + "\n... (Output bị cắt bớt do quá dài)";
        }
        
        return `[GIT SUCCESS] Thực thi: ${fullCommand}\nThư mục: ${cwd}\n\n[OUTPUT]\n${output || "Done."}`;

    } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
        logger.error(`[GitOperator] Lỗi: ${errMsg}`);
        if (error instanceof z.ZodError) {
            return `[GIT ERROR] Sai định dạng tham số: ${error.issues.map(e => e.message).join(", ")}`;
        }
        return `[GIT ERROR] Lệnh thất bại: ${errMsg}`;
    }
};
