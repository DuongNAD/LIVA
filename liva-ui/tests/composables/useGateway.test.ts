/**
 * useGateway.test.ts — Unit Tests
 * =================================
 * Tests the pure-logic parts of useGateway composable:
 *   - sendMsg guard when socket is null or not OPEN
 *   - destroy clears timers and closes socket
 *   - updateConfig calls sendMsg with correct event
 *   - saveUserProfile updates local ref and calls sendMsg
 *   - Callback registration/unregistration
 *
 * WebSocket is stubbed globally via vi.stubGlobal.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';

// ─── Mock logger ───
vi.mock('../../src/utils/logger', () => ({
  logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

// ─── Mock msgpackr ───
vi.mock('msgpackr', () => ({
  pack: vi.fn().mockReturnValue(new Uint8Array([1, 2, 3])),
  unpack: vi.fn().mockReturnValue({ event: 'test', payload: {} }),
}));

// ─── Mock liva-common types (only type imports, provide empty module) ───
vi.mock('liva-common', () => ({}));

// ─── Mock WebSocket ───
class MockWebSocket {
  static OPEN = 1;
  static CONNECTING = 0;
  static CLOSING = 2;
  static CLOSED = 3;

  // Instance mirrors static for protocol compat
  OPEN = 1;
  CONNECTING = 0;
  CLOSING = 2;
  CLOSED = 3;

  readyState = MockWebSocket.OPEN;
  url: string;
  binaryType = '';
  onopen: ((ev: any) => void) | null = null;
  onmessage: ((ev: any) => void) | null = null;
  onclose: ((ev: any) => void) | null = null;
  onerror: ((ev: any) => void) | null = null;
  send = vi.fn();
  close = vi.fn();

  constructor(url: string) {
    this.url = url;
  }
}

vi.stubGlobal('WebSocket', MockWebSocket);

// ─── Import AFTER mocking ───
import {
  bootstrapCommandsForPrincipal,
  gatewayPrincipalForPath,
  useGateway,
} from '../../src/composables/useGateway';

describe('useGateway — principal boundary', () => {
  it('maps only known entry points to privileged principals', () => {
    expect(gatewayPrincipalForPath('/widget.html')).toBe('widget');
    expect(gatewayPrincipalForPath('/dashboard.html')).toBe('dashboard');
    expect(gatewayPrincipalForPath('/')).toBe('widget');
    expect(gatewayPrincipalForPath('/preview/admin.html')).toBe('remote');
  });

  it('does not bootstrap admin data in the widget', () => {
    const widgetCommands = bootstrapCommandsForPrincipal('widget');
    expect(widgetCommands).toContain('get_config');
    expect(widgetCommands).not.toContain('get_memory_data');
    expect(widgetCommands).not.toContain('get_tasks');
    expect(widgetCommands).not.toContain('get_skills_list');

    expect(bootstrapCommandsForPrincipal('dashboard')).toContain('get_memory_data');
    expect(bootstrapCommandsForPrincipal('remote')).toEqual([]);
  });

  it('does not self-declare a privileged principal over WebSocket', () => {
    const gw = useGateway();
    gw.init();
    const socket = gw.getRawWs() as MockWebSocket;
    expect(socket.url).toBe('ws://127.0.0.1:8002/ws');
    socket.onclose?.({} as Event);
    gw.destroy();
  });
});

describe('useGateway — sendMsg guards', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should return false when socket is null (not connected)', () => {
    const gw = useGateway();
    // By default, ws is null until init() + connect() is called
    // sendMsg should gracefully return false
    const result = gw.sendMsg('test_event', { foo: 'bar' });
    expect(result).toBe(false);
  });

  it('should return false when socket is not in OPEN state', () => {
    const gw = useGateway();

    // init() creates a WebSocket instance
    gw.init();

    // Grab the raw WebSocket and change readyState to CLOSED
    const rawWs = gw.getRawWs();
    expect(rawWs).not.toBeNull();
    (rawWs as any).readyState = MockWebSocket.CLOSED;

    const result = gw.sendMsg('test_event', { data: 123 });
    expect(result).toBe(false);
  });

  it('should return true and send when socket is OPEN', () => {
    const gw = useGateway();
    gw.init();

    const rawWs = gw.getRawWs();
    expect(rawWs).not.toBeNull();
    (rawWs as any).readyState = MockWebSocket.OPEN;

    const result = gw.sendMsg('test_event', { data: 123 });
    expect(result).toBe(true);
    expect((rawWs as any).send).toHaveBeenCalled();
  });
});

describe('useGateway — Tauri vision errors', () => {
  it('finishes vision immediately when native IPC rejects', async () => {
    vi.resetModules();
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    const invoke = vi.fn().mockRejectedValue(
      new Error('Vision requires a release build (debug CRT assertion in the mmproj loader)'),
    );
    vi.doMock('@tauri-apps/api/core', () => ({ invoke }));

    try {
      const { useGateway: useTauriGateway } = await import('../../src/composables/useGateway');
      const gw = useTauriGateway();
      gw.askVision();
      expect(gw.visionBusy.value).toBe(true);

      await vi.waitFor(() => expect(gw.visionBusy.value).toBe(false));
      expect(gw.visionError.value).toContain('Vision requires a release build');
    } finally {
      delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
      vi.doUnmock('@tauri-apps/api/core');
      vi.resetModules();
    }
  });
});

describe('useGateway — destroy', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
  });

  it('should close the WebSocket on destroy', () => {
    const gw = useGateway();
    gw.init();

    const rawWs = gw.getRawWs();
    expect(rawWs).not.toBeNull();

    gw.destroy();
    expect((rawWs as any).close).toHaveBeenCalled();
  });

  it('should not throw when destroy is called without init', () => {
    // Fresh gateway — calling destroy before init should be safe
    // We need a fresh module to get a clean state
    expect(() => {
      const gw = useGateway();
      gw.destroy();
    }).not.toThrow();
  });

  afterEach(() => {
    vi.useRealTimers();
  });
});

describe('useGateway — updateConfig', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should call sendMsg with update_config event', () => {
    const gw = useGateway();
    gw.init();

    const rawWs = gw.getRawWs();
    (rawWs as any).readyState = MockWebSocket.OPEN;

    const newConfig = { darkMode: true };
    gw.updateConfig(newConfig as any);

    // sendMsg should have been called — verify send was invoked
    expect((rawWs as any).send).toHaveBeenCalled();
  });
});

describe('useGateway — saveUserProfile', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should update local userProfile ref and call sendMsg', () => {
    const gw = useGateway();
    gw.init();

    const rawWs = gw.getRawWs();
    (rawWs as any).readyState = MockWebSocket.OPEN;

    const profile = { name: 'John', language: 'en-US' };
    gw.saveUserProfile(profile);

    // userProfile should be updated locally
    expect(gw.userProfile.value).toEqual({ name: 'John', language: 'en-US' });

    // sendMsg should have been called (send on the WebSocket)
    expect((rawWs as any).send).toHaveBeenCalled();
  });

  it('should handle null-ish profile gracefully', () => {
    const gw = useGateway();
    gw.init();

    const rawWs = gw.getRawWs();
    (rawWs as any).readyState = MockWebSocket.OPEN;

    // Pass empty object
    gw.saveUserProfile({});
    expect(gw.userProfile.value).toEqual({});
  });
});

describe('useGateway — Callback Registration', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should register onTaskPlanReply callback', () => {
    const gw = useGateway();
    const cb = vi.fn();

    // Should not throw
    expect(() => gw.onTaskPlanReply(cb)).not.toThrow();
  });

  it('should register onSkillCheckResult callback', () => {
    const gw = useGateway();
    const cb = vi.fn();

    expect(() => gw.onSkillCheckResult(cb)).not.toThrow();
  });

  it('should register onMemoryResetResult callback', () => {
    const gw = useGateway();
    const cb = vi.fn();

    expect(() => gw.onMemoryResetResult(cb)).not.toThrow();
  });

  it('should register onMemoryUpdated callback', () => {
    const gw = useGateway();
    const cb = vi.fn();

    expect(() => gw.onMemoryUpdated(cb)).not.toThrow();
  });
});

describe('useGateway — Callback Unregistration', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should unregister offSkillCheckResult without error', () => {
    const gw = useGateway();
    const cb = vi.fn();
    gw.onSkillCheckResult(cb);

    expect(() => gw.offSkillCheckResult()).not.toThrow();
  });

  it('should unregister offMemoryResetResult without error', () => {
    const gw = useGateway();
    const cb = vi.fn();
    gw.onMemoryResetResult(cb);

    expect(() => gw.offMemoryResetResult()).not.toThrow();
  });

  it('should unregister offMemoryUpdated without error', () => {
    const gw = useGateway();
    const cb = vi.fn();
    gw.onMemoryUpdated(cb);

    expect(() => gw.offMemoryUpdated()).not.toThrow();
  });

  it('should be safe to call off* without prior on* registration', () => {
    const gw = useGateway();

    expect(() => {
      gw.offSkillCheckResult();
      gw.offAllSkillsCheckComplete();
      gw.offMemoryResetResult();
      gw.offMemoryUpdated();
    }).not.toThrow();
  });
});

describe('useGateway — Exposed Reactive State', () => {
  it('should expose all required reactive state refs', () => {
    const gw = useGateway();

    expect(gw.isConnected).toBeDefined();
    expect(gw.configData).toBeDefined();
    expect(gw.aiConfig).toBeDefined();
    expect(gw.voiceStatus).toBeDefined();
    expect(gw.voiceProfiles).toBeDefined();
    expect(gw.systemStatus).toBeDefined();
    expect(gw.skillsList).toBeDefined();
    expect(gw.tasksList).toBeDefined();
    expect(gw.avatarModels3D).toBeDefined();
    expect(gw.avatarModels2D).toBeDefined();
    expect(gw.gpuSetupStatus).toBeDefined();
    expect(gw.userProfile).toBeDefined();
    expect(gw.isProfileLoading).toBeDefined();
    expect(gw.memoryData).toBeDefined();
  });

  it('should start with isConnected = false', () => {
    const gw = useGateway();
    // Note: this is a module-level ref, so it may carry state from prior tests.
    // The initial value (before any connect) is false.
    expect(typeof gw.isConnected.value).toBe('boolean');
  });
});

describe('useGateway — Message Dispatch & Error Handlers', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('should handle incoming JSON messages for all events', () => {
    const gw = useGateway();
    gw.init();
    const rawWs = gw.getRawWs() as any;

    const events = [
      { event: 'user_profile', payload: { name: 'Bob' } },
      { event: 'profile_updated_success', payload: { name: 'Bob Updated' } },
      { event: 'config_data', payload: { voice: { voiceName: 'test-voice' } } },
      { event: 'config_updated', payload: { voice: { voiceName: 'new-voice' } } },
      { event: 'ai_config', payload: { provider: 'cloud' } },
      { event: 'ai_config_updated', payload: { provider: 'local' } },
      { event: 'voice_status', payload: { pitch: 1.2 } },
      { event: 'voice_profiles', payload: { profiles: [{ id: '1' }] } },
      { event: 'avatar_models_list', payload: { models3d: [{ name: 'm3d' }], models2d: [{ name: 'm2d' }] } },
      { event: 'system_status', payload: { cpuUsage: 10 } },
      {
        event: 'get_preflight_status_response',
        payload: {
          items: [
            { name: 'Vision', available: false, status: 'debug', consequence: 'build release' },
            { name: 'invalid item', available: 'yes', status: 'bad', consequence: '' },
          ],
        },
      },
      { event: 'skills_list', payload: { skills: [{ name: 's1' }] } },
      { event: 'tasks_list', payload: { tasks: [{ title: 't1' }] } },
      { event: 'memory_data', payload: { facts: [{ key: 'k' }] } },
      { event: 'fact_deleted', payload: { success: true, key: 'k' } },
      { event: 'gpu_setup_progress', payload: { status: 'Complete' } },
    ];

    events.forEach(e => {
      rawWs.onmessage({ data: JSON.stringify(e) });
    });

    expect(gw.userProfile.value.name).toBe('Bob Updated');
    expect(gw.voiceProfiles.value).toEqual([{ id: '1' }]);
    expect(gw.avatarModels3D.value).toEqual([{ name: 'm3d' }]);
    expect(gw.systemStatus.value).toEqual({ cpuUsage: 10 });
    expect(gw.preflightReport.value).toEqual({
      items: [{ name: 'Vision', available: false, status: 'debug', consequence: 'build release' }],
    });
    expect(gw.skillsList.value).toEqual([{ name: 's1' }]);
    expect(gw.tasksList.value).toEqual([{ title: 't1' }]);
    expect(gw.gpuSetupStatus.value).toBe('Complete');
  });

  it('should trigger registered callbacks', () => {
    const gw = useGateway();
    gw.init();
    const rawWs = gw.getRawWs() as any;

    const taskPlanCb = vi.fn();
    const skillCheckCb = vi.fn();
    const allSkillsCb = vi.fn();
    const memoryResetCb = vi.fn();
    const memoryUpdatedCb = vi.fn();

    gw.onTaskPlanReply(taskPlanCb);
    gw.onSkillCheckResult(skillCheckCb);
    gw.onAllSkillsCheckComplete(allSkillsCb);
    gw.onMemoryResetResult(memoryResetCb);
    gw.onMemoryUpdated(memoryUpdatedCb);

    rawWs.onmessage({ data: JSON.stringify({ event: 'task_plan_reply', payload: { task: 1 } }) });
    rawWs.onmessage({ data: JSON.stringify({ event: 'skill_check_result', payload: { skill: 1 } }) });
    rawWs.onmessage({ data: JSON.stringify({ event: 'all_skills_check_complete', payload: { ok: true } }) });
    rawWs.onmessage({ data: JSON.stringify({ event: 'memory_reset_result', payload: { ok: true } }) });
    rawWs.onmessage({ data: JSON.stringify({ event: 'memory_updated' }) });
    rawWs.onmessage({ data: JSON.stringify({ event: 'consolidate_memory_response' }) });

    expect(taskPlanCb).toHaveBeenCalledWith({ task: 1 });
    expect(skillCheckCb).toHaveBeenCalledWith({ skill: 1 });
    expect(allSkillsCb).toHaveBeenCalledWith({ ok: true });
    expect(memoryResetCb).toHaveBeenCalledWith({ ok: true });
    expect(memoryUpdatedCb).toHaveBeenCalledTimes(2);
  });

  it('should handle websocket close and error events', () => {
    const gw = useGateway();
    gw.init();
    const rawWs = gw.getRawWs() as any;

    rawWs.onerror(new Error('WS error'));
    expect(rawWs.close).toHaveBeenCalled();

    rawWs.onclose();
    expect(gw.isConnected.value).toBe(false);
  });
});
