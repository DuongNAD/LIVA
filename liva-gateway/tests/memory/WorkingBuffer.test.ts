import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { WorkingBuffer } from "../../src/memory/WorkingBuffer";
import * as fs from "fs/promises";
import { logger } from "../../src/utils/logger";

// Mocks
vi.mock("fs/promises", () => ({
    mkdir: vi.fn().mockResolvedValue(undefined),
    writeFile: vi.fn().mockResolvedValue(undefined)
}));

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn()
    }
}));

describe("WorkingBuffer", () => {
    let workingBuffer: WorkingBuffer;
    const testAgentId = "test-agent";
    // Default: 8192 tokens → maxChars = floor(8192 * 0.7) * 4 = 22932
    const DEFAULT_MAX_CHARS = Math.floor(8192 * 0.7) * 4;

    beforeEach(() => {
        vi.clearAllMocks();
        workingBuffer = new WorkingBuffer(testAgentId);
    });

    afterEach(() => {
        vi.restoreAllMocks();
    });

    it("should initialize and ensure directory exists", () => {
        expect(fs.mkdir).toHaveBeenCalledWith(
            expect.stringContaining("test-agent"),
            { recursive: true }
        );
    });

    it("should return correct budget status for empty context", async () => {
        const status = await workingBuffer.checkBudget("");
        expect(status).toContain("0.0% used");
        expect(status).toContain(`${Math.floor(DEFAULT_MAX_CHARS / 4)} tokens remaining`);
    });

    it("should warn via logger and write draft when budget exceeds 60%", async () => {
        const largeString = "a".repeat(Math.floor(DEFAULT_MAX_CHARS * 0.65));
        const status = await workingBuffer.checkBudget(largeString);
        
        expect(status).toContain("65.0% used");
        expect(logger.info).toHaveBeenCalledWith(
            expect.stringContaining("[WorkingBuffer] Cảnh báo dung lượng ngữ cảnh")
        );
        expect(fs.writeFile).toHaveBeenCalledWith(
            expect.stringContaining("working-buffer.md"),
            expect.stringContaining("DANGER ZONE DRAFT"),
            "utf-8"
        );
    });

    it("should trigger snapshot and flush when budget exceeds 78%", async () => {
        const massiveString = "a".repeat(Math.floor(DEFAULT_MAX_CHARS * 0.80));
        const status = await workingBuffer.checkBudget(massiveString);
        
        expect(status).toContain("80.0% used");
        expect(logger.warn).toHaveBeenCalledWith(
            expect.stringContaining("[WorkingBuffer] Ngân sách Token nguy cấp")
        );
        expect(fs.writeFile).toHaveBeenCalledWith(
            expect.stringContaining("working-snapshot.md"),
            expect.stringContaining("COMPACTION SNAPSHOT"),
            "utf-8"
        );
    });

    it("should accept custom contextTokens in constructor", async () => {
        const customBuffer = new WorkingBuffer("custom-agent", 16384);
        const customMaxChars = Math.floor(16384 * 0.7) * 4;
        const status = await customBuffer.checkBudget("");
        expect(status).toContain(`${Math.floor(customMaxChars / 4)} tokens remaining`);
    });

    it("should update context limit dynamically via updateContextLimit()", async () => {
        workingBuffer.updateContextLimit(32768);
        const newMaxChars = Math.floor(32768 * 0.7) * 4;
        const status = await workingBuffer.checkBudget("");
        expect(status).toContain(`${Math.floor(newMaxChars / 4)} tokens remaining`);
    });
});

