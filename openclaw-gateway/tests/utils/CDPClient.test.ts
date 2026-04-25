import { describe, it, expect, vi, beforeEach } from "vitest";
import { CDPClient } from "../../src/utils/CDPClient";
// Mock ws
vi.mock("ws", () => {
    return {
        WebSocket: class MockWebSocket {
            static OPEN = 1;
            readyState = 1; // OPEN
            send = vi.fn();
            close = vi.fn();
            
            private handlers: any = {};
            
            on(event: string, handler: any) {
                if (!this.handlers[event]) this.handlers[event] = [];
                this.handlers[event].push(handler);
            }
            
            emit(event: string, ...args: any[]) {
                if (this.handlers[event]) {
                    this.handlers[event].forEach((h: any) => h(...args));
                }
            }

            constructor(url: string) {
                // Auto-open on next tick
                setTimeout(() => this.emit("open"), 10);
            }
        }
    };
});

describe("CDPClient", () => {
    let cdp: CDPClient;

    beforeEach(() => {
        vi.clearAllMocks();
        cdp = new CDPClient();
    });

    it("should connect and set status to connected", async () => {
        await cdp.connect("ws://127.0.0.1:9222/devtools/browser/xyz");
        expect(cdp.isConnected).toBe(true);
    });

    it("should block dangerous CDP domains", async () => {
        await cdp.connect("ws://mock");

        await expect(cdp.send("Security.setIgnoreCertificateErrors", { ignore: true }))
            .rejects.toThrow(/SECURITY BLOCK/);

        await expect(cdp.send("Browser.close"))
            .rejects.toThrow(/SECURITY BLOCK/);
    });

    it("should format command correctly", async () => {
        await cdp.connect("ws://mock");

        // We can't easily intercept the exact JSON sent without exposing ws internally,
        // but we can ensure it doesn't throw synchronously for allowed commands
        const promise = cdp.send("Page.navigate", { url: "http://example.com" }, 100);
        
        // Let it timeout since mock doesn't respond
        await expect(promise).rejects.toThrow(/timeout/);
    });

    it("should clean up resources on dispose", async () => {
        await cdp.connect("ws://mock");
        expect(cdp.isConnected).toBe(true);
        
        cdp.dispose();
        expect(cdp.isConnected).toBe(false);
    });
});
