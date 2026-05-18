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

import { execute, metadata } from "../../../src/skills/personal/CalendarScheduler";
import { safeFetch } from "../../../src/utils/HttpClient";
import { HITLGuard } from "../../../src/security/HITLGuard";

describe("Skill - CalendarScheduler", () => {
    beforeEach(() => { vi.clearAllMocks(); });

    it("should export correct metadata", () => {
        expect(metadata.name).toBe("calendar_scheduler");
    });

    it("should list calendar events via API", async () => {
        vi.mocked(safeFetch).mockResolvedValueOnce({ json: () => Promise.resolve([{ title: "Meeting" }]) } as any);
        const result = await execute({ action: "list" });
        expect(result).toContain("CALENDAR LIST");
        expect(result).toContain("Meeting");
    });

    it("should fallback to mock when list API fails", async () => {
        vi.mocked(safeFetch).mockRejectedValueOnce(new Error("API down"));
        const result = await execute({ action: "list" });
        expect(result).toContain("MOCK MODE");
    });

    it("should create event with HITL approval", async () => {
        vi.mocked(safeFetch).mockResolvedValueOnce({ json: () => Promise.resolve({ status: "created" }) } as any);
        const result = await execute({ action: "create", title: "Sprint Review", startTime: "2026-05-10T10:00:00Z" });
        expect(HITLGuard.requestApproval).toHaveBeenCalled();
        expect(result).toContain("CALENDAR CREATE SUCCESS");
    });

    it("should use mock fallback when create API fails", async () => {
        vi.mocked(safeFetch).mockRejectedValueOnce(new Error("API down"));
        const result = await execute({ action: "create", title: "Demo", startTime: "2026-05-10T10:00:00Z" });
        expect(result).toContain("MOCK MODE");
    });

    it("should block create if HITL denied", async () => {
        vi.mocked(HITLGuard.requestApproval).mockRejectedValueOnce(new Error("Rejected"));
        const result = await execute({ action: "create", title: "Test", startTime: "2026-05-10T10:00:00Z" });
        expect(result).toContain("CALENDAR ACTION BLOCKED");
    });

    it("should error if create is missing title or startTime", async () => {
        const result = await execute({ action: "create" });
        expect(result).toContain("CALENDAR ERROR");
    });

    it("should handle ZodError for invalid action", async () => {
        const result = await execute({ action: "delete" });
        expect(result).toContain("CALENDAR ERROR");
        expect(result).toContain("Sai định dạng");
    });
});
