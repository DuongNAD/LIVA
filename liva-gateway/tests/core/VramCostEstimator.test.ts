import { describe, it, expect } from "vitest";
import { VramCostEstimator, TaskType } from "../../src/core/VramCostEstimator";

describe("VramCostEstimator unit tests", () => {
    it("should retrieve correct estimated VRAM cost for known task types", () => {
        expect(VramCostEstimator.get(TaskType.LLM_ROUTER)).toBe(5300);
        expect(VramCostEstimator.get(TaskType.LLM_EXPERT)).toBe(6700);
        expect(VramCostEstimator.get(TaskType.LLM_STREAM)).toBe(4500);
        expect(VramCostEstimator.get(TaskType.AUDIO_VOICE)).toBe(300);
        expect(VramCostEstimator.get(TaskType.LIVE2D_RENDER_BUFFER)).toBe(150);
        expect(VramCostEstimator.get(TaskType.TELEMETRY)).toBe(50);
    });

    it("should fallback to 0 for unregistered or invalid task type", () => {
        expect(VramCostEstimator.get("UNKNOWN_TASK" as any)).toBe(0);
    });
});
