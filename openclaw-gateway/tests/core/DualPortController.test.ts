/**
 * DualPortController.test.ts — VRAM circuit breaker tests
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

// Mock ModelOrchestrator
vi.mock("../../src/core/ModelOrchestrator", () => ({
    ModelOrchestrator: class MockModelOrchestrator {
        startRouter = vi.fn().mockResolvedValue(undefined);
        stopRouter = vi.fn().mockResolvedValue(undefined);
        startExpert = vi.fn().mockResolvedValue(undefined);
        stopExpert = vi.fn().mockResolvedValue(undefined);
        static getAuthorizedTokenFactory = vi.fn().mockReturnValue({
            issueToken: vi.fn().mockReturnValue({ phase: "TEST", isValid: () => true }),
        });
    },
}));

import { DualPortController } from "../../src/core/DualPortController";
import { ModelOrchestrator } from "../../src/core/ModelOrchestrator";

describe("DualPortController", () => {
    let controller: DualPortController;
    let mockOrchestrator: InstanceType<typeof ModelOrchestrator>;

    beforeEach(() => {
        vi.clearAllMocks();
        mockOrchestrator = new ModelOrchestrator() as any;
        controller = new DualPortController(mockOrchestrator);
    });

    describe("ensureExpertReady", () => {
        it("should start expert and stop router on first call", async () => {
            const result = await controller.ensureExpertReady();
            expect(result).toBe(true);
            expect(controller.isExpertAwake).toBe(true);
        });

        it("should return true immediately if expert already awake", async () => {
            // First call starts expert
            await controller.ensureExpertReady();
            // Second call should short-circuit
            const result = await controller.ensureExpertReady();
            expect(result).toBe(true);
        });

        it("should fallback to router on VRAM overload (expert start fails)", async () => {
            (mockOrchestrator.startExpert as any).mockRejectedValue(new Error("VRAM exhausted"));
            const result = await controller.ensureExpertReady();
            expect(result).toBe(false);
            expect(controller.isExpertAwake).toBe(false);
        });
    });

    describe("releaseResources", () => {
        it("should release VRAM when expert is awake", async () => {
            // First start expert
            await controller.ensureExpertReady();
            expect(controller.isExpertAwake).toBe(true);

            // Then release
            await controller.releaseResources();
            expect(controller.isExpertAwake).toBe(false);
        });

        it("should do nothing when expert is not awake", async () => {
            await controller.releaseResources();
            expect(mockOrchestrator.stopExpert).not.toHaveBeenCalled();
        });

        it("should not crash if stopExpert throws", async () => {
            await controller.ensureExpertReady();
            (mockOrchestrator.stopExpert as any).mockRejectedValue(new Error("GPU locked"));
            await expect(controller.releaseResources()).resolves.not.toThrow();
            expect(controller.isExpertAwake).toBe(false);
        });
    });
});
