/**
 * useVoicePipeline.test.ts — Unit Tests
 * ====================================
 * Tests the voice pipeline composable's state management, worker integration, and lifecycle.
 * Browser APIs (getUserMedia, AudioContext, Worker) are mocked.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// ─── Module-level mocks ───
const mockGetUserMedia = vi.fn();
const mockAudioContext = {
  createMediaStreamSource: vi.fn().mockReturnValue({
    connect: vi.fn(),
    disconnect: vi.fn(),
  }),
  createAnalyser: vi.fn().mockReturnValue({
    fftSize: 0,
    frequencyBinCount: 128,
    connect: vi.fn(),
    disconnect: vi.fn(),
    getByteFrequencyData: vi.fn(),
  }),
  createScriptProcessor: vi.fn().mockReturnValue({
    onaudioprocess: null,
    connect: vi.fn(),
    disconnect: vi.fn(),
  }),
  destination: {},
  close: vi.fn(),
  sampleRate: 16000,
};

// Setup globals before importing
Object.defineProperty(globalThis, "navigator", {
  value: {
    mediaDevices: {
      getUserMedia: mockGetUserMedia,
    },
  },
  writable: true,
  configurable: true,
});

Object.defineProperty(globalThis, "AudioContext", {
  value: class {
    constructor() {
      return mockAudioContext;
    }
  },
  writable: true,
  configurable: true,
});

Object.defineProperty(globalThis, "window", {
  value: {
    AudioContext: class {
      constructor() {
        return mockAudioContext;
      }
    },
    requestAnimationFrame: vi.fn(),
    cancelAnimationFrame: vi.fn(),
  },
  writable: true,
  configurable: true,
});

// Mock Global Worker for ONNX Wake Word Detector
class MockWorker {
  onmessage: ((event: any) => void) | null = null;
  onerror: ((error: any) => void) | null = null;
  postMessage = vi.fn((message) => {
    // Auto-simulate loading -> ready handshake
    if (message.type === "init") {
      setTimeout(() => {
        if (this.onmessage) {
          this.onmessage({ data: { type: "ready", success: true } });
        }
      }, 0);
    }
  });
  terminate = vi.fn();
  constructor(url: string, options?: any) {
    setTimeout(() => {
      if (this.onmessage) {
        this.onmessage({ data: { type: "loaded" } });
      }
    }, 0);
  }
}

Object.defineProperty(globalThis, "Worker", {
  value: MockWorker,
  writable: true,
  configurable: true,
});

import { useVoicePipeline } from "../../src/composables/useVoicePipeline";

describe("useVoicePipeline — Composable State & Lifecycle", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("should initialize with correct default state", () => {
    const { state, volumeLevel, isReady } = useVoicePipeline();

    expect(state.value).toBe("OFF");
    expect(volumeLevel.value).toBe(0);
    expect(isReady.value).toBe(false);
  });

  it("should transition state on toggleVoice", async () => {
    const { state, toggleVoice } = useVoicePipeline();

    // In OFF state, toggleVoice should do nothing
    toggleVoice();
    expect(state.value).toBe("OFF");

    // Manually force to PASSIVE to test toggle
    state.value = "PASSIVE";
    toggleVoice();
    expect(state.value).toBe("ACTIVE");

    toggleVoice();
    expect(state.value).toBe("PASSIVE");
  });

  it("should start pipeline successfully", async () => {
    const mockStream = {
      getTracks: vi.fn().mockReturnValue([{ stop: vi.fn() }]),
    };
    mockGetUserMedia.mockResolvedValue(mockStream);

    const { state, isReady, startPipeline } = useVoicePipeline();
    const mockWs = {
      readyState: 1, // WebSocket.OPEN
      send: vi.fn(),
    } as any;

    const startPromise = startPipeline(mockWs);
    
    // Fast-forward to handle mock worker timeout handshakes
    await vi.advanceTimersByTimeAsync(10);
    await startPromise;

    expect(state.value).toBe("PASSIVE");
    expect(isReady.value).toBe(true);
    expect(mockGetUserMedia).toHaveBeenCalled();
  });

  it("should handle start failure when getUserMedia throws", async () => {
    mockGetUserMedia.mockRejectedValue(new Error("Permission denied"));

    const { state, isReady, startPipeline } = useVoicePipeline();
    const mockWs = {} as any;

    const startPromise = startPipeline(mockWs);
    await vi.advanceTimersByTimeAsync(10);
    await startPromise;

    expect(state.value).toBe("OFF");
    expect(isReady.value).toBe(false);
  });

  it("should stop pipeline and clean up resources", async () => {
    const mockStream = {
      getTracks: vi.fn().mockReturnValue([{ stop: vi.fn() }]),
    };
    mockGetUserMedia.mockResolvedValue(mockStream);

    const { state, isReady, startPipeline, stopPipeline } = useVoicePipeline();
    const mockWs = {
      readyState: 1,
      send: vi.fn(),
    } as any;

    const startPromise = startPipeline(mockWs);
    await vi.advanceTimersByTimeAsync(10);
    await startPromise;

    expect(state.value).toBe("PASSIVE");

    await stopPipeline();

    expect(state.value).toBe("OFF");
    expect(isReady.value).toBe(false);
  });
});
