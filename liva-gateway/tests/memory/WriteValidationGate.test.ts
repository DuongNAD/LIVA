import { describe, it, expect, vi, beforeEach } from "vitest";
import { WriteValidationGate } from "../../src/incubating/WriteValidationGate";
import { logger } from "../../src/utils/logger";

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn()
    }
}));

// [v27] Moved to src/incubating/ — NLI model not yet integrated. See src/incubating/README.md
describe.skip("WriteValidationGate (INCUBATING)", () => {
    let gate: WriteValidationGate;

    beforeEach(() => {
        vi.clearAllMocks();
        gate = WriteValidationGate.getInstance();
    });

    it("should be a singleton", () => {
        const instance1 = WriteValidationGate.getInstance();
        const instance2 = WriteValidationGate.getInstance();
        expect(instance1).toBe(instance2);
    });

    it("should return true if proposedFact is empty or coreFacts is empty", async () => {
        const res1 = await gate.validateUpdate("", ["core fact"]);
        expect(res1).toBe(true);

        const res2 = await gate.validateUpdate("new fact", []);
        expect(res2).toBe(true);
    });

    it("should return true for non-contradictory facts", async () => {
        const coreFacts = ["User lives in Hanoi", "User loves soccer"];
        const res = await gate.validateUpdate("User is a software engineer", coreFacts);
        expect(res).toBe(true);
        expect(logger.info).toHaveBeenCalledWith(
            expect.stringContaining("[SSGM] ✅ Validation passed")
        );
    });

    it("should detect contradiction and return false for negated facts", async () => {
        const coreFacts = ["User lives in Hanoi", "User loves soccer"];
        const res = await gate.validateUpdate("User does not live in Hanoi", coreFacts);
        expect(res).toBe(false);
        expect(logger.warn).toHaveBeenCalledWith(
            expect.stringContaining("Memory Poisoning Blocked")
        );
    });
});
