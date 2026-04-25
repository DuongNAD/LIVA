/**
 * UIController.test.ts — Multi-Client WebSocket + Config SSOT Tests
 * ===================================================================
 * Tests:
 * - Multi-client connection pool (Set<WebSocket>)
 * - Broadcast to all clients
 * - Config SSOT (get_config, update_config, config_updated broadcast)
 * - Ping/pong
 * - Event emission (user_input, interrupt, get_skills_list, get_system_status)
 * - Client disconnect handling (partial + full)
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// ─── Mock dependencies ───
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

import { UIController } from "../../src/core/UIController";
import { promises as fsp } from "node:fs";

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

function getWSSInstance(controller: any): any {
  return controller.wss;
}

function simulateConnection(controller: any, ws: any) {
  const wss = getWSSInstance(controller);
  // Pass a mock req object to prevent TypeError on req.url access
  const mockReq = { url: "/?token=test" };
  wss.emit("connection", ws, mockReq);
}

describe("UIController — Multi-Client Architecture", () => {

  // Force --dev mode so UIController skips WebSocket token authentication
  // (authToken is generated via randomUUID() and inaccessible from tests)
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

  describe("Client Connection Pool", () => {
    it("should add clients to the pool on connection", () => {
      const ctrl = new UIController(0);
      const ws1 = createMockWS();
      const ws2 = createMockWS();

      simulateConnection(ctrl, ws1);
      simulateConnection(ctrl, ws2);

      // Both clients should be in the pool
      expect((ctrl as any).clients.size).toBe(2);
    });

    it("should remove clients on disconnect", () => {
      const ctrl = new UIController(0);
      const ws1 = createMockWS();

      simulateConnection(ctrl, ws1);
      expect((ctrl as any).clients.size).toBe(1);

      // Simulate disconnect
      ws1.emit("close");
      expect((ctrl as any).clients.size).toBe(0);
    });

    it("should keep remaining clients when one disconnects", () => {
      const ctrl = new UIController(0);
      const ws1 = createMockWS();
      const ws2 = createMockWS();

      simulateConnection(ctrl, ws1);
      simulateConnection(ctrl, ws2);
      expect((ctrl as any).clients.size).toBe(2);

      ws1.emit("close");
      expect((ctrl as any).clients.size).toBe(1);
      expect((ctrl as any).clients.has(ws2)).toBe(true);
    });
  });

  describe("Broadcast", () => {
    it("should broadcast UI events to ALL connected clients", () => {
      const ctrl = new UIController(0);
      const ws1 = createMockWS();
      const ws2 = createMockWS();

      simulateConnection(ctrl, ws1);
      simulateConnection(ctrl, ws2);

      ctrl.broadcastUIEvent("ai_thinking_start", { text: "hello" });

      expect(ws1.send).toHaveBeenCalledOnce();
      expect(ws2.send).toHaveBeenCalledOnce();

      const payload1 = JSON.parse(ws1.send.mock.calls[0][0]);
      const payload2 = JSON.parse(ws2.send.mock.calls[0][0]);
      expect(payload1).toEqual({ event: "ai_thinking_start", payload: { text: "hello" } });
      expect(payload2).toEqual(payload1);
    });

    it("should NOT send to clients with closed connections", () => {
      const ctrl = new UIController(0);
      const wsOpen = createMockWS(1);  // OPEN
      const wsClosed = createMockWS(3); // CLOSED

      simulateConnection(ctrl, wsOpen);
      simulateConnection(ctrl, wsClosed);

      ctrl.broadcastUIEvent("test_event", {});

      expect(wsOpen.send).toHaveBeenCalledOnce();
      expect(wsClosed.send).not.toHaveBeenCalled();
    });

    it("should broadcast audio chunks as binary to all clients", () => {
      const ctrl = new UIController(0);
      const ws1 = createMockWS();
      const ws2 = createMockWS();

      simulateConnection(ctrl, ws1);
      simulateConnection(ctrl, ws2);

      const buffer = Buffer.from("test-audio");
      ctrl.broadcastAudioChunk(buffer);

      expect(ws1.send).toHaveBeenCalledWith(buffer, { binary: true });
      expect(ws2.send).toHaveBeenCalledWith(buffer, { binary: true });
    });
  });

  describe("Event Handling", () => {
    it("should emit user_input on user_voice_command", () => {
      const ctrl = new UIController(0);
      const ws = createMockWS();
      const userInputSpy = vi.fn();

      ctrl.on("user_input", userInputSpy);
      simulateConnection(ctrl, ws);

      const msg = JSON.stringify({ event: "user_voice_command", payload: { text: "hello LIVA" } });
      ws.emit("message", Buffer.from(msg), false);

      expect(userInputSpy).toHaveBeenCalledWith("hello LIVA");
    });

    it("should emit interrupt on [INTERRUPT] message", () => {
      const ctrl = new UIController(0);
      const ws = createMockWS();
      const interruptSpy = vi.fn();

      ctrl.on("interrupt", interruptSpy);
      simulateConnection(ctrl, ws);

      ws.emit("message", Buffer.from("[INTERRUPT]"), false);

      expect(interruptSpy).toHaveBeenCalledOnce();
    });

    it("should emit audio_input on binary message", () => {
      const ctrl = new UIController(0);
      const ws = createMockWS();
      const audioSpy = vi.fn();

      ctrl.on("audio_input", audioSpy);
      simulateConnection(ctrl, ws);

      const audioBuffer = Buffer.from("raw-audio-data");
      ws.emit("message", audioBuffer, true); // isBinary = true

      expect(audioSpy).toHaveBeenCalledWith(audioBuffer);
    });

    it("should respond with pong on ping", () => {
      const ctrl = new UIController(0);
      const ws = createMockWS();

      simulateConnection(ctrl, ws);

      const msg = JSON.stringify({ event: "ping" });
      ws.emit("message", Buffer.from(msg), false);

      expect(ws.send).toHaveBeenCalledOnce();
      const response = JSON.parse(ws.send.mock.calls[0][0]);
      expect(response).toEqual({ event: "pong", payload: {} });
    });

    it("should emit get_skills_list with WS reference", () => {
      const ctrl = new UIController(0);
      const ws = createMockWS();
      const skillsSpy = vi.fn();

      ctrl.on("get_skills_list", skillsSpy);
      simulateConnection(ctrl, ws);

      const msg = JSON.stringify({ event: "get_skills_list" });
      ws.emit("message", Buffer.from(msg), false);

      expect(skillsSpy).toHaveBeenCalledWith(ws);
    });

    it("should emit get_system_status with WS reference", () => {
      const ctrl = new UIController(0);
      const ws = createMockWS();
      const statusSpy = vi.fn();

      ctrl.on("get_system_status", statusSpy);
      simulateConnection(ctrl, ws);

      const msg = JSON.stringify({ event: "get_system_status" });
      ws.emit("message", Buffer.from(msg), false);

      expect(statusSpy).toHaveBeenCalledWith(ws);
    });

    it("should emit camera_frame with payload", () => {
      const ctrl = new UIController(0);
      const ws = createMockWS();
      const cameraSpy = vi.fn();

      ctrl.on("camera_frame", cameraSpy);
      simulateConnection(ctrl, ws);

      const payload = { image: "data:image/jpeg;base64,ABC123", timestamp: 12345 };
      const msg = JSON.stringify({ event: "camera_frame", payload });
      ws.emit("message", Buffer.from(msg), false);

      expect(cameraSpy).toHaveBeenCalledWith(payload);
    });
  });

  describe("Config SSOT", () => {
    it("should return config_data from file on get_config", async () => {
      const mockConfig = { avatar: { engineMode: "auto" }, ai: { provider: "local" } };
      (fsp.readFile as any).mockResolvedValue(JSON.stringify(mockConfig));

      const ctrl = new UIController(0);
      const ws = createMockWS();

      simulateConnection(ctrl, ws);

      const msg = JSON.stringify({ event: "get_config" });
      ws.emit("message", Buffer.from(msg), false);

      // Wait for async
      await new Promise(r => setTimeout(r, 50));

      expect(ws.send).toHaveBeenCalledOnce();
      const response = JSON.parse(ws.send.mock.calls[0][0]);
      expect(response.event).toBe("config_data");
      expect(response.payload.avatar.engineMode).toBe("auto");
    });

    it("should return default config when file not found", async () => {
      (fsp.readFile as any).mockRejectedValue(new Error("ENOENT"));

      const ctrl = new UIController(0);
      const ws = createMockWS();

      simulateConnection(ctrl, ws);

      const msg = JSON.stringify({ event: "get_config" });
      ws.emit("message", Buffer.from(msg), false);

      await new Promise(r => setTimeout(r, 50));

      const response = JSON.parse(ws.send.mock.calls[0][0]);
      expect(response.event).toBe("config_data");
      expect(response.payload.avatar).toBeDefined();
      expect(response.payload.ai).toBeDefined();
      expect(response.payload.ui).toBeDefined();
    });

    it("should merge partial update and broadcast config_updated to ALL clients", async () => {
      const existingConfig = {
        avatar: { engineMode: "auto", vrmModel: "default.vrm" },
        ai: { provider: "local", temperature: 0.7 },
        ui: { dashboardTheme: "dark" },
      };
      (fsp.readFile as any).mockResolvedValue(JSON.stringify(existingConfig));
      (fsp.writeFile as any).mockResolvedValue(undefined);

      const ctrl = new UIController(0);
      const ws1 = createMockWS(); // Dashboard (sender)
      const ws2 = createMockWS(); // Widget (receiver)

      simulateConnection(ctrl, ws1);
      simulateConnection(ctrl, ws2);

      // Dashboard sends partial update
      const msg = JSON.stringify({
        event: "update_config",
        payload: { ai: { temperature: 1.0 } },
      });
      ws1.emit("message", Buffer.from(msg), false);

      await new Promise(r => setTimeout(r, 50));

      // File should be written with merged config
      expect(fsp.writeFile).toHaveBeenCalledOnce();
      const writtenConfig = JSON.parse((fsp.writeFile as any).mock.calls[0][1]);
      expect(writtenConfig.ai.temperature).toBe(1.0);
      expect(writtenConfig.ai.provider).toBe("local"); // Preserved
      expect(writtenConfig.avatar.engineMode).toBe("auto"); // Preserved

      // BOTH clients should receive config_updated broadcast
      expect(ws1.send).toHaveBeenCalled();
      expect(ws2.send).toHaveBeenCalled();

      // Find the broadcast call (config_updated)
      const ws2Calls = ws2.send.mock.calls.map((c: any) => JSON.parse(c[0]));
      const broadcastCall = ws2Calls.find((c: any) => c.event === "config_updated");
      expect(broadcastCall).toBeDefined();
      expect(broadcastCall.payload.ai.temperature).toBe(1.0);
    });
  });

  describe("sendSkillsList / sendSystemStatus", () => {
    it("should send skills_list to specific client", () => {
      const ctrl = new UIController(0);
      const ws = createMockWS();

      simulateConnection(ctrl, ws);

      ctrl.sendSkillsList(ws, [{ name: "WebSearch", description: "Search web" }]);

      const response = JSON.parse(ws.send.mock.calls[0][0]);
      expect(response.event).toBe("skills_list");
      expect(response.payload.skills).toHaveLength(1);
      expect(response.payload.skills[0].name).toBe("WebSearch");
    });

    it("should send system_status to specific client", () => {
      const ctrl = new UIController(0);
      const ws = createMockWS();

      simulateConnection(ctrl, ws);

      ctrl.sendSystemStatus(ws, { model: "gemma4", uptime: 123 });

      const response = JSON.parse(ws.send.mock.calls[0][0]);
      expect(response.event).toBe("system_status");
      expect(response.payload.model).toBe("gemma4");
    });
  });

  describe("Security", () => {
    it("should not reset tokens when some clients remain", () => {
      const ctrl = new UIController(0);
      const ws1 = createMockWS();
      const ws2 = createMockWS();

      simulateConnection(ctrl, ws1);
      simulateConnection(ctrl, ws2);

      // ws1 disconnects, ws2 still connected
      ws1.emit("close");

      // Tokens should still be valid (broadcasting should work)
      ctrl.broadcastUIEvent("test", {});
      expect(ws2.send).toHaveBeenCalled();
    });

    it("should NOT broadcast without validated state", () => {
      const ctrl = new UIController(0);
      // No clients connected, no tokens initialized
      ctrl.broadcastUIEvent("test", {});
      // Should not crash, just log error
    });

    it("should handle malformed JSON gracefully", () => {
      const ctrl = new UIController(0);
      const ws = createMockWS();

      simulateConnection(ctrl, ws);

      // Send invalid JSON
      ws.emit("message", Buffer.from("not json {{}"), false);

      // Should not crash, ws.send should NOT be called
      expect(ws.send).not.toHaveBeenCalled();
    });
  });
});
