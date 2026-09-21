/**
 * useAvatarWorkerBridge.spec.ts
 * ==============================
 * Comprehensive Vitest Unit Test Suite for useAvatarWorkerBridge composable
 * and Audio-Viseme Clock Delta Synchronization (Milestone M2, Features 9 & 10).
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { ref } from 'vue';
import {
  useAvatarWorkerBridge,
  checkOffscreenCanvasSupport,
  calculateClockOffset,
  wireAudioClockToBridge,
  WorkerVisemeEvaluator,
} from '../../src/composables/useAvatarWorkerBridge';
import type { VisemeCue } from '../../src/utils/speakerFrame';

// ═══════════════════════════════════════════════════════════════════
//  JSDOM / Vitest Mock Environment Harness
// ═══════════════════════════════════════════════════════════════════

class MockOffscreenCanvas {
  width: number;
  height: number;

  constructor(width: number, height: number) {
    this.width = width;
    this.height = height;
  }

  getContext = vi.fn().mockReturnValue({
    getParameter: vi.fn(),
    getExtension: vi.fn(),
  });
}

interface PostedMessageRecord {
  message: any;
  transfer?: Transferable[];
}

class MockWorker implements Worker {
  url: string | URL;
  options?: WorkerOptions;
  onmessage: ((ev: MessageEvent) => void) | null = null;
  onmessageerror: ((ev: MessageEvent) => void) | null = null;
  onerror: ((ev: ErrorEvent) => void) | null = null;

  postMessage: Worker['postMessage'] = vi.fn((message: any, transfer?: any) => {
    MockWorker.postedMessages.push({ message, transfer });
    MockWorker.lastMessage = message;
    MockWorker.lastTransfer = transfer;
  });

  terminate = vi.fn(() => {
    MockWorker.terminatedWorkers.push(this);
  });

  addEventListener = vi.fn((type: string, listener: any) => {
    if (type === 'message') this.messageListeners.push(listener);
    if (type === 'error') this.errorListeners.push(listener);
  });

  removeEventListener = vi.fn((type: string, listener: any) => {
    if (type === 'message') {
      this.messageListeners = this.messageListeners.filter((l) => l !== listener);
    }
    if (type === 'error') {
      this.errorListeners = this.errorListeners.filter((l) => l !== listener);
    }
  });

  dispatchEvent = vi.fn().mockReturnValue(true);

  private messageListeners: Array<(ev: MessageEvent) => void> = [];
  private errorListeners: Array<(ev: ErrorEvent) => void> = [];

  constructor(url: string | URL, options?: WorkerOptions) {
    this.url = url;
    this.options = options;
    MockWorker.instances.push(this);
  }

  // Simulation helpers
  simulateMessage(data: any) {
    const event = new MessageEvent('message', { data });
    if (this.onmessage) this.onmessage(event);
    for (const listener of this.messageListeners) {
      listener(event);
    }
  }

  simulateError(error: Error) {
    const event = new ErrorEvent('error', { error, message: error.message });
    if (this.onerror) this.onerror(event);
    for (const listener of this.errorListeners) {
      listener(event);
    }
  }

  // Static tracking
  static instances: MockWorker[] = [];
  static postedMessages: PostedMessageRecord[] = [];
  static lastMessage: any = null;
  static lastTransfer?: Transferable[];
  static terminatedWorkers: MockWorker[] = [];

  static reset() {
    MockWorker.instances = [];
    MockWorker.postedMessages = [];
    MockWorker.lastMessage = null;
    MockWorker.lastTransfer = undefined;
    MockWorker.terminatedWorkers = [];
  }
}

describe('useAvatarWorkerBridge & Clock Synchronization Test Suite', () => {
  let originalOffscreenCanvas: any;
  let originalTransferControl: any;
  let originalWorker: any;

  beforeEach(() => {
    MockWorker.reset();

    // Preserve originals
    originalOffscreenCanvas = (globalThis as any).OffscreenCanvas;
    originalTransferControl = HTMLCanvasElement.prototype.transferControlToOffscreen;
    originalWorker = (globalThis as any).Worker;

    // Install mock OffscreenCanvas and Worker
    (globalThis as any).OffscreenCanvas = MockOffscreenCanvas;
    HTMLCanvasElement.prototype.transferControlToOffscreen = function () {
      return new MockOffscreenCanvas(this.width, this.height) as unknown as OffscreenCanvas;
    };
    (globalThis as any).Worker = MockWorker;
  });

  afterEach(() => {
    // Restore originals
    (globalThis as any).OffscreenCanvas = originalOffscreenCanvas;
    HTMLCanvasElement.prototype.transferControlToOffscreen = originalTransferControl;
    (globalThis as any).Worker = originalWorker;
    vi.restoreAllMocks();
  });

  // ═════════════════════════════════════════════════════════════════
  // 1. Environment Detection & Graceful Fallback
  // ═════════════════════════════════════════════════════════════════
  describe('Environment Detection & Fallback', () => {
    it('reports isSupported = true when OffscreenCanvas and Worker are available', () => {
      expect(checkOffscreenCanvasSupport()).toBe(true);

      const bridge = useAvatarWorkerBridge({
        width: 400,
        height: 700,
        modelPath: 'models/avatar.vrm',
      });
      expect(bridge.isSupported.value).toBe(true);
    });

    it('reports isSupported = false and safely aborts initBridge when transferControlToOffscreen is missing', () => {
      delete (HTMLCanvasElement.prototype as any).transferControlToOffscreen;
      expect(checkOffscreenCanvasSupport()).toBe(false);

      const bridge = useAvatarWorkerBridge({
        width: 400,
        height: 700,
        modelPath: 'models/avatar.vrm',
      });
      expect(bridge.isSupported.value).toBe(false);

      const canvas = document.createElement('canvas');
      const initialized = bridge.initBridge(canvas);
      expect(initialized).toBe(false);
      expect(MockWorker.instances.length).toBe(0);
      expect(bridge.isWorkerActive.value).toBe(false);
    });

    it('returns false cleanly if target canvas is null or undefined', () => {
      const bridge = useAvatarWorkerBridge({
        width: 400,
        height: 700,
        modelPath: 'models/avatar.vrm',
      });

      const initialized = bridge.initBridge(null);
      expect(initialized).toBe(false);
      expect(bridge.error.value).toBe('Target canvas element is null');
      expect(MockWorker.instances.length).toBe(0);
    });
  });

  // ═════════════════════════════════════════════════════════════════
  // 2. Worker Creation & Canvas Ownership Transfer
  // ═════════════════════════════════════════════════════════════════
  describe('Worker Creation & Canvas Transfer', () => {
    it('transfers canvas control and posts INIT message with Transferable', () => {
      const canvas = document.createElement('canvas');
      canvas.width = 400;
      canvas.height = 700;

      const transferSpy = vi.spyOn(canvas, 'transferControlToOffscreen');

      const bridge = useAvatarWorkerBridge({
        canvas: ref(canvas),
        width: 400,
        height: 700,
        dpr: 1.5,
        modelPath: 'models/avatar.vrm',
      });

      const success = bridge.initBridge();
      expect(success).toBe(true);
      expect(transferSpy).toHaveBeenCalledTimes(1);

      expect(MockWorker.instances.length).toBe(1);

      expect(MockWorker.postedMessages.length).toBe(1);
      const initRecord = MockWorker.postedMessages[0];
      expect(initRecord.message.type).toBe('INIT');
      expect(initRecord.message.width).toBe(400);
      expect(initRecord.message.height).toBe(700);
      expect(initRecord.message.dpr).toBe(1.5);
      expect(initRecord.message.modelPath).toBe('models/avatar.vrm');
      expect(initRecord.transfer).toBeDefined();
      expect(initRecord.transfer?.length).toBe(1);
      expect(bridge.isWorkerActive.value).toBe(true);
    });

    it('handles transferControlToOffscreen failure gracefully without throwing', () => {
      const onError = vi.fn();
      const canvas = document.createElement('canvas');
      vi.spyOn(canvas, 'transferControlToOffscreen').mockImplementation(() => {
        throw new Error('Cannot transfer control from canvas with 2D context');
      });

      const bridge = useAvatarWorkerBridge({
        width: 400,
        height: 700,
        modelPath: 'models/avatar.vrm',
        onError,
      });

      const success = bridge.initBridge(canvas);
      expect(success).toBe(false);
      expect(bridge.error.value).toBe('Cannot transfer control from canvas with 2D context');
      expect(onError).toHaveBeenCalled();
      expect(bridge.isWorkerActive.value).toBe(false);
    });

    it('disposes previous worker when initBridge is called repeatedly', () => {
      const canvas1 = document.createElement('canvas');
      const canvas2 = document.createElement('canvas');

      const bridge = useAvatarWorkerBridge({
        width: 400,
        height: 700,
        modelPath: 'models/avatar.vrm',
      });

      expect(bridge.initBridge(canvas1)).toBe(true);
      expect(MockWorker.instances.length).toBe(1);
      const worker1 = MockWorker.instances[0];

      expect(bridge.initBridge(canvas2)).toBe(true);
      expect(MockWorker.instances.length).toBe(2);
      expect(worker1.terminate).toHaveBeenCalledTimes(1);
      expect(MockWorker.terminatedWorkers).toContain(worker1);
    });
  });

  // ═════════════════════════════════════════════════════════════════
  // 3. Outbound Worker Event Handling
  // ═════════════════════════════════════════════════════════════════
  describe('Outbound Event Handling', () => {
    it('handles READY message and updates isLoaded and modelFormat', () => {
      const onReady = vi.fn();
      const canvas = document.createElement('canvas');
      const bridge = useAvatarWorkerBridge({
        width: 400,
        height: 700,
        modelPath: 'models/avatar.vrm',
        onReady,
      });
      bridge.initBridge(canvas);
      const worker = MockWorker.instances[0];

      worker.simulateMessage({ type: 'READY', format: 'vrm', hasClips: true });

      expect(bridge.isLoaded.value).toBe(true);
      expect(bridge.modelFormat.value).toBe('vrm');
      expect(onReady).toHaveBeenCalledWith({ format: 'vrm', hasClips: true });
    });

    it('handles FPS_METRICS telemetry messages', () => {
      const onFpsMetrics = vi.fn();
      const canvas = document.createElement('canvas');
      const bridge = useAvatarWorkerBridge({
        width: 400,
        height: 700,
        modelPath: 'models/avatar.vrm',
        onFpsMetrics,
      });
      bridge.initBridge(canvas);
      const worker = MockWorker.instances[0];

      worker.simulateMessage({
        type: 'FPS_METRICS',
        currentFps: 59.8,
        drawCalls: 42,
        frameTimeMs: 16.7,
      });

      expect(bridge.fps.value).toBe(59.8);
      expect(bridge.drawCalls.value).toBe(42);
      expect(bridge.frameTimeMs.value).toBe(16.7);
      expect(onFpsMetrics).toHaveBeenCalledWith({
        type: 'FPS_METRICS',
        currentFps: 59.8,
        drawCalls: 42,
        frameTimeMs: 16.7,
      });
    });

    it('handles BOUNDS_UPDATED messages for interactive click-through zones', () => {
      const onBoundsUpdated = vi.fn();
      const canvas = document.createElement('canvas');
      const bridge = useAvatarWorkerBridge({
        width: 400,
        height: 700,
        modelPath: 'models/avatar.vrm',
        onBoundsUpdated,
      });
      bridge.initBridge(canvas);
      const worker = MockWorker.instances[0];

      const bounds = { x: 50, y: 120, width: 250, height: 500 };
      worker.simulateMessage({ type: 'BOUNDS_UPDATED', bounds });

      expect(bridge.interactiveBounds.value).toEqual(bounds);
      expect(onBoundsUpdated).toHaveBeenCalledWith(bounds);
    });

    it('handles ERROR events and worker failures', () => {
      const onError = vi.fn();
      const canvas = document.createElement('canvas');
      const bridge = useAvatarWorkerBridge({
        width: 400,
        height: 700,
        modelPath: 'models/avatar.vrm',
        onError,
      });
      bridge.initBridge(canvas);
      const worker = MockWorker.instances[0];

      worker.simulateMessage({
        type: 'ERROR',
        message: 'WebGL context creation failed',
        stack: 'Error at initWebGL',
      });

      expect(bridge.error.value).toBe('WebGL context creation failed');
      expect(onError).toHaveBeenCalled();
      expect(onError.mock.calls[0][0].message).toBe('WebGL context creation failed');
    });
  });

  // ═════════════════════════════════════════════════════════════════
  // 4. Audio-Viseme Clock Delta Synchronization Protocol
  // ═════════════════════════════════════════════════════════════════
  describe('Clock Delta Synchronization Protocol', () => {
    it('calculates clock offset accurately between AudioContext and performance.now()', () => {
      const mockAudioCtx = { currentTime: 2.456 } as AudioContext;
      const nowMs = 1500; // 1.500s

      const offset = calculateClockOffset(mockAudioCtx, nowMs);
      // offset = 2.456 - 1.5 = 0.956 seconds
      expect(offset).toBeCloseTo(0.956, 5);
    });

    it('forwards SYNC_CLOCK and SCHEDULE_CHUNK messages to worker', () => {
      const canvas = document.createElement('canvas');
      const bridge = useAvatarWorkerBridge({
        width: 400,
        height: 700,
        modelPath: 'models/avatar.vrm',
      });
      bridge.initBridge(canvas);

      bridge.syncClock(0.852);
      expect(MockWorker.lastMessage).toEqual({ type: 'SYNC_CLOCK', clockOffset: 0.852 });

      bridge.scheduleChunk(1.2, 0.45);
      expect(MockWorker.lastMessage).toEqual({
        type: 'SCHEDULE_CHUNK',
        startTimeSec: 1.2,
        durationSec: 0.45,
      });
    });

    it('forwards SET_VISEME_TIMELINE and AUDIO_RMS messages', () => {
      const canvas = document.createElement('canvas');
      const bridge = useAvatarWorkerBridge({
        width: 400,
        height: 700,
        modelPath: 'models/avatar.vrm',
      });
      bridge.initBridge(canvas);

      const cues: VisemeCue[] = [
        { v: 'aa', tMs: 0 },
        { v: 'ee', tMs: 120 },
        { v: 'oh', tMs: 250 },
      ];

      bridge.setVisemeTimeline(1, cues);
      expect(MockWorker.lastMessage).toEqual({
        type: 'SET_VISEME_TIMELINE',
        turnEpoch: 1,
        cues,
      });

      const bands = new Float32Array([0.1, 0.5, 0.8, 0.3, 0.05]);
      bridge.sendAudioRms(bands);
      expect(MockWorker.lastMessage).toEqual({
        type: 'AUDIO_RMS',
        bands,
      });
    });

    it('forwards FLUSH message instantly on barge-in', () => {
      const canvas = document.createElement('canvas');
      const bridge = useAvatarWorkerBridge({
        width: 400,
        height: 700,
        modelPath: 'models/avatar.vrm',
      });
      bridge.initBridge(canvas);

      bridge.flush(42);
      expect(MockWorker.lastMessage).toEqual({
        type: 'FLUSH',
        seq_id: 42,
      });
    });

    it('wires audio clock hooks correctly using wireAudioClockToBridge', () => {
      const mockAudioCtx = { currentTime: 1.0 } as AudioContext;
      const bridgeSpy: any = {
        syncClock: vi.fn(),
        scheduleChunk: vi.fn(),
        setVisemeTimeline: vi.fn(),
        flush: vi.fn(),
      };

      const hooks = wireAudioClockToBridge(mockAudioCtx, bridgeSpy);

      // Playback started
      hooks.onPlaybackStarted();
      expect(bridgeSpy.syncClock).toHaveBeenCalled();

      // Chunk scheduled
      hooks.onChunkScheduled({ startTimeSec: 1.05, durationSec: 0.1 });
      expect(bridgeSpy.syncClock).toHaveBeenCalledTimes(2);
      expect(bridgeSpy.scheduleChunk).toHaveBeenCalledWith(1.05, 0.1);

      // Visemes received
      const cues: VisemeCue[] = [{ v: 'aa', tMs: 0 }];
      hooks.onVisemeReceived(2, cues);
      expect(bridgeSpy.setVisemeTimeline).toHaveBeenCalledWith(2, cues);

      // Flush received
      hooks.onFlushReceived(2);
      expect(bridgeSpy.flush).toHaveBeenCalledWith(2);
    });
  });

  // ═════════════════════════════════════════════════════════════════
  // 5. Worker-Side Timeline Evaluation & < 5ms Drift Verification
  // ═════════════════════════════════════════════════════════════════
  describe('Worker Viseme Evaluator & Drift Invariants', () => {
    it('evaluates active visemes and satisfies drift < 5ms invariant', () => {
      const evaluator = new WorkerVisemeEvaluator();

      // Set clock offset: AudioContext is 1.0s ahead of performance.now()
      // e.g. at performance.now() = 2000ms (2.0s), audioCtx.currentTime = 3.0s -> offset = 1.0
      evaluator.syncClock(1.0);

      // Set phoneme timeline cues
      evaluator.setTimeline([
        { v: 'aa', tMs: 0 },
        { v: 'ee', tMs: 200 },
        { v: 'nil', tMs: 500 },
      ]);

      // Schedule chunk starting at audio time 3.0s for duration 0.6s (3.0s to 3.6s)
      evaluator.scheduleChunk(3.0, 0.6);

      // At performance.now() = 1950ms (audioTime = 2.95s, before audio start)
      let weights = evaluator.evaluate(1950, 0.016);
      expect(weights.aa).toBe(0);

      // At performance.now() = 2010ms (audioTime = 3.01s, 10ms into chunk -> viseme 'aa')
      weights = evaluator.evaluate(2010, 0.016);
      expect(weights.aa).toBeGreaterThan(0);

      // At performance.now() = 2220ms (audioTime = 3.22s, 220ms into chunk -> viseme 'ee')
      weights = evaluator.evaluate(2220, 0.016);
      expect(weights.ee).toBeGreaterThan(0);

      // Test drift precision: test at exact cue boundary + 4ms (must match, drift < 5ms)
      const audioTime = evaluator.getCurrentAudioTime(2204);
      expect(audioTime).toBeCloseTo(3.204, 3);

      // Flush clears timeline immediately
      evaluator.flush();
      weights = evaluator.evaluate(2220, 0.016);
      expect(weights.aa).toBe(0);
      expect(weights.ee).toBe(0);
    });
  });

  // ═════════════════════════════════════════════════════════════════
  // 6. Locomotion, Gaze, Resize & Visibility Controls
  // ═════════════════════════════════════════════════════════════════
  describe('Avatar Kinematics, Gaze & Window Controls', () => {
    it('forwards SET_LOCOMOTION, LOOK_AT, RESIZE, and VISIBILITY_CHANGE messages', () => {
      const canvas = document.createElement('canvas');
      const bridge = useAvatarWorkerBridge({
        width: 400,
        height: 700,
        modelPath: 'models/avatar.vrm',
      });
      bridge.initBridge(canvas);

      bridge.setLocomotionState('walk', 0.75);
      expect(MockWorker.lastMessage).toEqual({
        type: 'SET_LOCOMOTION',
        state: 'walk',
        motionWeight: 0.75,
      });

      bridge.lookAt(0.3, -0.15);
      expect(MockWorker.lastMessage).toEqual({
        type: 'LOOK_AT',
        yaw: 0.3,
        pitch: -0.15,
      });

      bridge.resize(800, 600, 2);
      expect(MockWorker.lastMessage).toEqual({
        type: 'RESIZE',
        width: 800,
        height: 600,
        dpr: 2,
      });

      bridge.setVisibility(false);
      expect(MockWorker.lastMessage).toEqual({
        type: 'VISIBILITY_CHANGE',
        visible: false,
      });
    });
  });

  // ═════════════════════════════════════════════════════════════════
  // 7. Cleanup & Worker Termination on Unmount
  // ═════════════════════════════════════════════════════════════════
  describe('Disposal and Lifecycle Teardown', () => {
    it('posts DISPOSE message and terminates worker on dispose()', () => {
      const canvas = document.createElement('canvas');
      const bridge = useAvatarWorkerBridge({
        width: 400,
        height: 700,
        modelPath: 'models/avatar.vrm',
      });
      bridge.initBridge(canvas);
      const worker = MockWorker.instances[0];

      bridge.dispose();

      expect(MockWorker.lastMessage).toEqual({ type: 'DISPOSE' });
      expect(worker.terminate).toHaveBeenCalledTimes(1);
      expect(bridge.isWorkerActive.value).toBe(false);
      expect(bridge.isLoaded.value).toBe(false);
    });
  });
});
