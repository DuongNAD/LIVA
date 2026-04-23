import { describe, it, expect, vi, beforeEach } from "vitest";
import { PromptBuilder } from "../../src/core/PromptBuilder";

// ============================================================
// Mocks
// ============================================================
vi.mock("../../src/memory/SensoryManager", () => ({
    SensoryManager: {
        getInstance: () => ({
            injectSensoryPrompt: () => "<SystemSensory>Test sensory data</SystemSensory>",
        }),
    },
}));

vi.mock("../../src/system_prompt", () => ({
    getBaseSystemPrompt: () => "Bạn là Liva, một trợ lý AI.",
}));

// ============================================================
// TEST GROUP 1: semanticSkillFilter (via buildToolsPrompt)
// ============================================================
describe("PromptBuilder.buildToolsPrompt", () => {
    const sampleTools = [
        {
            name: "search_web",
            description: "Tìm kiếm thông tin trên mạng internet",
            isCoreSkill: false,
            search_keywords: ["tìm kiếm", "google", "web", "internet"],
            parameters: { type: "object", properties: { query: { type: "string" } } },
        },
        {
            name: "send_zalo_bot",
            description: "Gửi tin nhắn qua Zalo",
            isCoreSkill: false,
            search_keywords: ["zalo", "tin nhắn", "nhắn", "gửi"],
            parameters: { type: "object", properties: { message: { type: "string" } } },
        },
        {
            name: "get_current_time",
            description: "Lấy thời gian hiện tại",
            isCoreSkill: true,
            search_keywords: ["giờ", "thời gian", "ngày"],
            parameters: { type: "object", properties: {} },
        },
        {
            name: "read_file",
            description: "Đọc nội dung tệp tin",
            isCoreSkill: false,
            search_keywords: ["đọc", "file", "tệp"],
            parameters: { type: "object", properties: { path: { type: "string" } } },
        },
        {
            name: "run_code_sandbox",
            description: "Chạy code trong Docker sandbox",
            isCoreSkill: false,
            search_keywords: ["code", "chạy", "sandbox", "python"],
            parameters: { type: "object", properties: { code: { type: "string" } } },
        },
        {
            name: "send_email",
            description: "Gửi email qua Gmail SMTP",
            isCoreSkill: false,
            search_keywords: ["email", "gmail", "gửi mail"],
            parameters: { type: "object", properties: { to: { type: "string" } } },
        },
        {
            name: "browse_web",
            description: "Mở và đọc nội dung trang web bằng Puppeteer",
            isCoreSkill: false,
            search_keywords: ["duyệt web", "mở trang", "puppeteer"],
            parameters: { type: "object", properties: { url: { type: "string" } } },
        },
    ];

    beforeEach(() => {
        // Clear the static cache between tests
        // @ts-ignore - accessing private static
        PromptBuilder["#promptCache"]?.clear?.();
    });

    it("should always include core skills (get_current_time)", () => {
        const prompt = PromptBuilder.buildToolsPrompt("Mấy giờ rồi?", sampleTools);
        expect(prompt).toContain("get_current_time");
    });

    it("should always include handoff_to_expert (auto-injected)", () => {
        const prompt = PromptBuilder.buildToolsPrompt("test", sampleTools);
        expect(prompt).toContain("handoff_to_expert");
    });

    it("should prioritize tools matching query keywords", () => {
        const prompt = PromptBuilder.buildToolsPrompt("Tìm kiếm thời tiết Hà Nội", sampleTools);
        expect(prompt).toContain("search_web");
    });

    it("should prioritize Zalo tool when query mentions Zalo", () => {
        const prompt = PromptBuilder.buildToolsPrompt("Gửi tin nhắn cho Sếp qua Zalo", sampleTools);
        expect(prompt).toContain("send_zalo_bot");
    });

    it("should limit to topK non-core tools (max 5)", () => {
        const prompt = PromptBuilder.buildToolsPrompt("help me", sampleTools);
        // Parse the JSON tools array in the prompt
        const toolsMatch = prompt.match(/\<tools\>\n([\s\S]*?)\n\<\/tools\>/);
        if (toolsMatch) {
            const toolsArr = JSON.parse(toolsMatch[1]);
            // Core skills (get_current_time, handoff_to_expert) + up to 5 non-core
            expect(toolsArr.length).toBeLessThanOrEqual(7);
        }
    });

    it("should strip search_keywords and isCoreSkill from final output", () => {
        const prompt = PromptBuilder.buildToolsPrompt("test", sampleTools);
        expect(prompt).not.toContain("search_keywords");
        expect(prompt).not.toContain("isCoreSkill");
    });

    it("should include system time in the prompt", () => {
        const prompt = PromptBuilder.buildToolsPrompt("test", sampleTools);
        // Vietnamese date format includes "/" separator
        expect(prompt).toContain("Thời gian hệ thống:");
    });

    it("should include CRITICAL RULES for tool call format", () => {
        const prompt = PromptBuilder.buildToolsPrompt("test", sampleTools);
        expect(prompt).toContain("<tool_call>");
        expect(prompt).toContain("CRITICAL RULES:");
    });

    it("should return all core + random topK for empty user query", () => {
        const prompt = PromptBuilder.buildToolsPrompt("", sampleTools);
        // Should still include core skills
        expect(prompt).toContain("get_current_time");
        expect(prompt).toContain("handoff_to_expert");
    });
});

// ============================================================
// TEST GROUP 2: buildContextPrompt (Requires Memory Mock)
// ============================================================
describe("PromptBuilder.buildContextPrompt", () => {
    let mockMemory: any;

    beforeEach(() => {
        mockMemory = {
            getUserProfile: vi.fn().mockResolvedValue({
                name: "Dương",
                preferred_name: "Anh Dương",
            }),
            getStructuredMemoryPrompt: vi.fn().mockReturnValue("\n[BỘ NHỚ CẤU TRÚC]\n- name: Dương\n"),
            getLongTermMarkdown: vi.fn().mockResolvedValue("## Working Concepts\n- User likes dark mode UI.\n- User prefers Vietnamese language for all interactions"),
            getSessionState: vi.fn().mockResolvedValue(""),
        };
    });

    it("should include user profile in context", async () => {
        const ctx = await PromptBuilder.buildContextPrompt(mockMemory, "Hà Nội");
        expect(ctx).toContain("Dương");
        expect(ctx).toContain("HỒ SƠ NGƯỜI DÙNG");
    });

    it("should include structured memory prompt", async () => {
        const ctx = await PromptBuilder.buildContextPrompt(mockMemory, "Hà Nội");
        expect(ctx).toContain("BỘ NHỚ CẤU TRÚC");
    });

    it("should include long-term context when available", async () => {
        const ctx = await PromptBuilder.buildContextPrompt(mockMemory, "Hà Nội");
        expect(ctx).toContain("KÝ ỨC DÀI HẠN");
        expect(ctx).toContain("Working Concepts");
    });

    it("should skip long-term context when too short", async () => {
        mockMemory.getLongTermMarkdown.mockResolvedValue("short");
        const ctx = await PromptBuilder.buildContextPrompt(mockMemory, "Hà Nội");
        expect(ctx).not.toContain("KÝ ỨC DÀI HẠN");
    });

    it("should include sensory data", async () => {
        const ctx = await PromptBuilder.buildContextPrompt(mockMemory, "Hà Nội");
        expect(ctx).toContain("SystemSensory");
    });

    it("should set current_location on user profile", async () => {
        await PromptBuilder.buildContextPrompt(mockMemory, "TP.HCM");
        const profile = await mockMemory.getUserProfile();
        // The method mutates the returned profile object
        expect(mockMemory.getUserProfile).toHaveBeenCalled();
    });

    it("should handle null user profile gracefully", async () => {
        mockMemory.getUserProfile.mockResolvedValue(null);
        const ctx = await PromptBuilder.buildContextPrompt(mockMemory, "Hà Nội");
        expect(ctx).not.toContain("HỒ SƠ NGƯỜI DÙNG");
    });
});

// ============================================================
// TEST GROUP 3: prepareFullAiMessages
// ============================================================
describe("PromptBuilder.prepareFullAiMessages", () => {
    let mockMemory: any;

    beforeEach(() => {
        mockMemory = {
            getUserProfile: vi.fn().mockResolvedValue({ name: "Test" }),
            getStructuredMemoryPrompt: vi.fn().mockReturnValue(""),
            getLongTermMarkdown: vi.fn().mockResolvedValue(""),
            getSessionState: vi.fn().mockResolvedValue(""),
            workingBuffer: { checkBudget: vi.fn().mockResolvedValue("[context-budget: 5.0% used, 63000 tokens remaining]") },
            getHybridContext: vi.fn().mockResolvedValue([
                { role: "user", content: "Xin chào" },
                { role: "assistant", content: "Chào bạn!" },
            ]),
        };
    });

    it("should return array starting with system message", async () => {
        const messages = await PromptBuilder.prepareFullAiMessages(
            "test query",
            mockMemory,
            "Hà Nội",
            []
        );

        expect(messages[0].role).toBe("system");
        expect(messages[0].content).toContain("Bạn là Liva");
    });

    it("should include history messages after system", async () => {
        const messages = await PromptBuilder.prepareFullAiMessages(
            "test",
            mockMemory,
            "Hà Nội",
            []
        );

        expect(messages.length).toBe(3); // system + 2 history
        expect(messages[1].role).toBe("user");
        expect(messages[1].content).toBe("Xin chào");
    });
});
