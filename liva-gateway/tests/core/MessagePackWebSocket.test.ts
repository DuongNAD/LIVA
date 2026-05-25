import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { pack, unpack } from "msgpackr";
import { UIController } from "../../src/core/UIController";

// Mock dependencies
vi.mock("ws", () => {
  class MockWebSocketServer {
    handlers: Record<string, Function[]> = {};
    constructor(_opts?: any) {}
    on(event: string, handler: Function) {
      if (!this.handlers[event]) this.handlers[event] = [];
      this.handlers[event].push(handler);
    }
    emit(event: string, ...args: any[]) {
      (this.handlers[event] || []).forEach(h => h(...args));
    }
  }

  return {
    WebSocketServer: MockWebSocketServer,
    WebSocket: { OPEN: 1, CLOSED: 3 },
  };
});

vi.mock("fs", () => ({
  promises: {
    readFile: vi.fn(),
    writeFile: vi.fn().mockResolvedValue(undefined),
    rename: vi.fn().mockResolvedValue(undefined),
  },
}));

vi.mock("../../src/utils/logger", () => ({
  logger: {
    info: vi.fn(),
    debug: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  },
}));

function createMockWS(readyState = 1): any {
  const handlers: Record<string, Function[]> = {};
  return {
    readyState,
    send: vi.fn(),
    on(event: string, handler: Function) {
      if (!handlers[event]) handlers[event] = [];
      handlers[event].push(handler);
    },
    emit(event: string, ...args: any[]) {
      (handlers[event] || []).forEach(h => h(...args));
    },
    _handlers: handlers,
  };
}

describe("MessagePack WebSocket Binary Protocol", () => {
  let originalArgv: string[];
  
  beforeEach(() => {
    originalArgv = [...process.argv];
    if (!process.argv.includes("--dev")) {
      process.argv.push("--dev");
    }
  });

  afterEach(() => {
    process.argv = originalArgv;
  });

  it("should wrap UIController output in MsgPack event header 0x02", () => {
    const ctrl = new UIController(0);
    const ws = createMockWS();
    
    // Simulate connection
    const wss = (ctrl as any).wss;
    wss.emit("connection", ws, { url: "/?token=test" });
    
    ctrl.broadcastUIEvent("test_msgpack_event", { hello: "world" });
    
    expect(ws.send).toHaveBeenCalledOnce();
    const sent = ws.send.mock.calls[0][0] as Uint8Array;
    expect(sent[0]).toBe(0x02); // MessagePack event header
    
    const decoded = unpack(sent.subarray(1));
    expect(decoded).toEqual({
      event: "test_msgpack_event",
      payload: { hello: "world" }
    });
  });

  it("should wrap UIController audio output in raw header 0x01", () => {
    const ctrl = new UIController(0);
    const ws = createMockWS();
    
    // Simulate connection
    const wss = (ctrl as any).wss;
    wss.emit("connection", ws, { url: "/?token=test" });
    
    const audioData = Buffer.from([1, 2, 3, 4, 5]);
    ctrl.broadcastAudioChunk(audioData);
    
    expect(ws.send).toHaveBeenCalledOnce();
    const sent = ws.send.mock.calls[0][0] as Uint8Array;
    expect(sent[0]).toBe(0x01); // Audio header
    expect(Buffer.from(sent.subarray(1))).toEqual(audioData);
  });

  it("should parse incoming binary MessagePack events on 0x02 header", async () => {
    const ctrl = new UIController(0);
    const ws = createMockWS();
    
    const wss = (ctrl as any).wss;
    wss.emit("connection", ws, { url: "/?token=test" });
    
    const userVoiceCommandSpy = vi.fn();
    ctrl.on("user_input", userVoiceCommandSpy);
    
    // Construct incoming binary msgpack message
    const payload = pack({
      event: "user_voice_command",
      payload: { text: "hello antigravity" }
    });
    const message = new Uint8Array(1 + payload.byteLength);
    message[0] = 0x02; // MessagePack event
    message.set(new Uint8Array(payload), 1);
    
    // Emit message to ws
    ws.emit("message", Buffer.from(message), true); // isBinary = true
    
    expect(userVoiceCommandSpy).toHaveBeenCalledWith("hello antigravity");
  });

  it("should parse incoming raw audio chunks on 0x01 header", async () => {
    const ctrl = new UIController(0);
    const ws = createMockWS();
    
    const wss = (ctrl as any).wss;
    wss.emit("connection", ws, { url: "/?token=test" });
    
    const audioSpy = vi.fn();
    ctrl.on("audio_input", audioSpy);
    
    const rawAudio = Buffer.from([10, 20, 30, 40]);
    const message = new Uint8Array(1 + rawAudio.length);
    message[0] = 0x01; // Audio header
    message.set(rawAudio, 1);
    
    ws.emit("message", Buffer.from(message), true);
    
    expect(audioSpy).toHaveBeenCalledWith(rawAudio);
  });

  it("should handle malformed binary packages gracefully without throwing", async () => {
    const ctrl = new UIController(0);
    const ws = createMockWS();
    
    const wss = (ctrl as any).wss;
    wss.emit("connection", ws, { url: "/?token=test" });
    
    const message = new Uint8Array([0x02, 0xff, 0xff, 0xff]); // malformed MsgPack
    
    // This should not throw fatal exceptions
    expect(() => {
      ws.emit("message", Buffer.from(message), true);
    }).not.toThrow();
  });
});
