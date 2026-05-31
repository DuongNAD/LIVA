import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@utils/logger", () => ({
    logger: { info: vi.fn(), error: vi.fn(), warn: vi.fn(), debug: vi.fn() }
}));

vi.mock("@utils/HttpClient", () => ({
    safeFetch: vi.fn()
}));

vi.mock("@security/HITLGuard", () => ({
    HITLGuard: { requestApproval: vi.fn().mockResolvedValue(true) }
}));

import { execute, metadata } from "../../../src/skills/core/LinearJiraTracker";
import { safeFetch } from "../../../src/utils/HttpClient";
import { HITLGuard } from "../../../src/security/HITLGuard";

describe("Skill - LinearJiraTracker", () => {
    beforeEach(() => { vi.clearAllMocks(); });

    it("should export correct metadata", () => {
        expect(metadata.name).toBe("linear_jira_tracker");
    });

    it("should list issues via API", async () => {
        vi.mocked(safeFetch).mockResolvedValueOnce({ json: () => Promise.resolve([{ id: "LIVA-1" }]) } as any);
        const result = await execute({ action: "list_issues" });
        expect(result).toContain("TRACKER ISSUES");
        expect(result).toContain("LIVA-1");
    });

    it("should fallback to mock on list API fail", async () => {
        vi.mocked(safeFetch).mockRejectedValueOnce(new Error("Timeout"));
        const result = await execute({ action: "list_issues" });
        expect(result).toContain("MOCK");
    });

    it("should create issue with HITL approval", async () => {
        vi.mocked(safeFetch).mockResolvedValueOnce({ json: () => Promise.resolve({ status: "OK" }) } as any);
        const result = await execute({ action: "create_issue", title: "New Bug" });
        expect(HITLGuard.requestApproval).toHaveBeenCalled();
        expect(result).toContain("TRACKER SUCCESS");
    });

    it("should use mock fallback on create API fail", async () => {
        vi.mocked(safeFetch).mockRejectedValueOnce(new Error("API down"));
        const result = await execute({ action: "create_issue", title: "Bug" });
        expect(result).toContain("MOCK");
    });

    it("should update status with HITL approval", async () => {
        vi.mocked(safeFetch).mockResolvedValueOnce({ json: () => Promise.resolve({ status: "OK" }) } as any);
        const result = await execute({ action: "update_status", issueId: "LIVA-1", newStatus: "Done" });
        expect(HITLGuard.requestApproval).toHaveBeenCalled();
        expect(result).toContain("TRACKER SUCCESS");
    });

    it("should block if HITL denied", async () => {
        vi.mocked(HITLGuard.requestApproval).mockRejectedValueOnce(new Error("Denied"));
        const result = await execute({ action: "create_issue" });
        expect(result).toContain("TRACKER BLOCKED");
    });

    it("should return ZodError for invalid action", async () => {
        const result = await execute({ action: "invalid" });
        expect(result).toContain("TRACKER ERROR");
        expect(result).toContain("Sai định dạng");
    });
});
