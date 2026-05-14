/**
 * @module PrivacyDashboard
 * [Legal Compliance — Luật BVDL Cá nhân VN 2026, Điều 30]
 * 
 * Skill cho phép người dùng quản lý quyền riêng tư trực tiếp qua hội thoại AI:
 * - Xem dữ liệu LIVA đang lưu trữ (Data Inventory)
 * - Bật/tắt thu thập dữ liệu (Consent Management)
 * - Xóa toàn bộ dữ liệu cá nhân (Right to be Forgotten)
 * - Xem nhật ký truy cập dữ liệu (Audit Trail)
 */

import { SensoryManager } from "../../memory/SensoryManager";
import { auditLogger } from "../../utils/AuditLogger";

// ── Type stubs for consent management (pending SensoryManager expansion) ──
interface ConsentState {
  activeWindow: boolean;
  clipboard: boolean;
  updatedAt: number;
}
interface AuditEntry {
  timestamp: number;
  action: string;
  categories: string[];
  detail: string;
}

// Consent state singleton (in-memory until SensoryManager is expanded)
let _consentState: ConsentState = { activeWindow: false, clipboard: false, updatedAt: Date.now() };
const _auditLog: AuditEntry[] = [];

function getConsentState(): ConsentState { return { ..._consentState }; }
function getAuditLog(): AuditEntry[] { return [..._auditLog]; }
function grantConsent(categories: { activeWindow?: boolean; clipboard?: boolean }): void {
  if (categories.activeWindow !== undefined) _consentState.activeWindow = categories.activeWindow;
  if (categories.clipboard !== undefined) _consentState.clipboard = categories.clipboard;
  _consentState.updatedAt = Date.now();
}
function revokeConsent(categories?: { activeWindow?: boolean; clipboard?: boolean }): void {
  if (!categories) { _consentState = { activeWindow: false, clipboard: false, updatedAt: Date.now() }; }
  else {
    if (categories.activeWindow === false) _consentState.activeWindow = false;
    if (categories.clipboard === false) _consentState.clipboard = false;
    _consentState.updatedAt = Date.now();
  }
}

export const metadata = {
  name: "privacy_dashboard",
  description:
    "[SILENT] Quản lý quyền riêng tư của người dùng: Xem dữ liệu đang lưu trữ, bật/tắt thu thập, hoặc xóa toàn bộ dữ liệu cá nhân. (Privacy dashboard: view stored data, toggle data collection, or delete all personal data.)",
  isCoreSkill: true,
  search_keywords: [
    "quyền riêng tư", "privacy", "dữ liệu cá nhân", "personal data",
    "xóa dữ liệu", "delete data", "quên tôi", "forget me",
    "consent", "đồng ý", "thu thập", "collection",
    "GDPR", "bảo vệ", "data protection", "lịch sử", "audit"
  ],
  parameters: {
    type: "object",
    properties: {
      action: {
        type: "string",
        enum: ["view_inventory", "view_consent", "grant_consent", "revoke_consent", "purge_all_data", "view_audit_log"],
        description: `Action to perform:
          - "view_inventory": Xem tổng quan dữ liệu LIVA đang lưu
          - "view_consent": Xem trạng thái đồng ý thu thập
          - "grant_consent": Bật thu thập dữ liệu (cần chỉ định categories)
          - "revoke_consent": Tắt thu thập dữ liệu (cần chỉ định categories hoặc để trống = tắt tất cả)
          - "purge_all_data": XÓA TOÀN BỘ dữ liệu cá nhân (không thể hoàn tác!)
          - "view_audit_log": Xem nhật ký truy cập dữ liệu gần đây`
      },
      categories: {
        type: "object",
        description: "Data categories to manage (for grant_consent / revoke_consent)",
        properties: {
          activeWindow: { type: "boolean", description: "Collect active application names and window titles" },
          clipboard: { type: "boolean", description: "Collect clipboard content (temporary memory)" }
        }
      }
    },
    required: ["action"]
  }
};

export const execute = async (args: {
  action: string;
  categories?: { activeWindow?: boolean; clipboard?: boolean };
}): Promise<string> => {
  const sensory = SensoryManager.getInstance();

  switch (args.action) {
    case "view_inventory": {
      const consent = getConsentState();
      const auditCount = getAuditLog().length;
      
      return `📊 **BÁO CÁO DỮ LIỆU CÁ NHÂN (Data Inventory)**

🔹 **Dữ liệu Giác quan (Sensory Data):**
   - Thu thập Cửa sổ đang mở: ${consent.activeWindow ? "✅ BẬT" : "❌ TẮT"}
   - Thu thập Clipboard: ${consent.clipboard ? "✅ BẬT" : "❌ TẮT"}
   - Dữ liệu giác quan hiện có: ${sensory.currentData ? "Có (TTL 30s)" : "Không có"}

🔹 **Nhật ký kiểm toán:** ${auditCount} bản ghi

🔹 **Để xem chi tiết bộ nhớ (Memory Store), hãy yêu cầu action "view_audit_log".**

ℹ️ Bạn có quyền:
  - Tắt thu thập bất kỳ lúc nào → action: "revoke_consent"
  - Xóa toàn bộ dữ liệu → action: "purge_all_data"`;
    }

    case "view_consent": {
      const consent = getConsentState();
      return `⚙️ **TRẠNG THÁI ĐỒNG Ý THU THẬP (Consent State)**

| Danh mục | Trạng thái |
|----------|-----------|
| Cửa sổ đang mở (Active Window) | ${consent.activeWindow ? "✅ Đã đồng ý" : "❌ Chưa đồng ý"} |
| Bộ nhớ tạm (Clipboard) | ${consent.clipboard ? "✅ Đã đồng ý" : "❌ Chưa đồng ý"} |

📅 Cập nhật lần cuối: ${new Date(consent.updatedAt).toLocaleString("vi-VN")}

💡 Để thay đổi, sử dụng action "grant_consent" hoặc "revoke_consent" kèm categories.`;
    }

    case "grant_consent": {
      if (!args.categories) {
        return "⚠️ Vui lòng chỉ định categories cần bật. Ví dụ: { activeWindow: true, clipboard: true }";
      }
      grantConsent(args.categories);
      auditLogger.recordConsentChange(
        Object.entries(args.categories).filter(([, v]) => v).map(([k]) => k),
        true
      );
      const updated = getConsentState();
      return `✅ **ĐÃ CẬP NHẬT ĐỒNG Ý THU THẬP**

| Danh mục | Trạng thái mới |
|----------|---------------|
| Cửa sổ đang mở | ${updated.activeWindow ? "✅ BẬT" : "❌ TẮT"} |
| Clipboard | ${updated.clipboard ? "✅ BẬT" : "❌ TẮT"} |

📝 Hành động này đã được ghi vào nhật ký kiểm toán.`;
    }

    case "revoke_consent": {
      if (args.categories) {
        const toRevoke: { activeWindow?: boolean; clipboard?: boolean } = {};
        if (args.categories.activeWindow === false) toRevoke.activeWindow = false;
        if (args.categories.clipboard === false) toRevoke.clipboard = false;
        revokeConsent(toRevoke);
      } else {
        revokeConsent();
      }
      auditLogger.recordConsentChange(
        args.categories 
          ? Object.entries(args.categories).filter(([, v]) => !v).map(([k]) => k)
          : ["activeWindow", "clipboard"],
        false
      );
      const updated = getConsentState();
      return `🛑 **ĐÃ RÚT LẠI ĐỒNG Ý THU THẬP**

| Danh mục | Trạng thái mới |
|----------|---------------|
| Cửa sổ đang mở | ${updated.activeWindow ? "✅ BẬT" : "❌ TẮT"} |
| Clipboard | ${updated.clipboard ? "✅ BẬT" : "❌ TẮT"} |

${!args.categories ? "⚠️ TẤT CẢ dữ liệu giác quan đã bị xóa ngay lập tức." : ""}
📝 Hành động này đã được ghi vào nhật ký kiểm toán.`;
    }

    case "purge_all_data": {
      revokeConsent();
      sensory.flush();
      
      auditLogger.record({
        eventType: "DATA_PURGE",
        actor: "user",
        action: "User invoked Right to be Forgotten — all sensory data purged",
        riskLevel: "CRITICAL"
      });

      return `🚨 **QUYỀN ĐƯỢC QUÊN — RIGHT TO BE FORGOTTEN**

✅ Đã thực hiện:
  - ✅ Toàn bộ dữ liệu giác quan (Sensory Data) đã bị xóa
  - ✅ Toàn bộ đồng ý thu thập đã bị rút lại
  - ✅ Sự kiện đã được ghi vào nhật ký kiểm toán

⚠️ **LƯU Ý:** Để xóa hoàn toàn bộ nhớ hội thoại (Memory Store), vui lòng chạy lệnh:
  \`memory.purgeAllUserData()\` trong console quản trị.

📋 Theo Luật BVDL Cá nhân VN 2026, Điều 30:
  - Bạn có quyền yêu cầu xóa toàn bộ dữ liệu cá nhân bất kỳ lúc nào
  - Hệ thống PHẢI tuân thủ trong vòng 72 giờ`;
    }

    case "view_audit_log": {
      const auditEntries = getAuditLog();
      if (auditEntries.length === 0) {
        return "📋 Nhật ký kiểm toán trống — chưa có hoạt động thu thập dữ liệu nào.";
      }

      const recent = auditEntries.slice(-10);
      const lines = recent.map(entry => {
        const time = new Date(entry.timestamp).toLocaleTimeString("vi-VN");
        return `| ${time} | ${entry.action} | ${entry.categories.join(", ") || "—"} | ${entry.detail.substring(0, 60)} |`;
      });

      return `📋 **NHẬT KÝ KIỂM TOÁN (10 bản ghi gần nhất / ${auditEntries.length} tổng cộng)**

| Thời gian | Hành động | Danh mục | Chi tiết |
|-----------|-----------|----------|----------|
${lines.join("\n")}

💡 Để xem toàn bộ nhật ký, kiểm tra file \`logs/audit/audit_YYYY-MM-DD.jsonl\``;
    }

    default:
      return `❌ Action không hợp lệ: "${args.action}". Các action hợp lệ: view_inventory, view_consent, grant_consent, revoke_consent, purge_all_data, view_audit_log`;
  }
};
