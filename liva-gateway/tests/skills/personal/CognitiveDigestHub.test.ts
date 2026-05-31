import { describe, it, expect, vi, beforeEach } from "vitest";
import { execute, metadata } from "../../../src/skills/personal/CognitiveDigestHub";
import { safeFetch } from "../../../src/utils/HttpClient";
import { EmbeddingService } from "../../../src/services/EmbeddingService";

vi.mock("@utils/logger", () => ({
    logger: { info: vi.fn(), error: vi.fn(), warn: vi.fn(), debug: vi.fn() }
}));

vi.mock("@utils/HttpClient", () => ({
    safeFetch: vi.fn()
}));

vi.mock("../../../src/services/EmbeddingService", () => {
    const mockEmbedSvc = {
        embedBatch: vi.fn().mockImplementation((texts: string[]) => {
            // Return vectors that are close or far depending on context to test deduplication
            return Promise.resolve(texts.map(t => {
                if (t.includes("Task 1")) return [0.1, 0.2, 0.3];
                if (t.includes("duplicate Task 1")) return [0.1, 0.2, 0.31]; // Close vector
                return [0.9, 0.8, 0.7]; // Different vector
            }));
        }),
        ready: true
    };
    return {
        EmbeddingService: {
            getInstance: () => mockEmbedSvc
        }
    };
});

describe("Skill - CognitiveDigestHub", () => {
    beforeEach(() => {
        vi.clearAllMocks();
        delete (globalThis as any).kernelInstance;
        EmbeddingService.getInstance().ready = true;
    });

    it("should export correct metadata", () => {
        expect(metadata.name).toBe("cognitive_digest_hub");
        expect(metadata.category).toBe("personal");
    });

    it("should fallback to mock mode when kernel/database is not set", async () => {
        const result = await execute({ action: "get_digest" });
        expect(result).toContain("Mock Mode");
        expect(result).toContain("📬 LIVA Cognitive Digest");
    });

    it("should return mock array for get_recent_events action when database is not set", async () => {
        const result = await execute({ action: "get_recent_events" });
        expect(Array.isArray(result)).toBe(true);
        expect(result[0].eventId).toBe("mock_ev_1");
    });

    it("should query database and request LLM summary in real mode", async () => {
        const mockDb = {
            all: vi.fn().mockImplementation((sql: string) => {
                if (sql.includes("turn_layer_nodes")) {
                    return Promise.resolve([
                        { userMsg: "Task 1 complete", aiReply: "Understood sếp.", temporal_anchor: Date.now() }
                    ]);
                }
                if (sql.includes("events")) {
                    return Promise.resolve([
                        { phi_facts: JSON.stringify(["LIVA added tasks"]), psi_intent: "Task sync", domain: "General", category: "Uncategorized", timestamp: Date.now() }
                    ]);
                }
                return Promise.resolve([]);
            })
        };

        (globalThis as any).kernelInstance = {
            memory: {
                getStructuredMemoryInstance: () => ({
                    dbBridge: mockDb
                })
            }
        };

        vi.mocked(safeFetch).mockResolvedValueOnce({
            json: () => Promise.resolve({
                choices: [{ message: { content: "Tóm tắt: Hoàn thành Task 1 thành công." } }]
            })
        } as any);

        const result = await execute({ action: "get_digest", time_window_hours: 6 });
        expect(mockDb.all).toHaveBeenCalledTimes(2);
        expect(result).toContain("Tóm tắt: Hoàn thành Task 1 thành công.");
    });

    it("should perform semantic deduplication on turns and events", async () => {
        const mockDb = {
            all: vi.fn().mockImplementation((sql: string) => {
                if (sql.includes("turn_layer_nodes")) {
                    return Promise.resolve([
                        { userMsg: "Task 1", aiReply: "OK", temporal_anchor: Date.now() },
                        { userMsg: "duplicate Task 1", aiReply: "OK", temporal_anchor: Date.now() + 1000 } // Duplicate
                    ]);
                }
                return Promise.resolve([]);
            })
        };

        (globalThis as any).kernelInstance = {
            memory: {
                getStructuredMemoryInstance: () => ({
                    dbBridge: mockDb
                })
            }
        };

        vi.mocked(safeFetch).mockResolvedValueOnce({
            json: () => Promise.resolve({
                choices: [{ message: { content: "Deduplicated Output" } }]
            })
        } as any);

        await execute({ action: "get_digest", time_window_hours: 1 });
        expect(EmbeddingService.getInstance().embedBatch).toHaveBeenCalled();
    });
});
