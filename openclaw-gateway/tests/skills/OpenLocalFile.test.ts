/**
 * OpenLocalFile.test.ts — File opening skill unit tests
 * Mocks child_process.spawn to avoid actually opening files
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

// Mock spawn to simulate successful file open
const mockOn = vi.fn();
const mockUnref = vi.fn();

vi.mock("child_process", () => ({
    exec: vi.fn((cmd: string, cb?: Function) => cb && cb(null, "", "")),
    spawn: vi.fn(() => {
        // Simulate process events — schedule 'close' with exit code 0
        const handlers: Record<string, Function> = {};
        const child = {
            on: vi.fn((event: string, handler: Function) => {
                handlers[event] = handler;
                // Auto-trigger 'close' event with exit code 0 on next tick
                if (event === "close") {
                    setTimeout(() => handler(0), 0);
                }
            }),
            unref: mockUnref,
        };
        return child;
    }),
}));

import * as OpenLocalFile from "../../src/skills/OpenLocalFile";

describe("OpenLocalFile Skill", () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    it("should have correct metadata", () => {
        expect(OpenLocalFile.metadata.name).toBe("open_local_file");
        expect(OpenLocalFile.metadata.parameters.required).toContain("targetPath");
    });

    it("should return success message for valid file path", async () => {
        const result = await OpenLocalFile.execute({ targetPath: "D:/test/file.txt" });
        expect(result).toContain("Đã ra lệnh mở thành công");
    });
});
