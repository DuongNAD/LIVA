import { vi, describe, it, expect, beforeEach } from "vitest";
import { TauriAdapter } from "../../src/platform/TauriAdapter";
import * as fs from "node:fs";
import * as path from "node:path";

const mockInvoke = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
}));

describe("Challenger Suite — FE-01 & FE-02", () => {
  let adapter: TauriAdapter;

  beforeEach(() => {
    vi.clearAllMocks();
    adapter = new TauriAdapter();
  });

  describe("FE-01: TauriAdapter.invokeBackend Routing & Payload Wrapping", () => {
    const internalCommands = [
      'toggle_ghost_mode',
      'set_eco_mode',
      'update_interactive_zones',
      'open_dashboard',
      'open_setup',
      'issue_websocket_session',
      'vault_secret_present',
      'store_vault_secret',
      'delete_vault_secret',
      'native_ipc_call',
      'native_ipc_call_stream',
    ];

    it.each(internalCommands)("routes desktop internal command '%s' directly without native_ipc_call wrapping", async (cmd) => {
      const payload = { testKey: "testVal", count: 42 };
      mockInvoke.mockResolvedValueOnce({ ok: true });

      const result = await adapter.invokeBackend(cmd, payload);

      expect(mockInvoke).toHaveBeenCalledTimes(1);
      expect(mockInvoke).toHaveBeenCalledWith(cmd, payload);
      expect(result).toEqual({ ok: true });
    });

    const customCommands = [
      { cmd: 'diff:get_pending_hunks', args: { session_id: 'sess-123' } },
      { cmd: 'canvas:get_canvas_state', args: undefined },
      { cmd: 'canvas:update_widget_state', args: { widget_id: 'w1', visible: true, pos: { x: 10, y: 20 } } },
      { cmd: 'agent:submit_hunk_decision', args: { hunk_id: 'hunk-99', accepted: true } },
      { cmd: 'browser:status', args: {} },
      { cmd: 'browser:screenshot', args: undefined },
      { cmd: 'pairing:initiate', args: { pin: '123456', device: 'MobileNode' } },
      { cmd: 'unknown:custom_domain_action', args: { flag: false } },
    ];

    it.each(customCommands)("wraps custom native IPC command '$cmd' into native_ipc_call", async ({ cmd, args }) => {
      mockInvoke.mockResolvedValueOnce({ status: "success", data: 123 });

      const result = await adapter.invokeBackend(cmd, args);

      expect(mockInvoke).toHaveBeenCalledTimes(1);
      expect(mockInvoke).toHaveBeenCalledWith("native_ipc_call", {
        command: cmd,
        payload: args ?? {},
      });
      expect(result).toEqual({ status: "success", data: 123 });
    });

    it("ensures payload is empty object `{}` when args is undefined or omitted", async () => {
      mockInvoke.mockResolvedValueOnce("ok");
      await adapter.invokeBackend("canvas:get_canvas_state");
      expect(mockInvoke).toHaveBeenCalledWith("native_ipc_call", {
        command: "canvas:get_canvas_state",
        payload: {},
      });
    });

    it("preserves deeply nested and heterogeneous payload types for custom commands", async () => {
      mockInvoke.mockResolvedValueOnce({ processed: true });
      const complexPayload = {
        meta: {
          timestamp: 1718000000,
          tags: ["alpha", "beta", "gamma"],
          nested: {
            deepKey: true,
            floatVal: 3.14159,
            nullVal: null,
          }
        },
        items: [1, "two", { three: 3 }]
      };

      const res = await adapter.invokeBackend("diff:apply_batch", complexPayload);
      expect(mockInvoke).toHaveBeenCalledWith("native_ipc_call", {
        command: "diff:apply_batch",
        payload: complexPayload,
      });
      expect(res).toEqual({ processed: true });
    });

    it("gracefully catches rejected errors from invoke and returns null without throwing", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("IPC transport disconnected"));
      const res1 = await adapter.invokeBackend("diff:get_pending_hunks", { id: "1" });
      expect(res1).toBeNull();

      mockInvoke.mockRejectedValueOnce("String error rejection");
      const res2 = await adapter.invokeBackend("open_dashboard");
      expect(res2).toBeNull();
    });
  });

  describe("FE-02: Security Audit of Tauri Capabilities (widget.json)", () => {
    const projectRoot = path.resolve(__dirname, "../../../");
    const widgetJsonPath = path.join(projectRoot, "liva-desktop/src-tauri/capabilities/widget.json");
    const dashboardJsonPath = path.join(projectRoot, "liva-desktop/src-tauri/capabilities/dashboard.json");

    it("verifies widget.json exists and is valid JSON", () => {
      expect(fs.existsSync(widgetJsonPath)).toBe(true);
      const content = fs.readFileSync(widgetJsonPath, "utf-8");
      const parsed = JSON.parse(content);
      expect(parsed.identifier).toBe("widget");
      expect(parsed.windows).toEqual(["widget"]);
    });

    it("verifies widget.json contains core:window:allow-hide permission", () => {
      const content = fs.readFileSync(widgetJsonPath, "utf-8");
      const parsed = JSON.parse(content);
      expect(parsed.permissions).toContain("core:window:allow-hide");
    });

    it("enforces least-privilege security policy: widget.json MUST NOT possess dangerous administrative capabilities", () => {
      const content = fs.readFileSync(widgetJsonPath, "utf-8");
      const parsed = JSON.parse(content);
      const permissions: string[] = parsed.permissions;

      // Forbidden capabilities for widget window (only allowed on dashboard/admin windows)
      const forbiddenPermissions = [
        "allow-vault-secret-present",
        "allow-store-vault-secret",
        "allow-delete-vault-secret",
        "dialog:allow-open",
        "process:allow-exit",
        "process:allow-restart",
        "core:window:allow-minimize",
        "core:window:allow-maximize",
        "core:window:allow-unmaximize",
        "core:window:allow-close",
        "core:window:allow-destroy",
      ];

      for (const forbidden of forbiddenPermissions) {
        expect(permissions).not.toContain(forbidden);
      }
    });

    it("confirms dashboard.json retains administrative permissions while widget is constrained", () => {
      const content = fs.readFileSync(dashboardJsonPath, "utf-8");
      const parsed = JSON.parse(content);
      const permissions: string[] = parsed.permissions;

      expect(permissions).toContain("core:window:allow-hide");
      expect(permissions).toContain("allow-vault-secret-present");
      expect(permissions).toContain("process:allow-exit");
      expect(permissions).toContain("dialog:allow-open");
    });
  });
});
