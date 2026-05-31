import { z } from "zod";
import { logger } from "@utils/logger";
import * as http from "node:http";
import * as url from "node:url";
import * as crypto from "node:crypto";

// ── Zod Schema ──────────────────────────────────────────────────────────────
const EndpointSchema = z.object({
    method: z.enum(["GET", "POST", "PUT", "DELETE", "PATCH"]).describe("HTTP method"),
    path: z.string().min(1).describe("Endpoint path, ví dụ: /api/users"),
    response: z.any().describe("JSON response body"),
    statusCode: z.number().min(100).max(599).optional().default(200).describe("HTTP status code"),
    delay: z.number().min(0).max(30000).optional().default(0).describe("Response delay (ms)"),
});

const ApiMockerSchema = z.object({
    action: z.enum(["create", "stop", "list"]).describe("Hành động: tạo server, dừng server, hoặc liệt kê"),
    endpoints: z.array(EndpointSchema).optional().describe("Danh sách endpoint mock"),
    port: z.number().min(1024).max(65535).optional().describe("Port cho mock server (1024-65535)"),
    serverId: z.string().optional().describe("Server ID để dừng (dùng cho action 'stop')"),
});

// ── Metadata ────────────────────────────────────────────────────────────────
export const metadata = {
    name: "api_endpoint_mocker",
    description: "[AUTO_RUN] Instantly spawn mock REST API servers. Define endpoints with JSON responses and LIVA creates an HTTP server on a random port for frontend testing.",
    kit: "DEVOPS_KIT",
    search_keywords: ["mock", "api", "rest", "server", "endpoint", "test", "frontend", "giả lập"],
    parameters: {
        type: "object",
        properties: {
            action: { type: "string", enum: ["create", "stop", "list"] },
            endpoints: {
                type: "array",
                items: {
                    type: "object",
                    properties: {
                        method: { type: "string", enum: ["GET", "POST", "PUT", "DELETE", "PATCH"] },
                        path: { type: "string", description: "Endpoint path (e.g., /api/users)" },
                        response: { description: "JSON response body" },
                        statusCode: { type: "number", description: "HTTP status code (default 200)" },
                        delay: { type: "number", description: "Response delay in ms (default 0)" },
                    },
                    required: ["method", "path", "response"],
                },
                description: "List of mock endpoints",
            },
            port: { type: "number", description: "Port number (random 3100-3999 if not specified)" },
            serverId: { type: "string", description: "Server ID to stop" },
        },
        required: ["action"],
    },
};

// ── Server info type ────────────────────────────────────────────────────────
interface EndpointConfig {
    method: string;
    path: string;
    response: unknown;
    statusCode: number;
    delay: number;
}

interface MockServerInfo {
    server: http.Server;
    port: number;
    endpoints: EndpointConfig[];
    createdAt: number;
    requestCount: number;
}

// ── MockServerRegistry Singleton ────────────────────────────────────────────
class MockServerRegistry {
    #servers: Map<string, MockServerInfo> = new Map();

    /** Tạo mock server mới */
    async create(endpoints: EndpointConfig[], requestedPort?: number): Promise<string> {
        // Tạo port ngẫu nhiên nếu không chỉ định
        const port = requestedPort ?? (3100 + Math.floor(Math.random() * 900));

        // Kiểm tra port đã được dùng bởi server khác trong registry
        for (const [id, info] of this.#servers) {
            if (info.port === port) {
                return `[MOCK ERROR] Port ${port} đã được dùng bởi mock server "${id}". Chọn port khác hoặc dừng server đó.`;
            }
        }

        // Generate server ID
        const serverId = `mock_${port}_${crypto.randomBytes(3).toString("hex")}`;

        // Tạo HTTP server
        const server = http.createServer((req, res) => {
            this.#handleRequest(serverId, req, res);
        });

        const serverInfo: MockServerInfo = {
            server,
            port,
            endpoints,
            createdAt: Date.now(),
            requestCount: 0,
        };

        // Listen trên port
        return new Promise<string>((resolve) => {
            server.on("error", (err: NodeJS.ErrnoException) => {
                if (err.code === "EADDRINUSE") {
                    resolve(`[MOCK ERROR] Port ${port} đã bị chiếm. Thử port khác.`);
                } else {
                    resolve(`[MOCK ERROR] Không thể khởi tạo server: ${err.message}`);
                }
            });

            server.listen(port, "127.0.0.1", () => {
                this.#servers.set(serverId, serverInfo);

                // Build endpoint listing
                let endpointList = "";
                for (const ep of endpoints) {
                    endpointList += `  ${ep.method.padEnd(7)} http://localhost:${port}${ep.path} → ${ep.statusCode}`;
                    if (ep.delay > 0) endpointList += ` (delay: ${ep.delay}ms)`;
                    endpointList += "\n";
                }

                logger.info(`[ApiMocker] ✅ Mock server "${serverId}" chạy tại http://localhost:${port} với ${endpoints.length} endpoints.`);

                resolve(
                    `[MOCK SUCCESS] Mock server đã khởi tạo!\n` +
                    `- Server ID: ${serverId}\n` +
                    `- URL: http://localhost:${port}\n` +
                    `- Endpoints:\n${endpointList}` +
                    `\nDùng action "stop" với serverId "${serverId}" để dừng.`,
                );
            });
        });
    }

    /** Xử lý request đến */
    #handleRequest(serverId: string, req: http.IncomingMessage, res: http.ServerResponse): void {
        const info = this.#servers.get(serverId);
        if (!info) {
            res.writeHead(500, { "Content-Type": "application/json" });
            res.end(JSON.stringify({ error: "Server not found" }));
            return;
        }

        info.requestCount++;

        // CORS headers
        res.setHeader("Access-Control-Allow-Origin", "*");
        res.setHeader("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, PATCH, OPTIONS");
        res.setHeader("Access-Control-Allow-Headers", "Content-Type, Authorization, X-Requested-With");
        res.setHeader("X-Mock-Server", serverId);

        // Handle preflight
        if (req.method === "OPTIONS") {
            res.writeHead(204);
            res.end();
            return;
        }

        // Parse URL
        const parsedUrl = url.parse(req.url ?? "/", true);
        const reqPath = parsedUrl.pathname ?? "/";
        const reqMethod = (req.method ?? "GET").toUpperCase();

        // Tìm endpoint phù hợp
        const endpoint = info.endpoints.find(
            (ep) => ep.method === reqMethod && this.#matchPath(ep.path, reqPath),
        );

        if (!endpoint) {
            // 404 với danh sách endpoint available
            const availableEndpoints = info.endpoints
                .map((ep) => `  ${ep.method} ${ep.path}`)
                .join("\n");

            res.writeHead(404, { "Content-Type": "application/json" });
            res.end(
                JSON.stringify({
                    error: "Endpoint not found",
                    message: `${reqMethod} ${reqPath} không tồn tại trên mock server.`,
                    available_endpoints: info.endpoints.map((ep) => `${ep.method} ${ep.path}`),
                }),
            );

            logger.info(`[ApiMocker] 404: ${reqMethod} ${reqPath} on ${serverId}`);
            return;
        }

        // Collect request body for POST/PUT/PATCH
        let body = "";
        req.on("data", (chunk) => {
            body += chunk.toString();
            // Giới hạn body 1MB
            if (body.length > 1024 * 1024) {
                res.writeHead(413, { "Content-Type": "application/json" });
                res.end(JSON.stringify({ error: "Request body too large (max 1MB)" }));
                req.destroy();
            }
        });

        req.on("end", () => {
            // Parse body nếu có
            let parsedBody: unknown = null;
            if (body) {
                try {
                    parsedBody = JSON.parse(body);
                } catch {
                    parsedBody = body;
                }
            }

            const sendResponse = () => {
                // Xây dựng response
                let responseBody: unknown = endpoint.response;

                // Nếu response là function template (chứa {{body}}), thay thế
                if (typeof responseBody === "object" && responseBody !== null) {
                    responseBody = JSON.parse(
                        JSON.stringify(responseBody)
                            .replace(/"{{request_body}}"/g, JSON.stringify(parsedBody ?? null))
                            .replace(/"{{timestamp}}"/g, JSON.stringify(new Date().toISOString()))
                            .replace(/"{{request_path}}"/g, JSON.stringify(reqPath)),
                    );
                }

                res.writeHead(endpoint.statusCode, { "Content-Type": "application/json" });
                res.end(JSON.stringify(responseBody));

                logger.info(`[ApiMocker] ${reqMethod} ${reqPath} → ${endpoint.statusCode} on ${serverId}`);
            };

            // Apply delay nếu có
            if (endpoint.delay > 0) {
                const delayTimer = setTimeout(sendResponse, endpoint.delay);
                delayTimer.unref();
            } else {
                sendResponse();
            }
        });
    }

    /** Kiểm tra path matching (hỗ trợ path params đơn giản) */
    #matchPath(pattern: string, actual: string): boolean {
        // Exact match
        if (pattern === actual) return true;

        // Path param matching: /api/users/:id → /api/users/123
        const patternParts = pattern.split("/");
        const actualParts = actual.split("/");

        if (patternParts.length !== actualParts.length) return false;

        for (let i = 0; i < patternParts.length; i++) {
            if (patternParts[i].startsWith(":")) continue; // Param wildcard
            if (patternParts[i] !== actualParts[i]) return false;
        }

        return true;
    }

    /** Dừng mock server */
    async stop(serverId: string): Promise<string> {
        const info = this.#servers.get(serverId);
        if (!info) {
            return `[MOCK ERROR] Không tìm thấy server "${serverId}". Dùng action "list" để xem danh sách.`;
        }

        return new Promise<string>((resolve) => {
            info.server.close(() => {
                const uptime = Math.round((Date.now() - info.createdAt) / 1000);
                this.#servers.delete(serverId);

                logger.info(`[ApiMocker] Đã dừng server "${serverId}" (port ${info.port}).`);
                resolve(
                    `[MOCK SUCCESS] Đã dừng mock server "${serverId}".\n` +
                    `- Port: ${info.port}\n` +
                    `- Uptime: ${uptime}s\n` +
                    `- Tổng requests: ${info.requestCount}`,
                );
            });

            // Force close kết nối đang mở
            info.server.closeAllConnections?.();
        });
    }

    /** Liệt kê tất cả mock servers */
    list(): string {
        if (this.#servers.size === 0) {
            return "[MOCK STATUS] Không có mock server nào đang chạy.";
        }

        let output = `[MOCK STATUS] ${this.#servers.size} mock server đang chạy:\n\n`;

        for (const [id, info] of this.#servers) {
            const uptime = Math.round((Date.now() - info.createdAt) / 1000);
            output += `🖥️ ${id}\n`;
            output += `   URL: http://localhost:${info.port}\n`;
            output += `   Uptime: ${uptime}s\n`;
            output += `   Requests: ${info.requestCount}\n`;
            output += `   Endpoints:\n`;
            for (const ep of info.endpoints) {
                output += `     ${ep.method.padEnd(7)} ${ep.path} → ${ep.statusCode}`;
                if (ep.delay > 0) output += ` (delay: ${ep.delay}ms)`;
                output += "\n";
            }
            output += "\n";
        }

        return output;
    }

    /** Cleanup tất cả servers */
    async dispose(): Promise<void> {
        for (const [id, info] of this.#servers) {
            info.server.close();
            info.server.closeAllConnections?.();
            logger.info(`[ApiMocker] Đã đóng server: ${id}`);
        }
        this.#servers.clear();
    }
}

// ── Singleton instance ──────────────────────────────────────────────────────
export const mockServerRegistry = new MockServerRegistry();

// ── Execute function ────────────────────────────────────────────────────────
export const execute = async (argsObj: unknown): Promise<string> => {
    try {
        const parsed = ApiMockerSchema.parse(argsObj);

        switch (parsed.action) {
            case "create": {
                if (!parsed.endpoints || parsed.endpoints.length === 0) {
                    return "[MOCK ERROR] Cần cung cấp ít nhất 1 endpoint để tạo mock server.";
                }
                return await mockServerRegistry.create(parsed.endpoints, parsed.port);
            }
            case "stop": {
                if (!parsed.serverId) {
                    return "[MOCK ERROR] Cần cung cấp 'serverId' để dừng server. Dùng action 'list' để xem danh sách.";
                }
                return await mockServerRegistry.stop(parsed.serverId);
            }
            case "list":
                return mockServerRegistry.list();
            default:
                return "[MOCK ERROR] Hành động không hợp lệ.";
        }
    } catch (error: unknown) {
        const errMsg = error instanceof Error ? error.message : String(error);
        logger.error(`[ApiMocker] Lỗi: ${errMsg}`);
        if (error instanceof z.ZodError) {
            return `[MOCK ERROR] Sai định dạng: ${error.issues.map((e) => e.message).join(", ")}`;
        }
        return `[MOCK ERROR] Lỗi hệ thống: ${errMsg}`;
    }
};
