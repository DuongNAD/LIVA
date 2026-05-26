import { describe, it, expect, vi, beforeEach } from "vitest";
import { ConsolidationPipeline, type ConsolidationContext } from "../../src/memory/ConsolidationPipeline";
import type { ConsolidationStep } from "../../src/memory/ConsolidationPipeline";

describe("ConsolidationPipeline", () => {
    let pipeline: ConsolidationPipeline;
    let mockDbExec: ReturnType<typeof vi.fn>;
    let mockDbPrepareGet: ReturnType<typeof vi.fn>;
    let mockDbPrepareRun: ReturnType<typeof vi.fn>;
    let mockRun: ReturnType<typeof vi.fn>;

    beforeEach(() => {
        mockDbExec = vi.fn();
        mockRun = vi.fn();
        
        mockDbPrepareGet = vi.fn().mockReturnValue({
            get: vi.fn()
        });
        
        mockDbPrepareRun = vi.fn().mockReturnValue({
            run: mockRun
        });

        pipeline = new ConsolidationPipeline(
            mockDbExec,
            mockDbPrepareGet,
            mockDbPrepareRun
        );
    });

    it("should register and count steps", () => {
        const step1: ConsolidationStep = { stepName: "Step1", execute: vi.fn() };
        const step2: ConsolidationStep = { stepName: "Step2", execute: vi.fn() };
        
        pipeline.addStep(step1).addStep(step2);
        
        expect(pipeline.stepCount).toBe(2);
        expect(pipeline.stepNames).toEqual(["Step1", "Step2"]);
    });

    it("should execute steps sequentially and update context", async () => {
        const ctx: ConsolidationContext = {
            sessionId: "test",
            currentStepIndex: 0,
            totalConsolidated: 0,
            sharedState: {}
        };

        const step1: ConsolidationStep = { 
            stepName: "Step1", 
            execute: vi.fn().mockImplementation(async (c) => { c.sharedState.s1 = true; }) 
        };
        const step2: ConsolidationStep = { 
            stepName: "Step2", 
            execute: vi.fn().mockImplementation(async (c) => { c.sharedState.s2 = true; }) 
        };

        pipeline.addStep(step1).addStep(step2);
        
        const completeSpy = vi.fn();
        pipeline.on("pipeline_complete", completeSpy);

        await pipeline.run(ctx);

        expect(step1.execute).toHaveBeenCalledWith(ctx);
        expect(step2.execute).toHaveBeenCalledWith(ctx);
        expect(ctx.currentStepIndex).toBe(2);
        expect(ctx.sharedState).toEqual({ s1: true, s2: true });
        
        // Ensure checkpoints were saved then cleared
        expect(mockRun).toHaveBeenCalledWith("test", 1, expect.any(String), expect.any(Number), expect.any(Number));
        expect(mockRun).toHaveBeenCalledWith("test", 2, expect.any(String), expect.any(Number), expect.any(Number));
        expect(mockRun).toHaveBeenCalledWith("test"); // clearCheckpoint
        
        expect(completeSpy).toHaveBeenCalledWith({ totalSteps: 2, totalConsolidated: 0 });
    });

    it("should stop execution and write to DLQ on failure", async () => {
        const ctx: ConsolidationContext = {
            sessionId: "test_fail",
            currentStepIndex: 0,
            totalConsolidated: 0,
            sharedState: {}
        };

        const step1: ConsolidationStep = { stepName: "Step1", execute: vi.fn() };
        const step2: ConsolidationStep = { 
            stepName: "Step2", 
            execute: vi.fn().mockRejectedValue(new Error("Step2 Error")) 
        };
        const step3: ConsolidationStep = { stepName: "Step3", execute: vi.fn() };

        pipeline.addStep(step1).addStep(step2).addStep(step3);
        
        const errorSpy = vi.fn();
        pipeline.on("pipeline_error", errorSpy);

        await pipeline.run(ctx);

        expect(step1.execute).toHaveBeenCalled();
        expect(step2.execute).toHaveBeenCalled();
        expect(step3.execute).not.toHaveBeenCalled(); // Stopped after step 2 failed
        
        expect(ctx.currentStepIndex).toBe(1); // Didn't increment for step 2
        
        expect(errorSpy).toHaveBeenCalledWith({
            step: "Step2",
            index: 1,
            error: expect.any(Error)
        });
        
        // Verify DLQ insert
        expect(mockRun).toHaveBeenCalledWith("test_fail", "Step2", "Step2 Error", expect.any(Number));
    });

    it("should resume from checkpoint", () => {
        mockDbPrepareGet.mockReturnValue({
            get: vi.fn().mockReturnValue({ last_step: 2, state_data: '{"s1":true}' })
        });

        const ctx = pipeline.resumeFromCheckpoint("test_resume");
        
        expect(ctx).not.toBeNull();
        expect(ctx?.currentStepIndex).toBe(2);
        expect(ctx?.sharedState).toEqual({ s1: true });
    });

    it("should return null if no checkpoint exists", () => {
        mockDbPrepareGet.mockReturnValue({
            get: vi.fn().mockReturnValue(undefined)
        });

        const ctx = pipeline.resumeFromCheckpoint("test_no_resume");
        expect(ctx).toBeNull();
    });
});
