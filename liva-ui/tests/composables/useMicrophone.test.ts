/**
 * useMicrophone.test.ts — Unit Tests
 * ====================================
 * Tests the microphone composable's state management and lifecycle.
 * Browser APIs (getUserMedia, AudioContext) are mocked.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

// ─── Module-level mock for navigator.mediaDevices ───
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

// Setup globals before import
Object.defineProperty(globalThis, 'navigator', {
  value: {
    mediaDevices: {
      getUserMedia: mockGetUserMedia,
    },
  },
  writable: true,
  configurable: true,
});

Object.defineProperty(globalThis, 'AudioContext', {
  value: vi.fn().mockImplementation(() => mockAudioContext),
  writable: true,
  configurable: true,
});

Object.defineProperty(globalThis, 'window', {
  value: {
    AudioContext: vi.fn().mockImplementation(() => mockAudioContext),
  },
  writable: true,
  configurable: true,
});

import { useMicrophone } from "../../src/composables/useMicrophone";

describe("useMicrophone — Composable State", () => {

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should initialize with correct default state", () => {
    const { isListening, volumeLevel, isSupported } = useMicrophone();

    expect(isListening.value).toBe(false);
    expect(volumeLevel.value).toBe(0);
    // isSupported depends on navigator.mediaDevices
    expect(typeof isSupported.value).toBe("boolean");
  });

  it("should not crash when stopListening called before start", () => {
    const { stopListening } = useMicrophone();

    expect(() => {
      stopListening();
    }).not.toThrow();
  });

  it("should reset state on stopListening", () => {
    const { isListening, volumeLevel, stopListening } = useMicrophone();

    // Manually set listening state
    isListening.value = true;
    volumeLevel.value = 0.7;

    stopListening();

    expect(isListening.value).toBe(false);
    expect(volumeLevel.value).toBe(0);
  });

  it("should not start if already listening", async () => {
    const mockStream = {
      getTracks: vi.fn().mockReturnValue([{ stop: vi.fn() }]),
    };
    mockGetUserMedia.mockResolvedValue(mockStream);

    const { isListening, startListening, stopListening } = useMicrophone();

    const mockWs = {
      readyState: 1, // WebSocket.OPEN
      send: vi.fn(),
    } as any;

    // Force listening state
    isListening.value = true;

    await startListening(mockWs);

    // Should not call getUserMedia again
    expect(mockGetUserMedia).not.toHaveBeenCalled();

    // Cleanup
    isListening.value = false;
    stopListening();
  });

  it("should handle getUserMedia failure gracefully", async () => {
    mockGetUserMedia.mockRejectedValue(new Error("Permission denied"));

    const { isListening, startListening } = useMicrophone();

    const mockWs = {
      readyState: 1,
      send: vi.fn(),
    } as any;

    await startListening(mockWs);

    // Should remain not listening
    expect(isListening.value).toBe(false);
  });

  it("should not crash when stopListening called multiple times", () => {
    const { stopListening } = useMicrophone();

    expect(() => {
      stopListening();
      stopListening();
      stopListening();
    }).not.toThrow();
  });
});

describe("useMicrophone — Interface Contract", () => {
  it("should expose all required methods and refs", () => {
    const mic = useMicrophone();

    expect(mic).toHaveProperty("isListening");
    expect(mic).toHaveProperty("volumeLevel");
    expect(mic).toHaveProperty("isSupported");
    expect(mic).toHaveProperty("startListening");
    expect(mic).toHaveProperty("stopListening");
    expect(typeof mic.startListening).toBe("function");
    expect(typeof mic.stopListening).toBe("function");
  });
});
