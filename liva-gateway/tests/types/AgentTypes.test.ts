/**
 * AgentTypes.test.ts — Type system & brand tests
 */
import { describe, it, expect } from "vitest";
import {
    AgentPhase,
    TaskLane,
    TaskState,
    AuthorityToken,
} from "../../src/types/AgentTypes";

describe("AgentTypes", () => {
    describe("AgentPhase", () => {
        it("should have all expected phases", () => {
            expect(AgentPhase.INITIALIZING).toBeDefined();
            expect(AgentPhase.RUNNING).toBeDefined();
            expect(AgentPhase.PAUSING).toBeDefined();
            expect(AgentPhase.TERMINATING).toBeDefined();
        });
    });

    describe("TaskLane", () => {
        it("should have expected lanes", () => {
            expect(TaskLane.UI_INTERACTION).toBeDefined();
            expect(TaskLane.LLM_REASONING).toBeDefined();
            expect(TaskLane.BACKGROUND_JOB).toBeDefined();
        });
    });

    describe("TaskState", () => {
        it("should have expected states", () => {
            expect(TaskState.PENDING).toBe("PENDING");
            expect(TaskState.EXECUTING).toBe("EXECUTING");
            expect(TaskState.COMPLETED).toBe("COMPLETED");
            expect(TaskState.FAILED).toBe("FAILED");
        });
    });
});
