/**
 * CreateGoogleDoc.test.ts — Coverage Tests
 * Mocks the auth wrapper and googleapis module to test
 * document creation logic without real API calls.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock the auth wrapper
vi.mock("../../src/utils/googleAuth", () => ({
    getGoogleAuthClient: vi.fn().mockResolvedValue({ type: "mock-auth" }),
}));

// Mock googleapis — thin mock of docs API
const mockCreate = vi.fn().mockResolvedValue({
    data: { documentId: "new-doc-789" },
});
const mockBatchUpdate = vi.fn().mockResolvedValue({});

vi.mock("googleapis", () => ({
    google: {
        docs: vi.fn(() => ({
            documents: {
                create: mockCreate,
                batchUpdate: mockBatchUpdate,
            },
        })),
    },
}));

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn(),
    },
}));

import { metadata, execute } from "../../src/skills/CreateGoogleDoc";

describe("CreateGoogleDoc Skill", () => {
    beforeEach(() => {
        vi.clearAllMocks();
        mockCreate.mockResolvedValue({
            data: { documentId: "new-doc-789" },
        });
        mockBatchUpdate.mockResolvedValue({});
    });

    describe("metadata", () => {
        it("should export correct skill name", () => {
            expect(metadata.name).toBe("create_google_doc");
        });

        it("should require title and content", () => {
            expect(metadata.parameters.required).toContain("title");
            expect(metadata.parameters.required).toContain("content");
        });

        it("should have search keywords", () => {
            expect(metadata.search_keywords).toBeDefined();
            expect(metadata.search_keywords.length).toBeGreaterThan(0);
        });
    });

    describe("execute — happy path", () => {
        it("should create a document and return success with link", async () => {
            const result = await execute({
                title: "Test Document",
                content: "Hello World Content",
            });

            expect(result).toContain("✅");
            expect(result).toContain("new-doc-789");
            expect(result).toContain("docs.google.com");
            expect(result).toContain("Test Document");
            expect(mockCreate).toHaveBeenCalled();
            expect(mockBatchUpdate).toHaveBeenCalled();
        });

        it("should pass title to create API", async () => {
            await execute({ title: "My Title", content: "Content" });

            const createCall = mockCreate.mock.calls[0][0];
            expect(createCall.requestBody.title).toBe("My Title");
        });

        it("should insert content at index 1", async () => {
            await execute({ title: "Title", content: "Body text here" });

            const batchCall = mockBatchUpdate.mock.calls[0][0];
            const insertRequest = batchCall.requestBody.requests[0].insertText;
            expect(insertRequest.location.index).toBe(1);
            expect(insertRequest.text).toBe("Body text here");
        });
    });

    describe("execute — edge cases", () => {
        it("should handle null documentId from API", async () => {
            mockCreate.mockResolvedValueOnce({
                data: { documentId: null },
            });

            const result = await execute({ title: "Null ID", content: "text" });
            expect(result).toContain("Lỗi");
        });
    });

    describe("execute — error handling", () => {
        it("should return error message when create API fails", async () => {
            mockCreate.mockRejectedValueOnce(new Error("Quota limit reached"));

            const result = await execute({
                title: "Failing Doc",
                content: "text",
            });
            expect(result).toContain("❌");
            expect(result).toContain("Quota limit reached");
        });

        it("should return error message when batchUpdate fails", async () => {
            mockBatchUpdate.mockRejectedValueOnce(new Error("Network timeout"));

            const result = await execute({
                title: "Timeout Doc",
                content: "text",
            });
            expect(result).toContain("❌");
            expect(result).toContain("Network timeout");
        });
    });
});
