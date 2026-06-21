import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// Mocks
vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn(),
    },
}));

vi.mock("child_process", () => ({
    spawn: vi.fn().mockReturnValue({
        pid: 9999,
        stdout: { on: vi.fn() },
        stderr: { on: vi.fn() },
        on: vi.fn(),
        kill: vi.fn(),
    }),
    execSync: vi.fn().mockReturnValue(Buffer.from("")),
}));

vi.mock("fs", async (importOriginal) => {
    const actual = await importOriginal<typeof import("fs")>();
    return {
        ...actual,
        existsSync: () => true,
    };
});

let mockSwapSuccess = true;
let mockSwapDelayMs = 50;

vi.mock("../../src/utils/NativeIPCClient", () => ({
    NativeIPCClient: class {
        healthCheck() {
            return Promise.resolve(true);
        }
        async swapModel(modelPath: string, nCtx: number = 0, nGpuLayers: number = -1, backend: string = "") {
            await new Promise(resolve => setTimeout(resolve, mockSwapDelayMs));
            if (!mockSwapSuccess) {
                return { success: false, errorMessage: "Simulated Swap Failure" };
            }
            return { success: true, loadedModel: "mock-model", swapDurationMs: mockSwapDelayMs };
        }
        destroy() {}
    },
}));

// Mock ConfigManager
vi.mock("../../src/core/config/ConfigManager", () => ({
    ConfigManager: {
        getInstance: () => ({
            isNativeMode: true,
            get aiProvider() { return "local"; },
            get contextWindowTokens() { return 8192; },
            get env() {
                return {
                    LIVA_USE_NATIVE: true,
                    AI_MODELS_DIR: "/tmp/models",
                    EXPERT_MODEL_NAME: "gemma-expert.gguf",
                };
            },
            get() {
                return this.env;
            },
        }),
    },
}));

import { ModelOrchestrator } from "@core/ModelOrchestrator";

describe("ModelOrchestrator — VRAM Yielding and Swap Concurrency Stress Test", () => {
    let orchestrator: ModelOrchestrator;

    beforeEach(() => {
        orchestrator = new ModelOrchestrator();
        mockSwapSuccess = true;
        mockSwapDelayMs = 50;
    });

    afterEach(async () => {
        await orchestrator.dispose();
    });

    it("should handle yielding mid-swap without deadlock or flag corruption", async () => {
        // Start a swap to expert (takes 50ms)
        const swapPromise = orchestrator.swapToExpert();
        
        expect(orchestrator.isSwapping).toBe(true);

        // Simulate 5 concurrent wait requests
        const waits = Promise.all([
            orchestrator.waitForSwap(),
            orchestrator.waitForSwap(),
            orchestrator.waitForSwap(),
            orchestrator.waitForSwap(),
            orchestrator.waitForSwap(),
        ]);

        // Yield VRAM immediately (mid-swap)
        mockSwapSuccess = false;
        await orchestrator.killLlamaServer();

        // The swap should resolve to false
        const swapResult = await swapPromise;
        expect(swapResult).toBe(false);
        expect(orchestrator.isSwapping).toBe(false);

        // All wait promises must resolve promptly (no deadlock)
        await expect(waits).resolves.toBeDefined();
    });

    it("should handle high load concurrent swaps and battery yielding without deadlock", async () => {
        const results: boolean[] = [];
        const promises: Promise<void>[] = [];

        // Initiate a series of concurrent actions under load
        for (let i = 0; i < 20; i++) {
            if (i % 2 === 0) {
                promises.push(
                    orchestrator.swapToExpert().then(res => {
                        results.push(res);
                    })
                );
            } else {
                promises.push(
                    orchestrator.swapToRouter().then(res => {
                        results.push(res);
                    })
                );
            }

            // Yield VRAM midway through the concurrent storm
            if (i === 10) {
                promises.push(orchestrator.killLlamaServer());
            }
        }

        // Await all actions
        await Promise.all(promises);

        // Verify that orchestrator state is clean and no deadlock occurred
        expect(orchestrator.isSwapping).toBe(false);
        const status = orchestrator.getStatus();
        expect(status.isSwapping).toBe(false);
        // The test succeeds if it finishes without timing out, indicating zero deadlocks.
    });
});
