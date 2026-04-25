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
            expect(AgentPhase.IDLE).toBeDefined();
            expect(AgentPhase.RUNNING).toBeDefined();
            expect(AgentPhase.AWAITING_APPROVAL).toBeDefined();
            expect(AgentPhase.PAUSING).toBeDefined();
            expect(AgentPhase.TERMINATING).toBeDefined();
        });

        it("should have unique values for each phase", () => {
            const values = Object.values(AgentPhase);
            const unique = new Set(values);
            expect(unique.size).toBe(values.length);
        });
    });

    describe("TaskLane", () => {
        it("should have all expected lanes", () => {
            expect(TaskLane.UI_INTERACTION).toBeDefined();
            expect(TaskLane.LLM_REASONING).toBeDefined();
            expect(TaskLane.BACKGROUND_JOB).toBeDefined();
        });
    });

    describe("TaskState", () => {
        it("should have all expected states", () => {
            expect(TaskState.PENDING).toBe("PENDING");
            expect(TaskState.EXECUTING).toBe("EXECUTING");
            expect(TaskState.COMPLETED).toBe("COMPLETED");
            expect(TaskState.FAILED).toBe("FAILED");
        });
    });

    describe("AuthorityToken", () => {
        it("should be constructable with phase and secret", () => {
            const token = new AuthorityToken(AgentPhase.RUNNING, "secret123");
            expect(token.phase).toBe(AgentPhase.RUNNING);
        });

        it("should validate correctly with matching phase and secret", () => {
            const token = new AuthorityToken(AgentPhase.IDLE, "mySecret");
            expect(token.isValid(AgentPhase.IDLE, "mySecret")).toBe(true);
        });

        it("should reject mismatched phase", () => {
            const token = new AuthorityToken(AgentPhase.IDLE, "mySecret");
            expect(token.isValid(AgentPhase.RUNNING as any, "mySecret")).toBe(false);
        });

        it("should reject mismatched secret", () => {
            const token = new AuthorityToken(AgentPhase.IDLE, "mySecret");
            expect(token.isValid(AgentPhase.IDLE, "wrongSecret")).toBe(false);
        });

        it("should reject both mismatched phase and secret", () => {
            const token = new AuthorityToken(AgentPhase.IDLE, "mySecret");
            expect(token.isValid(AgentPhase.RUNNING as any, "wrongSecret")).toBe(false);
        });
    });
});
