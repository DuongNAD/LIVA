import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { CDPClient } from "../../src/utils/CDPClient";

const { MockWebSocket } = vi.hoisted(() => {
    class MockWebSocket {
        static OPEN = 1;
        readyState = 1;
        send = vi.fn();
        close = vi.fn();
        handlers: Record<string, Function[]> = {};
        url: string;
        didError = false;

        constructor(url: string) {
            this.url = url;
            MockWebSocket.instances.push(this);
            // Simulate connection delay
            if (url !== "ws://timeout") {
                setTimeout(() => {
                    if (!this.didError) this.emit("open");
                }, 1);
            }
        }

        on(event: string, handler: Function) {
            if (!this.handlers[event]) this.handlers[event] = [];
            this.handlers[event].push(handler);
        }

        emit(event: string, ...args: any[]) {
            if (event === "error" || event === "close") {
                this.didError = true;
            }
            if (this.handlers[event]) {
                this.handlers[event].forEach(h => h(...args));
            }
        }
        
        static instances: MockWebSocket[] = [];
        static clear() { this.instances = []; }
    }
    return { MockWebSocket };
});

vi.mock("ws", () => ({ WebSocket: MockWebSocket }));

describe("CDPClient", () => {
    let cdp: CDPClient;

    beforeEach(() => {
        vi.clearAllMocks();
        MockWebSocket.clear();
        cdp = new CDPClient();
    });

    afterEach(() => {
        cdp.dispose();
    });

    it("should connect, set status to connected, and ignore second connect", async () => {
        await cdp.connect("ws://127.0.0.1:9222/devtools/browser/xyz");
        expect(cdp.isConnected).toBe(true);

        const instancesBefore = MockWebSocket.instances.length;
        await cdp.connect("ws://127.0.0.1:9222/devtools/browser/xyz");
        expect(MockWebSocket.instances.length).toBe(instancesBefore);
    });

    it("should reject connect on timeout or error", async () => {
        const promise = cdp.connect("ws://timeout");
        MockWebSocket.instances[0].emit("error", new Error("conn err"));
        await expect(promise).rejects.toThrow(/conn err/);
    });

    it("should reconnect automatically on close", async () => {
        await cdp.connect("ws://mock");
        const instance1 = MockWebSocket.instances[0];

        // close event triggers reconnect
        instance1.emit("close", 1006, Buffer.from("closed"));
        expect(cdp.isConnected).toBe(false);
        
        // Fix open handle deadlock: dispose the client immediately so the scheduled reconnect is cancelled
        cdp.dispose();
    });

    it("should block dangerous CDP domains", async () => {
        await cdp.connect("ws://mock");

        await expect(cdp.send("Security.setIgnoreCertificateErrors", { ignore: true }))
            .rejects.toThrow(/SECURITY BLOCK/);

        await expect(cdp.send("Browser.close"))
            .rejects.toThrow(/SECURITY BLOCK/);
    });

    it("should send command and receive response", async () => {
        await cdp.connect("ws://mock");
        const instance = MockWebSocket.instances[0];

        const promise = cdp.send("Page.enable");
        
        instance.emit("message", Buffer.from(JSON.stringify({
            id: 1,
            result: { success: true }
        })));

        const res = await promise;
        expect(res).toEqual({ success: true });
        expect(instance.send).toHaveBeenCalledWith(JSON.stringify({ id: 1, method: "Page.enable", params: {} }));
    });

    it("should handle error response", async () => {
        await cdp.connect("ws://mock");
        const instance = MockWebSocket.instances[0];

        const promise = cdp.send("Page.navigate", { url: "bad" });
        instance.emit("message", Buffer.from(JSON.stringify({
            id: 1,
            error: { message: "Navigation failed" }
        })));

        await expect(promise).rejects.toThrow(/Navigation failed/);
    });

    it("should format command with sessionId if attached", async () => {
        await cdp.connect("ws://mock");
        const instance = MockWebSocket.instances[0];
        
        const attachPromise = cdp.attachToTarget("target-123");
        instance.emit("message", Buffer.from(JSON.stringify({
            id: 1,
            result: { sessionId: "session-456" }
        })));
        await attachPromise;

        const sendPromise = cdp.send("Page.enable");
        instance.emit("message", Buffer.from(JSON.stringify({
            id: 2,
            result: {}
        })));
        await sendPromise;

        expect(instance.send).toHaveBeenCalledWith(JSON.stringify({
            id: 2, method: "Page.enable", params: {}, sessionId: "session-456"
        }));
    });

    it("should handle navigateTo", async () => {
        await cdp.connect("ws://mock");
        const instance = MockWebSocket.instances[0];

        const navPromise = cdp.navigateTo("https://example.com");

        instance.emit("message", Buffer.from(JSON.stringify({
            id: 1,
            result: { frameId: "frame-1", loaderId: "loader-1" }
        })));

        await new Promise(r => setTimeout(r, 0));

        instance.emit("message", Buffer.from(JSON.stringify({
            method: "Page.loadEventFired",
            params: {}
        })));

        const res = await navPromise;
        expect(res.frameId).toBe("frame-1");
    });

    it("should handle navigateTo errorText", async () => {
        await cdp.connect("ws://mock");
        const instance = MockWebSocket.instances[0];

        const navPromise = cdp.navigateTo("https://example.com");

        instance.emit("message", Buffer.from(JSON.stringify({
            id: 1,
            result: { errorText: "net::ERR_NAME_NOT_RESOLVED" }
        })));

        await expect(navPromise).rejects.toThrow(/net::ERR_NAME_NOT_RESOLVED/);
    });

    it("should clean up resources on dispose", async () => {
        await cdp.connect("ws://mock");
        const instance = MockWebSocket.instances[0];
        
        cdp.dispose();
        expect(cdp.isConnected).toBe(false);
        expect(instance.close).toHaveBeenCalled();

        await expect(cdp.connect("ws://mock")).rejects.toThrow(/disposed/);
    });

    it("should handle evaluate", async () => {
        await cdp.connect("ws://mock");
        const instance = MockWebSocket.instances[0];
        
        const evalPromise = cdp.evaluate("document.title");
        instance.emit("message", Buffer.from(JSON.stringify({
            id: 1,
            result: { result: { value: "My Title" } }
        })));

        const res = await evalPromise;
        expect(res).toBe("My Title");
    });

    it("should provide convenience methods", async () => {
        await cdp.connect("ws://mock");

        const sendSpy = vi.spyOn(cdp, "send").mockResolvedValue({ data: "base64", result: { value: "ok" } });

        await cdp.enableDomains();
        expect(sendSpy).toHaveBeenCalledWith("Page.enable");
        
        await cdp.getAccessibilityTree();
        expect(sendSpy).toHaveBeenCalledWith("Accessibility.getFullAXTree", expect.any(Object), 15000);
        
        await cdp.dispatchClick(100, 200);
        expect(sendSpy).toHaveBeenCalledWith("Input.dispatchMouseEvent", expect.objectContaining({ type: "mousePressed", x: 100 }));
        
        await cdp.dispatchType("a");
        expect(sendSpy).toHaveBeenCalledWith("Input.dispatchKeyEvent", expect.objectContaining({ type: "keyDown", text: "a" }));
        
        await cdp.insertText("hello");
        expect(sendSpy).toHaveBeenCalledWith("Input.insertText", { text: "hello" });
        
        await cdp.screenshot("jpeg", 80);
        expect(sendSpy).toHaveBeenCalledWith("Page.captureScreenshot", { format: "jpeg", quality: 80 }, 10000);
        
        await cdp.scrollPage(100);
        expect(sendSpy).toHaveBeenCalledWith("Input.dispatchMouseEvent", expect.objectContaining({ type: "mouseWheel", deltaY: 100 }));

        vi.spyOn(cdp, "evaluate").mockResolvedValue("title");
        expect(await cdp.getCurrentUrl()).toBe("title");
        expect(await cdp.getPageTitle()).toBe("title");
    });

    it("should handle reconnect logic, enableDomains on reconnect, and max attempts", async () => {
        vi.useFakeTimers();
        
        const connPromise = cdp.connect("ws://mock");
        vi.advanceTimersByTime(10);
        await connPromise;
        
        let instance = MockWebSocket.instances[MockWebSocket.instances.length - 1];

        // Attach to target so it re-enables domains
        const attachPromise = cdp.attachToTarget("targ-1");
        instance.emit("message", Buffer.from(JSON.stringify({ id: 1, result: { sessionId: "sess-1" } })));
        await attachPromise;

        // Force reconnect
        instance.emit("close", 1006, Buffer.from("closed"));
        expect(cdp.isConnected).toBe(false);

        // Fast forward to trigger reconnect timeout
        vi.advanceTimersByTime(1010);
        
        // Connect creates a new MockWebSocket which sets a 1ms timeout.
        vi.advanceTimersByTime(10);
        await new Promise(r => process.nextTick(r)); // Let promise microtasks flush
        
        const newInstance = MockWebSocket.instances[MockWebSocket.instances.length - 1];
        expect(newInstance).not.toBe(instance);
        expect(newInstance.send).toHaveBeenCalled(); // Sent Page.enable, etc.

        // Fail reconnect 5 times
        // The connection is currently successful, so attempts = 0.
        for (let i = 1; i <= 5; i++) {
            // Close the current active (or failing) instance
            const currentInstance = MockWebSocket.instances[MockWebSocket.instances.length - 1];
            currentInstance.emit("close", 1006, Buffer.from("closed"));
            
            // Wait EXACTLY for exponential backoff timeout so connect() is called, but 1ms mock open hasn't fired yet
            vi.advanceTimersByTime(1000 * Math.pow(2, i - 1));
            
            // Now a new MockWebSocket is created. Emit error immediately.
            const failInstance = MockWebSocket.instances[MockWebSocket.instances.length - 1];
            failInstance.emit("error", new Error("conn fail"));
            
            // Advance past the 1ms mock open timer
            vi.advanceTimersByTime(10);
            
            // Allow promises to reject
            await new Promise(r => process.nextTick(r));
        }

        // Now attempts = 5. The next close on the last failing socket should just give up.
        const lastInstance = MockWebSocket.instances[MockWebSocket.instances.length - 1];
        lastInstance.emit("close", 1006, Buffer.from("closed"));
        
        // No new instance should be created.
        const instanceCount = MockWebSocket.instances.length;
        vi.advanceTimersByTime(100000);
        expect(MockWebSocket.instances.length).toBe(instanceCount);

        cdp.dispose();
        vi.useRealTimers();
    });
});
