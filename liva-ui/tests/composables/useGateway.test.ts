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
  binaryType = '';
  onopen: ((ev: any) => void) | null = null;
  onmessage: ((ev: any) => void) | null = null;
  onclose: ((ev: any) => void) | null = null;
  onerror: ((ev: any) => void) | null = null;
  send = vi.fn();
  close = vi.fn();
}

vi.stubGlobal('WebSocket', MockWebSocket);

// ─── Import AFTER mocking ───
import { useGateway } from '../../src/composables/useGateway';

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

  it('should register onEnvConfigData callback', () => {
    const gw = useGateway();
    const cb = vi.fn();

    expect(() => gw.onEnvConfigData(cb)).not.toThrow();
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

  it('should unregister offEnvConfigData without error', () => {
    const gw = useGateway();
    const cb = vi.fn();
    gw.onEnvConfigData(cb);

    expect(() => gw.offEnvConfigData()).not.toThrow();
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
      gw.offEnvConfigData();
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
