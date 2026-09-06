import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { TauriAdapter } from '../../src/platform/TauriAdapter';
import fs from 'node:fs';
import path from 'node:path';

describe('Challenger 2 Empirical Verification Suite — FE-01, FE-02, SEC-02', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('FE-01: TauriAdapter.invokeBackend Empirical Contract', () => {
    it('routes non-desktop commands through native_ipc_call with command and payload', async () => {
      const mockInvoke = vi.fn().mockResolvedValue({ status: 'ok', data: [1, 2, 3] });
      vi.doMock('@tauri-apps/api/core', () => ({
        invoke: mockInvoke,
      }));

      const adapter = new TauriAdapter();

      // Test cases with various payloads
      const testCases = [
        { cmd: 'diff:get_pending_hunks', args: { path: '/src/main.rs' }, expectedPayload: { path: '/src/main.rs' } },
        { cmd: 'agent:submit_hunk_decision', args: { hunk_id: 'h1', approved: true }, expectedPayload: { hunk_id: 'h1', approved: true } },
        { cmd: 'canvas:get_canvas_state', args: undefined, expectedPayload: {} },
        { cmd: 'custom:namespace:action', args: { nested: { val: 42 } }, expectedPayload: { nested: { val: 42 } } },
      ];

      for (const tc of testCases) {
        mockInvoke.mockClear();
        const res = await adapter.invokeBackend(tc.cmd, tc.args);
        expect(res).toEqual({ status: 'ok', data: [1, 2, 3] });
        expect(mockInvoke).toHaveBeenCalledTimes(1);
        expect(mockInvoke).toHaveBeenCalledWith('native_ipc_call', {
          command: tc.cmd,
          payload: tc.expectedPayload,
        });
      }
    });

    it('routes desktop internal commands directly without wrapping', async () => {
      const mockInvoke = vi.fn().mockResolvedValue({ success: true });
      vi.doMock('@tauri-apps/api/core', () => ({
        invoke: mockInvoke,
      }));

      const adapter = new TauriAdapter();
      const internalCmds = [
        'toggle_ghost_mode',
        'set_eco_mode',
        'update_interactive_zones',
        'open_dashboard',
        'open_setup',
        'issue_websocket_session',
      ];

      for (const cmd of internalCmds) {
        mockInvoke.mockClear();
        const args = { testParam: 123 };
        const res = await adapter.invokeBackend(cmd, args);
        expect(res).toEqual({ success: true });
        expect(mockInvoke).toHaveBeenCalledTimes(1);
        expect(mockInvoke).toHaveBeenCalledWith(cmd, args);
      }
    });

    it('catches and handles errors gracefully without throwing to caller', async () => {
      const mockInvoke = vi.fn().mockRejectedValue(new Error('Fatal IPC disconnect'));
      vi.doMock('@tauri-apps/api/core', () => ({
        invoke: mockInvoke,
      }));

      const adapter = new TauriAdapter();
      const res = await adapter.invokeBackend('diff:get_pending_hunks', { id: 1 });
      expect(res).toBeNull();
    });
  });

  describe('FE-02: Tauri Capabilities widget.json Permissions', () => {
    it('verifies widget.json contains core:window:allow-hide permission', () => {
      const widgetPath = path.resolve(__dirname, '../../../liva-desktop/src-tauri/capabilities/widget.json');
      expect(fs.existsSync(widgetPath)).toBe(true);

      const raw = fs.readFileSync(widgetPath, 'utf-8');
      const capability = JSON.parse(raw);

      expect(capability.identifier).toBe('widget');
      expect(capability.windows).toContain('widget');
      expect(Array.isArray(capability.permissions)).toBe(true);
      expect(capability.permissions).toContain('core:window:allow-hide');
      expect(capability.permissions).toContain('allow-native-ipc-call');
      expect(capability.permissions).toContain('allow-native-ipc-call-stream');
    });
  });

  describe('SEC-02: Root package.json overrides verification', () => {
    it('verifies package.json overrides pins fast-uri to ^3.1.6', () => {
      const pkgPath = path.resolve(__dirname, '../../../package.json');
      expect(fs.existsSync(pkgPath)).toBe(true);

      const raw = fs.readFileSync(pkgPath, 'utf-8');
      const pkg = JSON.parse(raw);

      expect(pkg.overrides).toBeDefined();
      expect(pkg.overrides['fast-uri']).toBe('^3.1.6');
    });
  });
});
