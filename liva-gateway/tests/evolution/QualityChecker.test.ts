import { describe, it, expect, vi, beforeEach } from "vitest";

const mockCreate = vi.fn();
vi.mock("openai", () => {
    return {
        default: class OpenAI {
            chat = { completions: { create: mockCreate } };
        }
    };
});

const mockExecFileSync = vi.fn();
vi.mock("node:child_process", () => ({
    execFileSync: (...args: any[]) => mockExecFileSync(...args)
}));

import { QualityChecker } from "../../src/evolution/QualityChecker";

describe("QualityChecker", () => {
    let checker: QualityChecker;

    beforeEach(() => {
        vi.clearAllMocks();
        checker = new QualityChecker("http://local", "key", "model");
    });

    it("should evaluate quality and return pass", async () => {
        mockExecFileSync.mockImplementation(() => { throw { stdout: "+ const a = 1;" } });
        mockCreate.mockResolvedValueOnce({
            choices: [{ message: { content: '{"pass": true, "feedback": "good"}' } }]
        });

        const res = await checker.evaluateCodeQuality("do thing", "/sandbox");
        expect(res.pass).toBe(true);
        expect(res.feedback).toBe("good");
    });

    it("should return false if extraction fails", async () => {
        mockExecFileSync.mockImplementation(() => { throw { stdout: "+ const a = 1;" } });
        mockCreate.mockResolvedValueOnce({
            choices: [{ message: { content: '{"pass": "maybe"}' } }]
        });

        const res = await checker.evaluateCodeQuality("do thing", "/sandbox");
        expect(res.pass).toBe(false);
        expect(res.feedback).toContain("Reviewer output failed validation");
    });

    it("should return false if git diff throws fatal error", async () => {
        mockExecFileSync.mockImplementation(() => { throw new Error("fatal") });
        const res = await checker.evaluateCodeQuality("do thing", "/sandbox");
        expect(res.pass).toBe(false);
    });

    it("should return false on OOM", async () => {
        mockExecFileSync.mockImplementation(() => { throw { stdout: "+ const a = 1;" } });
        mockCreate.mockRejectedValueOnce(new Error("maximum context length"));

        const res = await checker.evaluateCodeQuality("do thing", "/sandbox");
        expect(res.pass).toBe(false);
        expect(res.feedback).toContain("OOM Context");
    });
});
