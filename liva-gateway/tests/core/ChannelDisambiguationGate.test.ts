import { describe, it, expect, vi, beforeEach } from "vitest";
import {
    isAmbiguousChannel,
    resolveChannelFromReply,
    resolveChannelSignal,
    buildClarificationMessage,
    buildPreferenceKey,
    buildPreferenceValue,
    MESSAGING_TOOLS,
    CHANNEL_PREF_PREFIX,
    PREFERENCE_BYPASS_THRESHOLD,
} from "../../src/core/ChannelDisambiguationGate";

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        debug: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
    },
}));

// ===========================
// isAmbiguousChannel
// ===========================
describe("ChannelDisambiguationGate — isAmbiguousChannel", () => {
    it("should return true when no channel keyword is present (ambiguous)", () => {
        expect(isAmbiguousChannel("nhắn tin cho Khánh hỏi mai đi chơi không", "send_zalo_rpa")).toBe(true);
        expect(isAmbiguousChannel("gửi cho Khánh tin nhắn", "send_messenger_rpa")).toBe(true);
        expect(isAmbiguousChannel("nhắn tin cho mẹ bảo con về muộn", "send_zalo_rpa")).toBe(true);
        expect(isAmbiguousChannel("message Khanh asking about tomorrow", "send_messenger_rpa")).toBe(true);
    });

    it("should return false when Zalo keyword is present", () => {
        expect(isAmbiguousChannel("nhắn zalo cho Khánh hỏi mai đi chơi", "send_zalo_rpa")).toBe(false);
        expect(isAmbiguousChannel("gửi zalo cho mẹ", "send_zalo_rpa")).toBe(false);
    });

    it("should return false when Messenger keyword is present", () => {
        expect(isAmbiguousChannel("nhắn mess cho Khánh", "send_messenger_rpa")).toBe(false);
        expect(isAmbiguousChannel("message Khanh on messenger", "send_messenger_rpa")).toBe(false);
        expect(isAmbiguousChannel("gửi tin cho bạn trên facebook", "send_messenger_rpa")).toBe(false);
        expect(isAmbiguousChannel("nhắn fb cho Hùng", "send_messenger_rpa")).toBe(false);
    });

    it("should return false when Email keyword is present", () => {
        expect(isAmbiguousChannel("gửi mail cho sếp báo xin nghỉ", "send_email")).toBe(false);
        expect(isAmbiguousChannel("gửi email cho khách hàng", "send_email")).toBe(false);
        expect(isAmbiguousChannel("gửi thư điện tử cho Hùng", "send_email")).toBe(false);
    });

    it("should return false for non-messaging tools", () => {
        expect(isAmbiguousChannel("thời tiết hôm nay", "get_weather_forecast")).toBe(false);
        expect(isAmbiguousChannel("đọc email", "read_emails")).toBe(false);
        expect(isAmbiguousChannel("tìm file", "search_google_drive")).toBe(false);
    });

    it("should avoid false positives from message content (Regex word boundary)", () => {
        // "zalo.apk" — "zalo" IS a word boundary match here because \b matches before "."
        // But "gửi file zalo.apk" is an edge case — the intent is sending a file, not messaging via Zalo
        // The gate checks for channel signals, and "zalo" as a word IS a valid signal.
        // This is acceptable behavior — the tool description's ASK_FIRST will catch it.
        expect(isAmbiguousChannel("gửi cho Khánh file zalopay_report.xlsx", "send_messenger_rpa")).toBe(true);
    });

    it("should bypass gate when StructuredMemory preference exceeds threshold", () => {
        // count=3 >= PREFERENCE_BYPASS_THRESHOLD(3) → bypass
        expect(isAmbiguousChannel("nhắn tin cho Khánh", "send_zalo_rpa", "Khánh", "send_zalo_rpa:3")).toBe(false);
        expect(isAmbiguousChannel("nhắn tin cho Khánh", "send_zalo_rpa", "Khánh", "send_zalo_rpa:5")).toBe(false);
    });

    it("should NOT bypass gate when preference count is below threshold", () => {
        expect(isAmbiguousChannel("nhắn tin cho Khánh", "send_zalo_rpa", "Khánh", "send_zalo_rpa:2")).toBe(true);
        expect(isAmbiguousChannel("nhắn tin cho Khánh", "send_zalo_rpa", "Khánh", "send_zalo_rpa:1")).toBe(true);
    });

    it("should NOT bypass gate when preference is null/undefined", () => {
        expect(isAmbiguousChannel("nhắn tin cho Khánh", "send_zalo_rpa", "Khánh", null)).toBe(true);
        expect(isAmbiguousChannel("nhắn tin cho Khánh", "send_zalo_rpa", "Khánh", undefined)).toBe(true);
    });
});

// ===========================
// resolveChannelSignal
// ===========================
describe("ChannelDisambiguationGate — resolveChannelSignal", () => {
    it("should resolve Zalo signals", () => {
        expect(resolveChannelSignal("nhắn zalo cho Khánh")).toBe("send_zalo_rpa");
        expect(resolveChannelSignal("gửi zalo cho mẹ")).toBe("send_zalo_rpa");
    });

    it("should resolve Messenger signals", () => {
        expect(resolveChannelSignal("nhắn mess cho bạn")).toBe("send_messenger_rpa");
        expect(resolveChannelSignal("message on messenger")).toBe("send_messenger_rpa");
        expect(resolveChannelSignal("nhắn fb cho Hùng")).toBe("send_messenger_rpa");
    });

    it("should resolve Email signals", () => {
        expect(resolveChannelSignal("gửi mail cho sếp")).toBe("send_email");
        expect(resolveChannelSignal("send email to boss")).toBe("send_email");
    });

    it("should return null when no signal found", () => {
        expect(resolveChannelSignal("nhắn tin cho Khánh")).toBeNull();
        expect(resolveChannelSignal("gửi cho mẹ")).toBeNull();
    });
});

// ===========================
// resolveChannelFromReply
// ===========================
describe("ChannelDisambiguationGate — resolveChannelFromReply", () => {
    it("should resolve 'Zalo' reply", () => {
        expect(resolveChannelFromReply("Zalo")).toBe("send_zalo_rpa");
        expect(resolveChannelFromReply("gửi qua zalo")).toBe("send_zalo_rpa");
        expect(resolveChannelFromReply("zalo nhé")).toBe("send_zalo_rpa");
    });

    it("should resolve 'Messenger' reply", () => {
        expect(resolveChannelFromReply("Messenger")).toBe("send_messenger_rpa");
        expect(resolveChannelFromReply("mess")).toBe("send_messenger_rpa");
        expect(resolveChannelFromReply("gửi qua fb")).toBe("send_messenger_rpa");
    });

    it("should resolve 'Email' reply", () => {
        expect(resolveChannelFromReply("Email")).toBe("send_email");
        expect(resolveChannelFromReply("gửi mail")).toBe("send_email");
    });

    it("should return null for unrecognized replies", () => {
        expect(resolveChannelFromReply("")).toBeNull();
        expect(resolveChannelFromReply("   ")).toBeNull();
        expect(resolveChannelFromReply("cái gì vậy")).toBeNull();
    });
});

// ===========================
// buildClarificationMessage
// ===========================
describe("ChannelDisambiguationGate — buildClarificationMessage", () => {
    it("should build Vietnamese clarification message", () => {
        const msg = buildClarificationMessage("Khánh", "vi-VN");
        expect(msg).toContain("Khánh");
        expect(msg).toContain("💬 Zalo");
        expect(msg).toContain("📘 Messenger");
        expect(msg).toContain("📧 Email");
        expect(msg).toContain("kênh nào");
    });

    it("should build English clarification message", () => {
        const msg = buildClarificationMessage("Khanh", "en-US");
        expect(msg).toContain("Khanh");
        expect(msg).toContain("Which channel");
    });

    it("should default to Vietnamese", () => {
        const msg = buildClarificationMessage("Khánh");
        expect(msg).toContain("kênh nào");
    });
});

// ===========================
// Preference Key/Value Helpers
// ===========================
describe("ChannelDisambiguationGate — Preference Helpers", () => {
    it("should build preference key correctly", () => {
        expect(buildPreferenceKey("Khánh")).toBe("channel_pref::khánh");
        expect(buildPreferenceKey("  Mẹ  ")).toBe("channel_pref::mẹ");
    });

    it("should build new preference value", () => {
        expect(buildPreferenceValue("send_zalo_rpa")).toBe("send_zalo_rpa:1");
        expect(buildPreferenceValue("send_messenger_rpa", null)).toBe("send_messenger_rpa:1");
    });

    it("should increment preference value for same tool", () => {
        expect(buildPreferenceValue("send_zalo_rpa", "send_zalo_rpa:2")).toBe("send_zalo_rpa:3");
        expect(buildPreferenceValue("send_zalo_rpa", "send_zalo_rpa:9")).toBe("send_zalo_rpa:10");
    });

    it("should reset preference count when tool changes", () => {
        expect(buildPreferenceValue("send_messenger_rpa", "send_zalo_rpa:5")).toBe("send_messenger_rpa:1");
    });

    it("should export correct constants", () => {
        expect(MESSAGING_TOOLS.has("send_zalo_rpa")).toBe(true);
        expect(MESSAGING_TOOLS.has("send_messenger_rpa")).toBe(true);
        expect(MESSAGING_TOOLS.has("send_email")).toBe(true);
        expect(MESSAGING_TOOLS.has("get_weather_forecast")).toBe(false);
        expect(CHANNEL_PREF_PREFIX).toBe("channel_pref::");
        expect(PREFERENCE_BYPASS_THRESHOLD).toBe(3);
    });
});

// ===========================
// [v27 Regression] Greeting/Chitchat Non-Interference
// Ensures ChannelDisambiguationGate NEVER blocks or interferes with
// normal conversations, greetings, and non-messaging tool calls.
// ===========================
describe("ChannelDisambiguationGate — Greeting/Chitchat Regression", () => {
    it("should NOT trigger gate for 'Hello' (not a messaging tool)", () => {
        // Greetings don't involve messaging tools, so isAmbiguousChannel
        // should return false for any non-messaging tool name
        expect(isAmbiguousChannel("Hello", "chitchat")).toBe(false);
        expect(isAmbiguousChannel("Hello", "web_search")).toBe(false);
        expect(isAmbiguousChannel("Hello", "get_weather_forecast")).toBe(false);
    });

    it("should NOT resolve 'Hello' as a channel reply", () => {
        // 'Hello' should NOT be confused with Zalo/Messenger/Email
        expect(resolveChannelFromReply("Hello")).toBeNull();
        expect(resolveChannelFromReply("hello")).toBeNull();
    });

    it("should NOT resolve common Vietnamese greetings as channel replies", () => {
        expect(resolveChannelFromReply("Xin chào")).toBeNull();
        expect(resolveChannelFromReply("Chào LIVA")).toBeNull();
        expect(resolveChannelFromReply("Hi")).toBeNull();
        expect(resolveChannelFromReply("Hey")).toBeNull();
        expect(resolveChannelFromReply("Ê")).toBeNull();
    });

    it("should NOT resolve general conversation as channel replies", () => {
        expect(resolveChannelFromReply("thời tiết hôm nay")).toBeNull();
        expect(resolveChannelFromReply("mấy giờ rồi")).toBeNull();
        expect(resolveChannelFromReply("cảm ơn")).toBeNull();
        expect(resolveChannelFromReply("ok")).toBeNull();
        expect(resolveChannelFromReply("được rồi")).toBeNull();
        expect(resolveChannelFromReply("hủy đi")).toBeNull();
        expect(resolveChannelFromReply("không cần nữa")).toBeNull();
    });

    it("should NOT detect channel signals in greeting text", () => {
        expect(resolveChannelSignal("Hello")).toBeNull();
        expect(resolveChannelSignal("Xin chào LIVA")).toBeNull();
        expect(resolveChannelSignal("Hôm nay thời tiết thế nào")).toBeNull();
        expect(resolveChannelSignal("Giúp em tìm tài liệu")).toBeNull();
    });

    it("should NOT trigger gate for common non-messaging tools", () => {
        const commonTools = [
            "web_search", "web_browser", "get_weather_forecast",
            "search_google_drive", "read_emails", "execute_command",
            "handoff_to_expert", "update_memory", "chitchat",
        ];
        for (const tool of commonTools) {
            expect(isAmbiguousChannel("Hello", tool)).toBe(false);
            expect(isAmbiguousChannel("nhắn tin cho Khánh", tool)).toBe(false);
        }
    });

    it("should handle edge case: empty or whitespace-only user text", () => {
        expect(isAmbiguousChannel("", "send_zalo_rpa")).toBe(true); // ambiguous
        expect(isAmbiguousChannel("   ", "send_zalo_rpa")).toBe(true); // ambiguous
        expect(resolveChannelFromReply("")).toBeNull();
        expect(resolveChannelFromReply("   ")).toBeNull();
    });
});
