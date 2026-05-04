import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@utils/logger", () => ({
    logger: { info: vi.fn(), error: vi.fn(), warn: vi.fn(), debug: vi.fn() }
}));
vi.mock("@utils/HttpClient", () => ({ safeFetch: vi.fn() }));
vi.mock("@security/HITLGuard", () => ({ HITLGuard: { requestApproval: vi.fn().mockResolvedValue(true) } }));

import { execute, metadata } from "../../../src/skills/social/SocialMediaPoster";
import { safeFetch } from "../../../src/utils/HttpClient";
import { HITLGuard } from "../../../src/security/HITLGuard";

describe("Skill - SocialMediaPoster", () => {
    beforeEach(() => { vi.clearAllMocks(); });

    it("should export metadata", () => { expect(metadata.name).toBe("social_media_poster"); });

    it("should post to twitter with HITL + API", async () => {
        vi.mocked(safeFetch).mockResolvedValueOnce({ json: () => Promise.resolve({ status: "OK" }) } as any);
        const result = await execute({ platform: "twitter", content: "Hello World" });
        expect(HITLGuard.requestApproval).toHaveBeenCalled();
        expect(result).toContain("SOCIAL SUCCESS");
    });

    it("should fallback to mock on API fail", async () => {
        vi.mocked(safeFetch).mockRejectedValueOnce(new Error("timeout"));
        const result = await execute({ platform: "linkedin", content: "New post" });
        expect(result).toContain("MOCK MODE");
    });

    it("should block if HITL denied", async () => {
        vi.mocked(HITLGuard.requestApproval).mockRejectedValueOnce(new Error("Rejected"));
        const result = await execute({ platform: "facebook", content: "Test" });
        expect(result).toContain("SOCIAL BLOCKED");
    });

    it("should handle ZodError", async () => {
        const result = await execute({ platform: "tiktok", content: "x" });
        expect(result).toContain("SOCIAL ERROR");
    });
});
