import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

vi.mock("../../src/utils/HttpClient", () => ({
    safeFetch: vi.fn(),
}));

vi.mock("node:dns/promises", () => ({
    resolve4: vi.fn().mockResolvedValue(["142.250.80.46"]),
}));

import { metadata, execute } from "../../src/skills/devops/NetworkDiagnostics";
import { safeFetch } from "../../src/utils/HttpClient";

describe("NetworkDiagnostics", () => {
    beforeEach(() => { vi.clearAllMocks(); });

    it("should have correct metadata", () => {
        expect(metadata.name).toBe("network_diagnostics");
        expect(metadata.parameters.properties.action.enum).toContain("full");
    });

    it("should run ping check", async () => {
        (safeFetch as any).mockResolvedValue({ ok: true });
        const result = await execute({ action: "ping", host: "google.com" });
        expect(result).toContain("NETWORK DIAGNOSTICS");
        expect(result).toContain("google.com");
    });

    it("should run DNS check", async () => {
        const result = await execute({ action: "dns", host: "google.com" });
        expect(result).toContain("DNS");
        expect(result).toContain("142.250.80.46");
    });

    it("should handle ping failure gracefully", async () => {
        (safeFetch as any).mockRejectedValue(new Error("ECONNREFUSED"));
        const result = await execute({ action: "ping", host: "bad.host" });
        expect(result).toContain("Unreachable");
    });

    it("should run full diagnostics", async () => {
        (safeFetch as any).mockResolvedValue({ ok: true, arrayBuffer: async () => new ArrayBuffer(100) });
        const result = await execute({ action: "full" });
        expect(result).toContain("NETWORK DIAGNOSTICS");
    });
});
