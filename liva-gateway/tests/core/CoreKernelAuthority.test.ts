import { describe, it, expect } from "vitest";
import { CoreKernelAuthority } from "../../src/core/CoreKernelAuthority";

describe("CoreKernelAuthority", () => {
    it("should act as a singleton", () => {
        const instance1 = CoreKernelAuthority.getInstance();
        const instance2 = CoreKernelAuthority.getInstance();
        expect(instance1).toBe(instance2);
    });

    it("should issue and verify valid token", () => {
        const auth = CoreKernelAuthority.getInstance();
        const token = auth.issueToken("system");
        
        expect(token.phase).toBe("system");
        expect(auth.verify(token, "system")).toBe(true);
    });

    it("should fail verification with wrong phase", () => {
        const auth = CoreKernelAuthority.getInstance();
        const token = auth.issueToken("system");
        
        expect(auth.verify(token, "user")).toBe(false);
    });

    it("should fail verification if secret is somehow mismatched", () => {
        const auth = CoreKernelAuthority.getInstance();
        const token = auth.issueToken("system");
        
        // Mock a token with wrong secret
        const fakeToken = {
            phase: "system",
            isValid: (expectedPhase: string, expectedSecret: string) => expectedSecret === "WRONG"
        } as any;
        
        expect(auth.verify(fakeToken, "system")).toBe(false);
    });
});
