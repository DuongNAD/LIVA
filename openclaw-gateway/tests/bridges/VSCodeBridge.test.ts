/**
 * VSCodeBridge.test.ts — VS Code Remote Control Bridge Tests
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { EventEmitter } from "node:events";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
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
            // Emit open immediately to prevent promise deadlocks with FakeTimers
            queueMicrotask(() => this.emit("open"));
        }
    }
    return { MockWebSocket, MockWebSocketContext };
});

vi.mock("ws", () => ({
    WebSocket: MockWebSocket,
}));

import { VSCodeBridge } from "../../src/bridges/VSCodeBridge";

describe("VSCodeBridge", () => {
    let bridge: VSCodeBridge;

    beforeEach(() => {
        vi.clearAllMocks();
        vi.useFakeTimers();
        bridge = new VSCodeBridge("127.0.0.1", 3710);
    });

    afterEach(() => {
        bridge.dispose();
        vi.useRealTimers();
    });

    describe("Lifecycle", () => {
        it("should connect to WebSocket and emit connected", async () => {
            const connectedHandler = vi.fn();
            bridge.on("connected", connectedHandler);

            await bridge.connect();

            expect(bridge.isConnected()).toBe(true);
            expect(connectedHandler).toHaveBeenCalledTimes(1);
        });

        it("should not reconnect if already connected", async () => {
            await bridge.connect();
            const wsInstance1 = MockWebSocketContext.lastInstance;

            await bridge.connect(); // Should be a no-op
            const wsInstance2 = MockWebSocketContext.lastInstance;

            expect(wsInstance1).toBe(wsInstance2);
        });

        it("should handle connection errors", async () => {
            // Force the next WebSocket instance to emit error instead of open
            const origEmit = MockWebSocket.prototype.emit;
            MockWebSocket.prototype.emit = function(event: string, ...args: any[]) {
                if (event === "open") {
                    queueMicrotask(() => origEmit.call(this, "error", new Error("Connection failed")));
                    return false;
                }
                return origEmit.call(this, event, ...args);
            };

            await expect(bridge.connect()).rejects.toThrow("Connection failed");
            expect(bridge.isConnected()).toBe(false);
            
            // Restore
            MockWebSocket.prototype.emit = origEmit;
        });

        it("should auto-reconnect on disconnect", async () => {
            await bridge.connect();
            
            const disconnectedHandler = vi.fn();
            bridge.on("disconnected", disconnectedHandler);

            const ws = MockWebSocketContext.lastInstance;
            ws.emit("close"); // Simulate drop

            expect(disconnectedHandler).toHaveBeenCalledTimes(1);
            expect(bridge.isConnected()).toBe(false);

            // Fast forward through backoff
            vi.runAllTimers();
            await Promise.resolve(); // allow microtasks for reconnect

            // Should have initiated new connection
            expect(bridge.isConnected()).toBe(true);
        });

        it("should NOT auto-reconnect if disposed", async () => {
            await bridge.connect();
            
            const ws = MockWebSocketContext.lastInstance;
            
            bridge.dispose(); // User intentionally closed
            ws.emit("close"); // Emitted as side effect of closing

            vi.runAllTimers();
            await Promise.resolve();

            expect(bridge.isConnected()).toBe(false);
        });

        it("should clear reconnect timer on dispose (Lines 234-235)", async () => {
            await bridge.connect();
            const ws = MockWebSocketContext.lastInstance;
            ws.emit("close"); // Triggers auto-reconnect timer (this.#reconnectTimer is now set)
            
            bridge.dispose(); // Should clear the timer
            
            vi.runAllTimers();
            expect(bridge.isConnected()).toBe(false);
        });
    });

    describe("Commands", () => {
        beforeEach(async () => {
            await bridge.connect();
        });

        it("should send command and resolve when response received", async () => {
            const ws = MockWebSocketContext.lastInstance;
            const sendPromise = bridge.executeCommand("workbench.action.files.save");

            expect(ws.send).toHaveBeenCalledTimes(1);
            const sentStr = ws.send.mock.calls[0][0];
            const sentPayload = JSON.parse(sentStr);

            expect(sentPayload.command).toBe("executeCommand");
            expect(sentPayload.args[0]).toBe("workbench.action.files.save");
            expect(typeof sentPayload.id).toBe("number");

            // Simulate response
            ws.emit("message", JSON.stringify({
                id: sentPayload.id,
                result: "Success"
            }));

            const result = await sendPromise;
            expect(result).toBe("Success");
        });

        it("should reject if response contains an error", async () => {
            const ws = MockWebSocketContext.lastInstance;
            const sendPromise = bridge.executeCommand("invalid.command");

            const sentPayload = JSON.parse(ws.send.mock.calls[0][0]);

            // Simulate error response
            ws.emit("message", JSON.stringify({
                id: sentPayload.id,
                error: "Command not found"
            }));

            await expect(sendPromise).rejects.toThrow("IDE Error: Command not found");
        });

        it("should timeout if no response received", async () => {
            const sendPromise = bridge.executeCommand("slow.command");

            // Fast forward 15s (default timeout)
            vi.advanceTimersByTime(15000);

            await expect(sendPromise).rejects.toThrow("Timeout executing executeCommand (15000ms)");
        });

        it("should parse message safely", () => {
            const ws = MockWebSocketContext.lastInstance;
            // Should not crash on invalid JSON
            ws.emit("message", "{invalid_json:");
            // Since it's synchronous and only logs, we just verify it didn't crash
        });

        it("should map convenience methods correctly", async () => {
            const ws = MockWebSocketContext.lastInstance;

            const p1 = bridge.openFile("test.ts");
            const payload1 = JSON.parse(ws.send.mock.calls[0][0]);
            expect(payload1.command).toBe("openFile");
            ws.emit("message", JSON.stringify({ id: payload1.id }));
            await p1;

            ws.send.mockClear();
            const p2 = bridge.insertText("Hello");
            const payload2 = JSON.parse(ws.send.mock.calls[0][0]);
            expect(payload2.command).toBe("insertText");
            ws.emit("message", JSON.stringify({ id: payload2.id }));
            await p2;

            ws.send.mockClear();
            const p3 = bridge.getActiveEditor();
            const payload3 = JSON.parse(ws.send.mock.calls[0][0]);
            expect(payload3.command).toBe("getActiveEditor");
            ws.emit("message", JSON.stringify({ id: payload3.id, result: { fileName: "test.ts" } }));
            const editor = await p3;
            expect(editor?.fileName).toBe("test.ts");

            ws.send.mockClear();
            const p4 = bridge.openTerminal();
            const payload4 = JSON.parse(ws.send.mock.calls[0][0]);
            expect(payload4.command).toBe("executeCommand");
            expect(payload4.args[0]).toBe("workbench.action.terminal.toggleTerminal");
            ws.emit("message", JSON.stringify({ id: payload4.id }));
            await p4;

            ws.send.mockClear();
            const p5 = bridge.runTerminalCommand("ls -la");
            const payload5 = JSON.parse(ws.send.mock.calls[0][0]);
            expect(payload5.command).toBe("runTerminalCommand");
            expect(payload5.args[0]).toBe("ls -la");
            ws.emit("message", JSON.stringify({ id: payload5.id }));
            await p5;
        });
    });

    describe("Error Handling", () => {
        it("should reject commands if not connected", async () => {
            await expect(bridge.executeCommand("test")).rejects.toThrow("Not connected to VS Code IDE");
        });

        it("should reject pending requests on dispose (Lines 240-241)", async () => {
            await bridge.connect();
            const sendPromise = bridge.executeCommand("test_command");
            // Dispose while request is pending
            bridge.dispose();
            await expect(sendPromise).rejects.toThrow("[VSCode] Connection closed");
        });
    });
});
