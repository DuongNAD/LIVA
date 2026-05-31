import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

vi.mock("node:fs", async () => {
    const actual: any = await vi.importActual("node:fs");
    return {
        ...actual,
        promises: {
            ...actual.promises,
            access: vi.fn().mockResolvedValue(undefined),
            stat: vi.fn().mockResolvedValue({ size: 512000 }),
            mkdir: vi.fn().mockResolvedValue(undefined),
            rename: vi.fn().mockResolvedValue(undefined),
        },
    };
});

// ImageManipulator uses Worker threads with eval code, so we mock the Worker class
vi.mock("node:worker_threads", async () => {
    const actual: any = await vi.importActual("node:worker_threads");
    class MockWorker {
        #listeners: Record<string, Function[]> = {};
        constructor(_code: string, opts: any) {
            // Simulate worker behavior based on action
            setTimeout(() => {
                const action = opts?.workerData?.action;
                if (action === "info") {
                    this.#emit("message", {
                        success: true,
                        result: `[IMAGE INFO]\n📁 Path: test.jpg\n📐 Size: 1920x1080px\n🎨 Format: jpeg\n💾 File size: 500.0 KB\n🔍 Channels: 3\n📏 DPI: 72`
                    });
                } else if (action === "resize" && !opts?.workerData?.width && !opts?.workerData?.height) {
                    this.#emit("message", { success: false, error: "No dimensions specified" });
                } else {
                    this.#emit("message", { success: true, result: "✅ Done" });
                }
            }, 10);
        }
        on(event: string, handler: Function) { (this.#listeners[event] ??= []).push(handler); return this; }
        #emit(event: string, ...args: any[]) { (this.#listeners[event] ?? []).forEach(h => h(...args)); }
    }
    return { ...actual, Worker: MockWorker };
});

import { metadata, execute } from "../../src/skills/data/ImageManipulator";

describe("ImageManipulator", () => {
    beforeEach(() => { vi.clearAllMocks(); });

    it("should have correct metadata", () => {
        expect(metadata.name).toBe("image_manipulator");
        expect(metadata.parameters.required).toContain("action");
        expect(metadata.parameters.required).toContain("input_path");
    });

    it("should reject empty input_path", async () => {
        const result = await execute({ action: "info", input_path: "" });
        expect(result).toContain("Error");
    });

    it("should return image info", async () => {
        const result = await execute({ action: "info", input_path: "C:\\test\\photo.jpg" });
        expect(result).toContain("IMAGE INFO");
        expect(result).toContain("1920x1080");
    });

    it("should reject resize without dimensions", async () => {
        const result = await execute({ action: "resize", input_path: "C:\\test\\photo.jpg" });
        expect(result).toContain("error");
    });
});
