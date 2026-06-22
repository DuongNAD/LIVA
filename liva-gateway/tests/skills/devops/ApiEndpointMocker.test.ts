import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// Mock logger to avoid test log pollution
vi.mock("@utils/logger", () => ({
    logger: {
        info: vi.fn(),
        error: vi.fn(),
        warn: vi.fn(),
        debug: vi.fn(),
    },
}));

import { execute, metadata, mockServerRegistry } from "../../../src/skills/devops/ApiEndpointMocker";

describe("ApiEndpointMocker Skill", () => {
    let serverId: string = "";
    const port = 13950;

    beforeEach(() => {
        vi.clearAllMocks();
    });

    afterEach(async () => {
        // Cleanup all mock servers between tests
        await mockServerRegistry.dispose();
    });

    it("should export correct metadata", () => {
        expect(metadata.name).toBe("api_endpoint_mocker");
        expect(metadata.kit).toBe("DEVOPS_KIT");
    });

    describe("server lifecycle and API matching", () => {
        it("should create server, handle GET and POST with templated bodies, query params, path params, list and stop", async () => {
            // 1. Create Mock Server
            const createResult = await execute({
                action: "create",
                port,
                endpoints: [
                    {
                        method: "GET",
                        path: "/api/info",
                        response: { status: "active" },
                        statusCode: 200,
                    },
                    {
                        method: "POST",
                        path: "/api/items/:id",
                        response: {
                            id: "{{request_path}}",
                            body: "{{request_body}}",
                            time: "{{timestamp}}",
                        },
                        statusCode: 201,
                    },
                    {
                        method: "GET",
                        path: "/api/delay",
                        response: { delayed: true },
                        statusCode: 200,
                        delay: 50,
                    },
                ],
            });

            expect(createResult).toContain("[MOCK SUCCESS]");
            expect(createResult).toContain(`http://localhost:${port}`);

            // Extract server ID from result
            const match = createResult.match(/Server ID: (mock_\w+)/);
            expect(match).not.toBeNull();
            serverId = match![1];

            // 2. Test GET request
            const getRes = await fetch(`http://127.0.0.1:${port}/api/info`);
            expect(getRes.status).toBe(200);
            expect(getRes.headers.get("access-control-allow-origin")).toBe("*");
            expect(getRes.headers.get("x-mock-server")).toBe(serverId);
            
            const getData = await getRes.json();
            expect(getData).toEqual({ status: "active" });

            // 3. Test POST request with body and parameter matching (e.g. /api/items/99)
            const postBody = { name: "test-item", qty: 5 };
            const postRes = await fetch(`http://127.0.0.1:${port}/api/items/99`, {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify(postBody),
            });
            expect(postRes.status).toBe(201);
            
            const postData = await postRes.json();
            expect(postData.id).toBe("/api/items/99");
            expect(postData.body).toEqual(postBody);
            expect(postData.time).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}/); // ISO format

            // 4. Test 404 for unmatched path
            const notFoundRes = await fetch(`http://127.0.0.1:${port}/api/nonexistent`);
            expect(notFoundRes.status).toBe(404);
            const notFoundData = await notFoundRes.json();
            expect(notFoundData.error).toBe("Endpoint not found");

            // 5. Test OPTIONS preflight request
            const preflightRes = await fetch(`http://127.0.0.1:${port}/api/info`, {
                method: "OPTIONS",
            });
            expect(preflightRes.status).toBe(204);

            // 6. Test delay response
            const start = Date.now();
            const delayRes = await fetch(`http://127.0.0.1:${port}/api/delay`);
            const elapsed = Date.now() - start;
            expect(delayRes.status).toBe(200);
            expect(elapsed).toBeGreaterThanOrEqual(40); // should be close to 50ms

            // 7. Test list action
            const listResult = await execute({ action: "list" });
            expect(listResult).toContain(serverId);
            expect(listResult).toContain(`/api/info`);
            expect(listResult).toContain(`/api/items/:id`);

            // 8. Test stop action
            const stopResult = await execute({ action: "stop", serverId });
            expect(stopResult).toContain("[MOCK SUCCESS] Đã dừng mock server");

            // 9. Test list action after stopping
            const listAfterResult = await execute({ action: "list" });
            expect(listAfterResult).toContain("Không có mock server nào đang chạy");
        });

        it("should refuse to create server on already used port in registry", async () => {
            await execute({
                action: "create",
                port,
                endpoints: [{ method: "GET", path: "/api/1", response: {} }],
            });

            const result = await execute({
                action: "create",
                port,
                endpoints: [{ method: "GET", path: "/api/2", response: {} }],
            });

            expect(result).toContain("[MOCK ERROR] Port");
            expect(result).toContain("đã được dùng");
        });

        it("should return error when stopping nonexistent server", async () => {
            const result = await execute({ action: "stop", serverId: "mock_nonexistent" });
            expect(result).toContain("[MOCK ERROR] Không tìm thấy server");
        });
    });

    describe("validation", () => {
        it("should require endpoints for create action", async () => {
            const result = await execute({ action: "create" });
            expect(result).toContain("[MOCK ERROR] Cần cung cấp ít nhất 1 endpoint");
        });

        it("should require serverId for stop action", async () => {
            const result = await execute({ action: "stop" });
            expect(result).toContain("[MOCK ERROR] Cần cung cấp 'serverId'");
        });

        it("should reject invalid actions", async () => {
            const result = await execute({ action: "invalid_action" });
            expect(result).toContain("[MOCK ERROR] Sai định dạng");
        });
    });
});
