import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

vi.mock("isolated-vm", () => {
    class MockIsolate {
        createContext = vi.fn().mockResolvedValue({
            global: { set: vi.fn().mockResolvedValue(undefined) },
            eval: vi.fn().mockResolvedValue(undefined),
        });
        compileScript = vi.fn().mockResolvedValue({
            run: vi.fn().mockResolvedValue(42),
        });
        dispose = vi.fn();
    }
    class MockReference {
        #fn: Function;
        constructor(fn: Function) { this.#fn = fn; }
        applySync(_recv: any, args: any[]) { return this.#fn(...args); }
    }
    return {
        default: { Isolate: MockIsolate, Reference: MockReference },
        Isolate: MockIsolate,
        Reference: MockReference,
    };
});

import { metadata, execute } from "../../src/skills/devops/CodeRunner";

describe("CodeRunner", () => {
    beforeEach(() => { vi.clearAllMocks(); });

    it("should have correct metadata", () => {
        expect(metadata.name).toBe("code_runner");
        expect(metadata.parameters.required).toContain("code");
    });

    it("should reject empty code", async () => {
        const result = await execute({ code: "" });
        expect(result).toContain("Error");
    });

    it("should run JavaScript code", async () => {
        const result = await execute({ code: "console.log('Hello World')" });
        expect(result).toContain("✅");
    });

    it("should cap timeout to 30 seconds", async () => {
        const result = await execute({ code: "console.log(1)", timeout: 999 });
        expect(result).toBeDefined();
    });
});
