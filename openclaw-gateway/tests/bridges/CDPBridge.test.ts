/**
 * CDPBridge.test.ts — Chrome DevTools Protocol Bridge Tests
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { EventEmitter } from "node:events";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

// Mock safeFetch
const mockSafeFetch = vi.fn();
vi.mock("../../src/utils/HttpClient", () => ({
    safeFetch: (...args: any[]) => mockSafeFetch(...args),
}));

// Mock WebSocket class
const { MockWebSocket, MockWebSocketContext } = vi.hoisted(() => {
    const { EventEmitter } = require("node:events");
    const MockWebSocketContext = { lastInstance: null as any };
    class MockWebSocket extends EventEmitter {
        static OPEN = 1;
        readyState = 1; // WebSocket.OPEN
        send = vi.fn();
        close = vi.fn();
        constructor(public url: string) {
            super();
            MockWebSocketContext.lastInstance = this;
            if (!url.includes("_err") && !url.includes("_timeout")) {
                queueMicrotask(() => this.emit("open"));
            }
        }
    }
    return { MockWebSocket, MockWebSocketContext };
});

vi.mock("ws", () => ({
    WebSocket: MockWebSocket,
}));

import { CDPBridge } from "../../src/bridges/CDPBridge";

describe("CDPBridge", () => {
    let bridge: CDPBridge;

    beforeEach(() => {
        vi.clearAllMocks();
        vi.useFakeTimers();
        bridge = new CDPBridge("127.0.0.1", 9222);
    });

    afterEach(() => {
        bridge.dispose();
        vi.useRealTimers();
    });

    describe("Lifecycle", () => {
        it("should connect to the first page target and emit connected", async () => {
            mockSafeFetch.mockResolvedValueOnce({
                json: () => Promise.resolve([
                    { type: "background_page", webSocketDebuggerUrl: "ws://bg" },
                    { type: "page", webSocketDebuggerUrl: "ws://page", title: "Test Page" }
                ])
            });

            const connectedHandler = vi.fn();
            bridge.on("connected", connectedHandler);

            await bridge.connect();

            expect(bridge.isConnected()).toBe(true);
            expect(connectedHandler).toHaveBeenCalledTimes(1);
            expect(connectedHandler.mock.calls[0][0].title).toBe("Test Page");
        });

        it("should throw if no debuggable page found", async () => {
            mockSafeFetch.mockResolvedValueOnce({
                json: () => Promise.resolve([
                    { type: "background_page", webSocketDebuggerUrl: "ws://bg" }
                ])
            });

            await expect(bridge.connect()).rejects.toThrow("No debuggable page found");
        });

        it("should schedule reconnect on fetch failure", async () => {
            mockSafeFetch.mockRejectedValueOnce(new Error("Connection Refused"));

            await expect(bridge.connect()).rejects.toThrow("Connection Refused");

            // Fast forward reconnect timer
            vi.runAllTimers();
            await Promise.resolve();
            // Should fetch again
            expect(mockSafeFetch).toHaveBeenCalledTimes(2);
        });
        
        it("should auto-reconnect on WebSocket close", async () => {
            mockSafeFetch.mockResolvedValueOnce({
                json: () => Promise.resolve([{ type: "page", webSocketDebuggerUrl: "ws://page" }])
            });

            await bridge.connect();

            const ws = MockWebSocketContext.lastInstance;
            
            // Allow next connect fetch to succeed
            mockSafeFetch.mockResolvedValueOnce({
                json: () => Promise.resolve([{ type: "page", webSocketDebuggerUrl: "ws://page" }])
            });
            
            ws.emit("close"); // Simulate drop
            expect(bridge.isConnected()).toBe(false);

            // Fast forward reconnect timer
            vi.runAllTimers();
            // Wait for all the async steps in connect() to complete
            for (let i = 0; i < 5; i++) {
                await Promise.resolve();
            }
            
            expect(bridge.isConnected()).toBe(true);
        });

        it("should not connect if disposed", async () => {
            bridge.dispose();
            await bridge.connect();
            expect(mockSafeFetch).not.toHaveBeenCalled();
        });
    });

    describe("Commands", () => {
        beforeEach(async () => {
            mockSafeFetch.mockResolvedValueOnce({
                json: () => Promise.resolve([{ type: "page", webSocketDebuggerUrl: "ws://page" }])
            });
            await bridge.connect();
        });

        it("should send command and wait for response", async () => {
            const ws = MockWebSocketContext.lastInstance;
            const sendPromise = bridge.send("Page.enable");

            expect(ws.send).toHaveBeenCalledTimes(1);
            const sentPayload = JSON.parse(ws.send.mock.calls[0][0]);
            
            ws.emit("message", JSON.stringify({
                id: sentPayload.id,
                result: { success: true }
            }));

            const result = await sendPromise;
            expect(result.success).toBe(true);
        });

        it("should reject on CDP error response", async () => {
            const ws = MockWebSocketContext.lastInstance;
            const sendPromise = bridge.send("Page.invalid");

            const sentPayload = JSON.parse(ws.send.mock.calls[0][0]);
            
            ws.emit("message", JSON.stringify({
                id: sentPayload.id,
                error: { message: "Method not found", code: -32601 }
            }));

            await expect(sendPromise).rejects.toThrow("[CDP] Method not found");
        });

        it("should timeout if no response", async () => {
            const sendPromise = bridge.send("Slow.method");

            // Fast forward 10s (default CDP timeout)
            vi.advanceTimersByTime(10000);

            await expect(sendPromise).rejects.toThrow("Timeout: Slow.method (10000ms)");
        });

        it("should reject pending requests if connection closes (Lines 405-406)", async () => {
            const sendPromise = bridge.send("Test.method");
            const ws = MockWebSocketContext.lastInstance;
            
            // Close before responding
            ws.emit("close");
            
            await expect(sendPromise).rejects.toThrow("[CDP] Connection closed");
        });

        it("should reject on send if not connected", async () => {
            vi.spyOn(bridge, 'isConnected').mockReturnValue(false);
            bridge.disconnect();
            await expect(bridge.send("Test")).rejects.toThrow("Not connected");
        });

        it("should ignore invalid JSON in message listener (Line 330)", async () => {
            const ws = MockWebSocketContext.lastInstance;
            const sendPromise = bridge.send("Test.method");
            
            // Emit bad JSON
            ws.emit("message", "{bad json");
            
            // Should not crash, just log error. Then we fulfill it properly.
            const payload = JSON.parse(ws.send.mock.calls[0][0]);
            ws.emit("message", JSON.stringify({
                id: payload.id,
                result: { success: true }
            }));

            await expect(sendPromise).resolves.toEqual({ success: true });
        });

        it("should reject connection if socket errors out (Lines 343-345)", async () => {
            bridge.disconnect();
            
            mockSafeFetch.mockResolvedValueOnce({
                json: () => Promise.resolve([{ type: "page", webSocketDebuggerUrl: "ws://page_err" }])
            });

            const connectPromise = bridge.connect();
            
            let ws;
            while (true) {
                await Promise.resolve();
                ws = MockWebSocketContext.lastInstance;
                if (ws && ws.url === "ws://page_err") break;
            }
            
            ws.emit("error", new Error("Simulated WS error"));
            await expect(connectPromise).rejects.toThrow("Simulated WS error");
        });

        it("should reject connection on timeout (Lines 315-316)", async () => {
            bridge.disconnect();
            
            mockSafeFetch.mockResolvedValueOnce({
                json: () => Promise.resolve([{ type: "page", webSocketDebuggerUrl: "ws://page_timeout" }])
            });

            const connectPromise = bridge.connect();
            
            // Wait until the new WebSocket is created
            let ws;
            while (true) {
                await Promise.resolve();
                ws = MockWebSocketContext.lastInstance;
                if (ws && ws.url === "ws://page_timeout") break;
            }
            
            // Advance by 10s
            vi.advanceTimersByTime(10000);
            
            await expect(connectPromise).rejects.toThrow("[CDP] WebSocket connection timeout");
        });
    });

    describe("Helper Methods", () => {
        beforeEach(async () => {
            mockSafeFetch.mockResolvedValueOnce({
                json: () => Promise.resolve([{ type: "page", webSocketDebuggerUrl: "ws://page" }])
            });
            await bridge.connect();
        });

        it("evaluateJS should extract value", async () => {
            const ws = MockWebSocketContext.lastInstance;
            const p = bridge.evaluateJS("1 + 1");
            
            const payload = JSON.parse(ws.send.mock.calls[0][0]);
            ws.emit("message", JSON.stringify({
                id: payload.id,
                result: { result: { value: 2 } }
            }));

            expect(await p).toBe(2);
        });

        it("evaluateJS should throw on JS exception", async () => {
            const ws = MockWebSocketContext.lastInstance;
            const p = bridge.evaluateJS("throw new Error('x')");
            
            const payload = JSON.parse(ws.send.mock.calls[0][0]);
            ws.emit("message", JSON.stringify({
                id: payload.id,
                result: { exceptionDetails: { text: "Uncaught Error: x" } }
            }));

            await expect(p).rejects.toThrow("JS Error: Uncaught Error: x");
        });

        it("querySelector should return nodeId", async () => {
            const ws = MockWebSocketContext.lastInstance;
            const p = bridge.querySelector("button");
            
            // DOM.getDocument
            let payload = JSON.parse(ws.send.mock.calls[0][0]);
            ws.emit("message", JSON.stringify({ id: payload.id, result: { root: { nodeId: 1 } } }));
            
            while(ws.send.mock.calls.length < 2) await Promise.resolve();
            
            // DOM.querySelector
            payload = JSON.parse(ws.send.mock.calls[1][0]);
            ws.emit("message", JSON.stringify({ id: payload.id, result: { nodeId: 42 } }));

            expect(await p).toBe(42);
        });

        it("querySelector should return null if nodeId is 0", async () => {
            const ws = MockWebSocketContext.lastInstance;
            const p = bridge.querySelector("missing_element");
            
            let payload = JSON.parse(ws.send.mock.calls[0][0]);
            ws.emit("message", JSON.stringify({ id: payload.id, result: { root: { nodeId: 1 } } }));
            
            while(ws.send.mock.calls.length < 2) await Promise.resolve();
            
            payload = JSON.parse(ws.send.mock.calls[1][0]);
            ws.emit("message", JSON.stringify({ id: payload.id, result: { nodeId: 0 } })); // 0 means not found in CDP

            expect(await p).toBeNull();
        });

        it("querySelector should return null on error (Line 172)", async () => {
            const ws = MockWebSocketContext.lastInstance;
            const p = bridge.querySelector("error_element");
            
            // DOM.getDocument succeeds
            let payload = JSON.parse(ws.send.mock.calls[0][0]);
            ws.emit("message", JSON.stringify({ id: payload.id, result: { root: { nodeId: 1 } } }));
            
            while(ws.send.mock.calls.length < 2) await Promise.resolve();
            
            // DOM.querySelector fails
            payload = JSON.parse(ws.send.mock.calls[1][0]);
            ws.emit("message", JSON.stringify({
                id: payload.id,
                error: { message: "Internal CDP Error" }
            }));

            expect(await p).toBeNull();
        });

        it("captureScreenshot should parse base64", async () => {
            const ws = MockWebSocketContext.lastInstance;
            const p = bridge.captureScreenshot();
            
            const payload = JSON.parse(ws.send.mock.calls[0][0]);
            ws.emit("message", JSON.stringify({
                id: payload.id,
                result: { data: "R0lGODlhAQABAIAAAAUEBAAAACwAAAAAAQABAAACAkQBADs=" } // 1x1 gif base64
            }));

            const buf = await p;
            expect(buf).toBeInstanceOf(Buffer);
            expect(buf.length).toBeGreaterThan(0);
        });

        it("watchForApprovalButtons should handle __LIVA_APPROVAL_DETECTED__ events", async () => {
            const ws = MockWebSocketContext.lastInstance;
            
            const p = bridge.watchForApprovalButtons();
            
            // Respond to enable
            let payload = JSON.parse(ws.send.mock.calls[0][0]);
            ws.emit("message", JSON.stringify({ id: payload.id }));
            
            while(ws.send.mock.calls.length < 2) await Promise.resolve();
            
            // Respond to evaluateJS
            payload = JSON.parse(ws.send.mock.calls[1][0]);
            ws.emit("message", JSON.stringify({ id: payload.id, result: {} }));
            
            while(ws.send.mock.calls.length < 3) await Promise.resolve();
            
            // Respond to second enable
            payload = JSON.parse(ws.send.mock.calls[2][0]);
            ws.emit("message", JSON.stringify({ id: payload.id }));
            await p;

            // Now simulate the event
            const approvalHandler = vi.fn();
            bridge.on("approval_required", approvalHandler);

            ws.emit("message", JSON.stringify({
                method: "Runtime.consoleAPICalled",
                params: {
                    args: [{ value: "__LIVA_APPROVAL_DETECTED__:{\"text\":\"Approve\"}" }]
                }
            }));

            expect(approvalHandler).toHaveBeenCalledTimes(1);
            expect(approvalHandler.mock.calls[0][0].text).toBe("Approve");
        });

        it("getPageTitle should evaluate document.title", async () => {
            const ws = MockWebSocketContext.lastInstance;
            const p = bridge.getPageTitle();
            const payload = JSON.parse(ws.send.mock.calls[0][0]);
            ws.emit("message", JSON.stringify({
                id: payload.id,
                result: { result: { value: "Test Title" } }
            }));
            expect(await p).toBe("Test Title");
        });

        it("typeText should dispatch key events", async () => {
            const ws = MockWebSocketContext.lastInstance;
            const p = bridge.typeText("Hi");
            
            for (let i = 0; i < 4; i++) {
                while(ws.send.mock.calls.length < i + 1) await Promise.resolve();
                const payload = JSON.parse(ws.send.mock.calls[i][0]);
                ws.emit("message", JSON.stringify({ id: payload.id }));
            }
            
            await expect(p).resolves.toBeUndefined();
            expect(ws.send.mock.calls[0][0]).toContain('"type":"keyDown","text":"H"');
            expect(ws.send.mock.calls[1][0]).toContain('"type":"keyUp","text":"H"');
        });

        it("clickElement should evaluate position and dispatch mouse events", async () => {
            const ws = MockWebSocketContext.lastInstance;
            const p = bridge.clickElement("btn");
            
            while(ws.send.mock.calls.length < 1) await Promise.resolve();
            const payload = JSON.parse(ws.send.mock.calls[0][0]);
            ws.emit("message", JSON.stringify({
                id: payload.id,
                result: { result: { value: { x: 10, y: 20 } } }
            }));
            
            for (let i = 1; i < 3; i++) {
                while(ws.send.mock.calls.length < i + 1) await Promise.resolve();
                const p2 = JSON.parse(ws.send.mock.calls[i][0]);
                ws.emit("message", JSON.stringify({ id: p2.id }));
            }
            
            await expect(p).resolves.toBeUndefined();
            expect(ws.send.mock.calls[1][0]).toContain('"type":"mousePressed"');
            expect(ws.send.mock.calls[2][0]).toContain('"type":"mouseReleased"');
        });

        it("clickElement should throw if element not found", async () => {
            const ws = MockWebSocketContext.lastInstance;
            const p = bridge.clickElement("missing");
            
            const payload = JSON.parse(ws.send.mock.calls[0][0]);
            ws.emit("message", JSON.stringify({
                id: payload.id,
                result: { result: { value: null } }
            }));
            
            await expect(p).rejects.toThrow("Element not found");
        });

        it("clickApprovalButton should evaluate button click script", async () => {
            const ws = MockWebSocketContext.lastInstance;
            const p = bridge.clickApprovalButton(true);
            
            const payload = JSON.parse(ws.send.mock.calls[0][0]);
            ws.emit("message", JSON.stringify({
                id: payload.id,
                result: { result: { value: true } }
            }));
            
            await expect(p).resolves.toBeUndefined();
        });
    });
});
