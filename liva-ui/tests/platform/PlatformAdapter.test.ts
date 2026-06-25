import { vi, describe, it, expect, beforeEach, afterEach } from "vitest";
import { MockWebAdapter } from "../../src/platform/MockWebAdapter";
import { TauriAdapter } from "../../src/platform/TauriAdapter";
import { detectPlatform } from "../../src/platform/index";

const mockInvoke = vi.fn();
const mockHide = vi.fn();
const mockExit = vi.fn();
const mockListen = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
}));

vi.mock("@tauri-apps/api/window", () => ({
  Window: {
    getCurrent: () => ({
      hide: () => mockHide(),
    }),
  },
}));

vi.mock("@tauri-apps/plugin-process", () => ({
  exit: (...args: any[]) => mockExit(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: (...args: any[]) => mockListen(...args),
}));

describe("Platform Adapters", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe("MockWebAdapter", () => {
    it("should initialize and add class to document body", () => {
      const adapter = new MockWebAdapter();
      expect(adapter.platformName).toBe("web");
      expect(document.body.classList.contains("web-mock-mode")).toBe(true);
    });

    it("should return window size", async () => {
      const adapter = new MockWebAdapter();
      const size = await adapter.getWindowSize();
      expect(size).toHaveProperty("width");
      expect(size).toHaveProperty("height");
    });

    it("should mock toggleGhostMode, minimizeToTray, invokeBackend without errors", async () => {
      const adapter = new MockWebAdapter();
      await expect(adapter.toggleGhostMode(true)).resolves.not.toThrow();
      await expect(adapter.minimizeToTray()).resolves.not.toThrow();
      const res = await adapter.invokeBackend("some_command", { arg: 1 });
      expect(res).toBeNull();
    });

    it("should mock quitApp by calling window.close", async () => {
      const adapter = new MockWebAdapter();
      const closeSpy = vi.spyOn(window, "close").mockImplementation(() => {});
      await adapter.quitApp();
      expect(closeSpy).toHaveBeenCalled();
    });

    it("should read and write vault keys using localStorage", async () => {
      const adapter = new MockWebAdapter();
      await adapter.writeVaultKey("test_key", "secret_value");
      const value = await adapter.readVaultKey("test_key");
      expect(value).toBe("secret_value");
    });

    it("should trigger gateway ready callback after timeout", () => {
      vi.useFakeTimers();
      const adapter = new MockWebAdapter();
      const callback = vi.fn();
      adapter.onGatewayReady(callback);
      expect(callback).not.toHaveBeenCalled();
      vi.advanceTimersByTime(1000);
      expect(callback).toHaveBeenCalledWith(8002, null);
      vi.useRealTimers();
    });
  });

  describe("TauriAdapter", () => {
    it("should initialize with tauri name", () => {
      const adapter = new TauriAdapter();
      expect(adapter.platformName).toBe("tauri");
    });

    it("should return window size", async () => {
      const adapter = new TauriAdapter();
      const size = await adapter.getWindowSize();
      expect(size).toHaveProperty("width");
      expect(size).toHaveProperty("height");
    });

    it("should invoke toggle_ghost_mode in toggleGhostMode", async () => {
      const adapter = new TauriAdapter();
      await adapter.toggleGhostMode(true);
      expect(mockInvoke).toHaveBeenCalledWith("toggle_ghost_mode", { enabled: true });
    });

    it("should handle error in toggleGhostMode gracefully", async () => {
      const adapter = new TauriAdapter();
      mockInvoke.mockRejectedValueOnce(new Error("Invoking error"));
      await expect(adapter.toggleGhostMode(true)).resolves.not.toThrow();
    });

    it("should call Window hide in minimizeToTray", async () => {
      const adapter = new TauriAdapter();
      await adapter.minimizeToTray();
      expect(mockHide).toHaveBeenCalled();
    });

    it("should handle error in minimizeToTray gracefully", async () => {
      const adapter = new TauriAdapter();
      mockHide.mockImplementationOnce(() => {
        throw new Error("Window hide error");
      });
      await expect(adapter.minimizeToTray()).resolves.not.toThrow();
    });

    it("should call process exit in quitApp", async () => {
      const adapter = new TauriAdapter();
      await adapter.quitApp();
      expect(mockExit).toHaveBeenCalledWith(0);
    });

    it("should handle error in quitApp gracefully", async () => {
      const adapter = new TauriAdapter();
      mockExit.mockImplementationOnce(() => {
        throw new Error("Process exit error");
      });
      await expect(adapter.quitApp()).resolves.not.toThrow();
    });

    it("should read vault key from invoke", async () => {
      const adapter = new TauriAdapter();
      mockInvoke.mockResolvedValue("mocked_secret");
      const key = await adapter.readVaultKey("my_key");
      expect(mockInvoke).toHaveBeenCalledWith("read_vault_key", { key: "my_key" });
      expect(key).toBe("mocked_secret");
    });

    it("should handle error in readVaultKey gracefully", async () => {
      const adapter = new TauriAdapter();
      mockInvoke.mockRejectedValueOnce(new Error("Read vault error"));
      const key = await adapter.readVaultKey("my_key");
      expect(key).toBeNull();
    });

    it("should write vault key using invoke", async () => {
      const adapter = new TauriAdapter();
      await adapter.writeVaultKey("my_key", "my_val");
      expect(mockInvoke).toHaveBeenCalledWith("write_vault_key", { key: "my_key", value: "my_val" });
    });

    it("should handle error in writeVaultKey gracefully", async () => {
      const adapter = new TauriAdapter();
      mockInvoke.mockRejectedValueOnce(new Error("Write vault error"));
      await expect(adapter.writeVaultKey("my_key", "my_val")).resolves.not.toThrow();
    });

    it("should call invokeBackend via invoke", async () => {
      const adapter = new TauriAdapter();
      mockInvoke.mockResolvedValue("res");
      const res = await adapter.invokeBackend("cmd", { a: 1 });
      expect(mockInvoke).toHaveBeenCalledWith("cmd", { a: 1 });
      expect(res).toBe("res");
    });

    it("should handle error in invokeBackend gracefully", async () => {
      const adapter = new TauriAdapter();
      mockInvoke.mockRejectedValueOnce(new Error("Invoke backend error"));
      const res = await adapter.invokeBackend("cmd", { a: 1 });
      expect(res).toBeNull();
    });

    it("should subscribe to event onGatewayReady", async () => {
      const adapter = new TauriAdapter();
      const callback = vi.fn();
      mockListen.mockImplementation((event, cb) => {
        cb({ payload: { port: 1234, token: "tok" } });
        return Promise.resolve(() => {});
      });

      adapter.onGatewayReady(callback);
      
      // Wait for dynamic import/promise chain
      await new Promise(resolve => setTimeout(resolve, 0));
      
      expect(mockListen).toHaveBeenCalledWith("gateway-ready", expect.any(Function));
      expect(callback).toHaveBeenCalledWith(1234, "tok");
    });

    it("should handle error in onGatewayReady gracefully", async () => {
      const adapter = new TauriAdapter();
      const callback = vi.fn();
      mockListen.mockImplementationOnce(() => {
        return Promise.reject(new Error("Listen error"));
      });

      adapter.onGatewayReady(callback);
      await new Promise(resolve => setTimeout(resolve, 0));
      expect(callback).not.toHaveBeenCalled();
    });
  });

  describe("detectPlatform", () => {
    it("should return TauriAdapter if window.__TAURI_INTERNALS__ is set", () => {
      const originalTauri = (window as any).__TAURI_INTERNALS__;
      (window as any).__TAURI_INTERNALS__ = {};
      const adapter = detectPlatform();
      expect(adapter).toBeInstanceOf(TauriAdapter);
      (window as any).__TAURI_INTERNALS__ = originalTauri;
    });

    it("should return MockWebAdapter if window.__TAURI_INTERNALS__ is not set", () => {
      const originalTauri = (window as any).__TAURI_INTERNALS__;
      delete (window as any).__TAURI_INTERNALS__;
      const adapter = detectPlatform();
      expect(adapter).toBeInstanceOf(MockWebAdapter);
      if (originalTauri) {
        (window as any).__TAURI_INTERNALS__ = originalTauri;
      }
    });
  });
});
