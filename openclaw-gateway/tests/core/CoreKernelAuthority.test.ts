/**
 * CoreKernelAuthority.test.ts — Singleton authority + token validation
 */
import { describe, it, expect } from "vitest";
import { CoreKernelAuthority, AgentPhase } from "../../src/core";

describe("CoreKernelAuthority", () => {
    it("should return a singleton instance", () => {
        const a = CoreKernelAuthority.getInstance();
        const b = CoreKernelAuthority.getInstance();
        expect(a).toBe(b);
    });

    it("should issue a valid authority token", () => {
        const authority = CoreKernelAuthority.getInstance();
        const token = authority.issueToken(AgentPhase.RUNNING);
        expect(token).toBeDefined();
        expect(token.phase).toBe(AgentPhase.RUNNING);
    });

    it("should verify a valid token", () => {
        const authority = CoreKernelAuthority.getInstance();
        const token = authority.issueToken(AgentPhase.IDLE);
        expect(authority.verify(token, AgentPhase.IDLE)).toBe(true);
    });

    it("should reject a token with wrong phase", () => {
        const authority = CoreKernelAuthority.getInstance();
        const token = authority.issueToken(AgentPhase.RUNNING);
        // Verify against a different phase
        expect(authority.verify(token, AgentPhase.IDLE as any)).toBe(false);
    });

    it("should issue tokens for all phases", () => {
        const authority = CoreKernelAuthority.getInstance();
        for (const phase of Object.values(AgentPhase)) {
            const token = authority.issueToken(phase);
            expect(authority.verify(token, phase)).toBe(true);
        }
    });
});

describe("AuthorityToken", () => {
    it("should store the phase", () => {
        const token = CoreKernelAuthority.getInstance().issueToken(AgentPhase.INITIALIZING);
        expect(token.phase).toBe(AgentPhase.INITIALIZING);
    });

    it("should validate with correct secret using verify", () => {
        const token = CoreKernelAuthority.getInstance().issueToken(AgentPhase.RUNNING);
        expect(CoreKernelAuthority.getInstance().verify(token, AgentPhase.RUNNING)).toBe(true);
    });

    it("should reject with wrong phase using verify", () => {
        const token = CoreKernelAuthority.getInstance().issueToken(AgentPhase.RUNNING);
        expect(CoreKernelAuthority.getInstance().verify(token, AgentPhase.IDLE as any)).toBe(false);
    });
});
