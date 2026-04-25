/**
 * LTCOrchestrator.test.ts — Long-Term Context summarization tests
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

import { LTCOrchestrator } from "../../src/core/LTCOrchestrator";

describe("LTCOrchestrator", () => {
    let ltc: LTCOrchestrator;
    let mockMemory: any;
    let mockAI: any;

    beforeEach(() => {
        vi.clearAllMocks();
        mockMemory = {
            updateLongTermMemory: vi.fn().mockResolvedValue(undefined),
        };
        mockAI = {
            chat: {
                completions: {
                    create: vi.fn().mockResolvedValue({
                        choices: [{ message: { content: "User wants to build LIVA system" } }],
                    }),
                },
            },
        };
        ltc = new LTCOrchestrator(mockMemory, mockAI);
    });

    it("should extract and store a meaningful fact", async () => {
        await ltc.summarizeAndStore("Tôi đang làm dự án LIVA", "Dạ, em hiểu rồi anh!");
        expect(mockMemory.updateLongTermMemory).toHaveBeenCalledWith(
            "Working Concepts",
            expect.arrayContaining([expect.any(String)])
        );
    });

    it("should skip storage when AI returns NONE", async () => {
        mockAI.chat.completions.create.mockResolvedValue({
            choices: [{ message: { content: "NONE" } }],
        });
        await ltc.summarizeAndStore("Chào!", "Xin chào!");
        expect(mockMemory.updateLongTermMemory).not.toHaveBeenCalled();
    });

    it("should skip storage for very short responses", async () => {
        mockAI.chat.completions.create.mockResolvedValue({
            choices: [{ message: { content: "ok" } }],
        });
        await ltc.summarizeAndStore("ok", "ok");
        expect(mockMemory.updateLongTermMemory).not.toHaveBeenCalled();
    });

    it("should skip storage for empty responses", async () => {
        mockAI.chat.completions.create.mockResolvedValue({
            choices: [{ message: { content: "" } }],
        });
        await ltc.summarizeAndStore("test", "test");
        expect(mockMemory.updateLongTermMemory).not.toHaveBeenCalled();
    });

    it("should handle AI errors gracefully", async () => {
        mockAI.chat.completions.create.mockRejectedValue(new Error("Model unavailable"));
        await expect(ltc.summarizeAndStore("test", "test")).resolves.not.toThrow();
    });
});
