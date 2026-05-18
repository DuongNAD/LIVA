import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import * as http from "node:http";
import * as crypto from "node:crypto";
import { MetaBridge } from "../../src/channels/MetaBridge";

// Mock safeFetch
const mockSafeFetch = vi.fn();
vi.mock("../../src/utils/HttpClient", () => ({
    safeFetch: (...args: any[]) => mockSafeFetch(...args)
}));

describe("MetaBridge", () => {
    let bridge: MetaBridge;
    let currentPort: number;
    let portCounter = 3001;
    const originalEnv = process.env;

    beforeEach(() => {
        vi.clearAllMocks();
        process.env = { 
            ...originalEnv, 
            META_APP_SECRET: "test_secret",
            META_PAGE_ACCESS_TOKEN: "test_token",
            META_VERIFY_TOKEN: "test_verify"
        };
        currentPort = portCounter++;
        bridge = new MetaBridge(currentPort); 
    });

    afterEach(() => {
        process.env = originalEnv;
        bridge.stop();
    });

    describe("Lifecycle", () => {
        it("should not start if secrets are missing", async () => {
            delete process.env.META_APP_SECRET;
            const tempBridge = new MetaBridge(3002);
            await tempBridge.startWebhookServer();
            // Should not throw, but server shouldn't start (no hanging)
            tempBridge.stop();
        });

        it("should reject start if port is already in use (Line 47)", async () => {
            // Restore env manually since previous test deleted it
            process.env.META_APP_SECRET = "test_secret";
            process.env.META_PAGE_ACCESS_TOKEN = "test_token";
            
            const dummyServer = http.createServer();
            await new Promise<void>(resolve => dummyServer.listen(currentPort, "0.0.0.0", () => resolve()));
            
            const bridge2 = new MetaBridge(currentPort);
            await expect(bridge2.startWebhookServer()).rejects.toThrow();
            
            dummyServer.close();
        });

        it("should start and stop webhook server cleanly", async () => {
            // startWebhookServer resolves only when listen() is successful
            await bridge.startWebhookServer();
            expect(true).toBe(true); // Verification that promise resolved

            bridge.stop();
        });
    });

    describe("ChannelAdapter", () => {
        it("sendText should call Graph API correctly", async () => {
            await bridge.sendText("user123", "Hello Meta");
            
            expect(mockSafeFetch).toHaveBeenCalledTimes(1);
            const callArgs = mockSafeFetch.mock.calls[0];
            expect(callArgs[0]).toContain("access_token=test_token");
            expect(callArgs[1].method).toBe("POST");
            
            const body = JSON.parse(callArgs[1].body);
            expect(body.recipient.id).toBe("user123");
            expect(body.message.text).toBe("Hello Meta");
        });

        it("sendText should truncate messages over 2000 chars", async () => {
            const longText = "a".repeat(2500);
            await bridge.sendText("user123", longText);
            
            const body = JSON.parse(mockSafeFetch.mock.calls[0][1].body);
            expect(body.message.text.length).toBeLessThanOrEqual(2000);
            expect(body.message.text.endsWith("...")).toBe(true);
        });

        it("sendApprovalCard should send generic template", async () => {
            await bridge.sendApprovalCard("user123", "Alert", "Confirm execution", "apprv1");
            
            const body = JSON.parse(mockSafeFetch.mock.calls[0][1].body);
            const attachment = body.message.attachment;
            expect(attachment.type).toBe("template");
            expect(attachment.payload.template_type).toBe("button");
            expect(attachment.payload.buttons).toHaveLength(2);
            expect(attachment.payload.buttons[0].payload).toBe("approve:apprv1");
        });

        it("sendScreenshot should not throw (Lines 125-135)", async () => {
            // It currently just logs, so we just verify it resolves
            await expect(bridge.sendScreenshot("user123", Buffer.from("test"))).resolves.toBeUndefined();
        });

        it("should catch and log errors in sendText (Line 83)", async () => {
            mockSafeFetch.mockRejectedValueOnce(new Error("Network error"));
            await bridge.sendText("user123", "Fail text");
        });

        it("should catch and log errors in sendApprovalCard (Line 121)", async () => {
            mockSafeFetch.mockRejectedValueOnce(new Error("Network error"));
            await bridge.sendApprovalCard("user123", "Alert", "Text", "id");
        });

        it("should catch and log errors in sendScreenshot (Line 133)", async () => {
            // Force error inside the try block by mocking logger.info
            const { logger } = await import("../../src/utils/logger");
            vi.spyOn(logger, "info").mockImplementationOnce(() => { throw new Error("Simulated log error"); });
            await bridge.sendScreenshot("user123", Buffer.from("test"));
        });
    });

    describe("Webhook Handling", () => {
        // Helper to simulate incoming HTTP requests to our local server
        const simulateRequest = (options: http.RequestOptions, body?: string): Promise<{status: number, data: string}> => {
            return new Promise((resolve) => {
                const req = http.request(options, (res) => {
                    let data = "";
                    res.on("data", chunk => data += chunk);
                    res.on("end", () => resolve({ status: res.statusCode || 500, data }));
                });
                if (body) req.write(body);
                req.end();
            });
        };

        beforeEach(async () => {
            await bridge.startWebhookServer();
        });

        it("should handle GET verification successfully", async () => {
            const res = await simulateRequest({
                hostname: "127.0.0.1",
                port: currentPort,
                path: "/webhook?hub.mode=subscribe&hub.verify_token=test_verify&hub.challenge=12345",
                method: "GET"
            });
            expect(res.status).toBe(200);
            expect(res.data).toBe("12345");
        });

        it("should reject GET verification with wrong token", async () => {
            const res = await simulateRequest({
                hostname: "127.0.0.1",
                port: currentPort,
                path: "/webhook?hub.mode=subscribe&hub.verify_token=wrong_token",
                method: "GET"
            });
            expect(res.status).toBe(403);
        });

        it("should reject POST with invalid signature", async () => {
            const body = JSON.stringify({ object: "page", entry: [] });
            const res = await simulateRequest({
                hostname: "127.0.0.1",
                port: currentPort,
                path: "/webhook",
                method: "POST",
                headers: {
                    "X-Hub-Signature-256": "sha256=invalidhash"
                }
            }, body);
            expect(res.status).toBe(401);
        });

        it("should accept POST with valid signature and emit message", async () => {
            const payload = {
                object: "page",
                entry: [{
                    messaging: [{
                        sender: { id: "sender123" },
                        message: { mid: "mid1", text: "Hello" },
                        timestamp: 1000
                    }]
                }]
            };
            const bodyStr = JSON.stringify(payload);
            const hash = crypto.createHmac("sha256", "test_secret").update(bodyStr).digest("hex");
            
            // Spy on event emitter
            const emitSpy = vi.fn();
            bridge.on("message", emitSpy);

            const res = await simulateRequest({
                hostname: "127.0.0.1",
                port: currentPort,
                path: "/webhook",
                method: "POST",
                headers: { "X-Hub-Signature-256": `sha256=${hash}` }
            }, bodyStr);

            expect(res.status).toBe(200);
            expect(emitSpy).toHaveBeenCalledTimes(1);
            const emittedMsg = emitSpy.mock.calls[0][0];
            expect(emittedMsg.text).toBe("Hello");
            expect(emittedMsg.senderId).toBe("sender123");
        });

        it("should emit postback event on valid postback payload (Lines 203-207)", async () => {
            const payload = {
                object: "page",
                entry: [{
                    messaging: [{
                        sender: { id: "sender123" },
                        postback: { payload: "approve:apprv1" },
                        timestamp: 1000
                    }]
                }]
            };
            const bodyStr = JSON.stringify(payload);
            const hash = crypto.createHmac("sha256", "test_secret").update(bodyStr).digest("hex");
            
            const emitSpy = vi.fn();
            bridge.on("postback", emitSpy);

            const res = await simulateRequest({
                hostname: "127.0.0.1", port: currentPort, path: "/webhook", method: "POST",
                headers: { "X-Hub-Signature-256": `sha256=${hash}` }
            }, bodyStr);

            expect(res.status).toBe(200);
            expect(emitSpy).toHaveBeenCalledTimes(1);
            expect(emitSpy.mock.calls[0][0].payload).toBe("approve:apprv1");
        });

        it("should return 404 for unknown route (Line 147)", async () => {
            const res = await simulateRequest({ hostname: "127.0.0.1", port: currentPort, path: "/unknown", method: "GET" });
            expect(res.status).toBe(404);
        });

        it("should return 404 for unknown object type (Line 214)", async () => {
            const payload = { object: "unknown_obj", entry: [] };
            const bodyStr = JSON.stringify(payload);
            const hash = crypto.createHmac("sha256", "test_secret").update(bodyStr).digest("hex");
            const res = await simulateRequest({
                hostname: "127.0.0.1", port: currentPort, path: "/webhook", method: "POST",
                headers: { "X-Hub-Signature-256": `sha256=${hash}` }
            }, bodyStr);
            expect(res.status).toBe(404);
        });

        it("should return 500 on payload processing error (Line 218)", async () => {
            const bodyStr = "INVALID_JSON";
            const hash = crypto.createHmac("sha256", "test_secret").update(bodyStr).digest("hex");
            const res = await simulateRequest({
                hostname: "127.0.0.1", port: currentPort, path: "/webhook", method: "POST",
                headers: { "X-Hub-Signature-256": `sha256=${hash}` }
            }, bodyStr);
            expect(res.status).toBe(500);
        });
    

        it('should accept POST with postback and emit postback event', async () => {
            const payload = { object: 'page', entry: [{ messaging: [{ sender: { id: 'sender123' }, postback: { payload: 'BTN_PAYLOAD' }, timestamp: 1000 }] }] };
            const bodyStr = JSON.stringify(payload);
            const hash = crypto.createHmac('sha256', 'test_secret').update(bodyStr).digest('hex');
            const emitSpy = vi.fn();
            bridge.on('postback', emitSpy);
            const res = await simulateRequest({ hostname: '127.0.0.1', port: currentPort, path: '/webhook', method: 'POST', headers: { 'X-Hub-Signature-256': 'sha256=' + hash } }, bodyStr);
            expect(res.status).toBe(200);
            expect(emitSpy).toHaveBeenCalled();
        });
    
        it('should gracefully ignore events that are neither message nor postback', async () => {
            const payload = { object: 'page', entry: [{ messaging: [{ sender: { id: 'sender123' }, read: { watermark: 123 }, timestamp: 1000 }] }] };
            const bodyStr = JSON.stringify(payload);
            const hash = crypto.createHmac('sha256', 'test_secret').update(bodyStr).digest('hex');
            const res = await simulateRequest({ hostname: '127.0.0.1', port: currentPort, path: '/webhook', method: 'POST', headers: { 'X-Hub-Signature-256': 'sha256=' + hash } }, bodyStr);
            expect(res.status).toBe(200);
        });

        it('should gracefully ignore postback without payload', async () => {
            const payload = { object: 'page', entry: [{ messaging: [{ sender: { id: 'sender123' }, postback: { title: 'No Payload' }, timestamp: 1000 }] }] };
            const bodyStr = JSON.stringify(payload);
            const hash = crypto.createHmac('sha256', 'test_secret').update(bodyStr).digest('hex');
            const res = await simulateRequest({ hostname: '127.0.0.1', port: currentPort, path: '/webhook', method: 'POST', headers: { 'X-Hub-Signature-256': 'sha256=' + hash } }, bodyStr);
            expect(res.status).toBe(200);
        });
});
});
