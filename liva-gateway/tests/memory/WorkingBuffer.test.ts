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
        expect(status).toContain("64000 tokens remaining");
    });

    it("should warn via logger and write draft when budget exceeds 60%", async () => {
        const largeString = "a".repeat(256000 * 0.65); // 65% capacity
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
        const massiveString = "a".repeat(256000 * 0.80); // 80% capacity
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
});
