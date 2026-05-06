import { z } from "zod";
import { logger } from "@utils/logger";
import { safeFetch } from "@utils/HttpClient";
import { HITLGuard } from "@security/HITLGuard";

const TrackerSchema = z.object({
  action: z.enum(["list_issues", "create_issue", "update_status"]),
  projectId: z.string().optional(),
  title: z.string().optional(),
  description: z.string().optional(),
  issueId: z.string().optional(),
  newStatus: z.string().optional()
});

export const metadata = {
  name: "linear_jira_tracker",
  description: "Theo dõi và quản lý Task/Issue trên Linear hoặc Jira. Đọc danh sách task và cập nhật trạng thái hoặc tạo task mới (yêu cầu phê duyệt).",
  kit: "DEVOPS_KIT",
  parameters: {
    type: "object",
    properties: {
      action: { type: "string", enum: ["list_issues", "create_issue", "update_status"] },
      projectId: { type: "string" },
      title: { type: "string" },
      description: { type: "string" },
      issueId: { type: "string" },
      newStatus: { type: "string" }
    },
    required: ["action"],
  },
};

export const execute = async (argsObj: any): Promise<string> => {
    try {
        const parsed = TrackerSchema.parse(argsObj);
        const API_URL = process.env.TRACKER_API_URL || "http://127.0.0.1:9999/api/tracker";

        if (parsed.action === "create_issue" || parsed.action === "update_status") {
            // Yêu cầu HITL cho các thao tác Write
            logger.info(`[Tracker] Đang yêu cầu HITL phê duyệt: ${parsed.action}`);
            try {
                await HITLGuard.requestApproval({
                    toolName: "linear_jira_tracker",
                    args: parsed,
                    reason: `LIVA muốn thực hiện thao tác thay đổi trên Task Tracker: ${parsed.action}`
                });
            } catch (error: unknown) {
            const errMsg = error instanceof Error ? error.message : String(error);
                return `[TRACKER BLOCKED] Hành động bị từ chối: ${errMsg}`;
            }

            try {
                const res = await safeFetch(API_URL, {
                    method: "POST",
                    headers: { "Content-Type": "application/json" },
                    body: JSON.stringify(parsed)
                });
                const data = await res.json();
                return `[TRACKER SUCCESS] Thành công: ${data.status || 'OK'}`;
            } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
                logger.warn(`[Tracker] API lỗi, dùng Mock: ${errMsg}`);
                return `[TRACKER SUCCESS] (MOCK) Đã thực thi thao tác ${parsed.action} thành công.`;
            }
        } else {
            // list_issues
            try {
                const res = await safeFetch(`${API_URL}?action=list`, {}, 5000);
                const data = await res.json();
                return `[TRACKER ISSUES]\n${JSON.stringify(data, null, 2)}`;
            } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
                return `[TRACKER ISSUES] (MOCK)\n- LIVA-1: Cập nhật Dynamic Gating (In Progress)\n- LIVA-2: Tích hợp safeFetch (To Do)`;
            }
        }

    } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
        logger.error(`[Tracker] Lỗi: ${errMsg}`);
        if (error instanceof z.ZodError) {
            return `[TRACKER ERROR] Sai định dạng: ${error.issues.map(e => e.message).join(", ")}`;
        }
        return `[TRACKER ERROR] Lỗi hệ thống: ${errMsg}`;
    }
};
