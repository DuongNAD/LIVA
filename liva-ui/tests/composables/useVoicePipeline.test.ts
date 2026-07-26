/**
 * useVoicePipeline.test.ts — Unit Tests
 * ====================================
 * Tests the voice pipeline composable's state management, worker integration, and lifecycle.
 * Browser APIs (getUserMedia, AudioContext, Worker) are mocked.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// ─── Module-level mocks ───
const mockGetUserMedia = vi.fn();
const mockAudioWorkletNodes: MockAudioWorkletNode[] = [];
const mockWorkers: MockWorker[] = [];

class MockAudioWorkletNode {
  port = {
    onmessage: null as ((event: MessageEvent<Float32Array>) => void) | null,
  };
  connect = vi.fn();
  disconnect = vi.fn();

  constructor(
    public context: AudioContext,
    public name: string,
    public options?: AudioWorkletNodeOptions,
  ) {
    mockAudioWorkletNodes.push(this);
  }
}

const mockAudioContext = {
  audioWorklet: {
    addModule: vi.fn().mockResolvedValue(undefined),
  },
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

Object.defineProperty(globalThis, "AudioWorkletNode", {
  value: MockAudioWorkletNode,
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
    mockWorkers.push(this);
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
    mockAudioWorkletNodes.length = 0;
    mockWorkers.length = 0;
    mockAudioContext.audioWorklet.addModule.mockResolvedValue(undefined);
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
    mockGetUserMedia.mockImplementation(() => Promise.reject(new Error("Permission denied")));

    const { state, isReady, startPipeline } = useVoicePipeline();
    const mockWs = {} as any;

    const startPromise = startPipeline(mockWs);
    const rejectsPromise = expect(startPromise).rejects.toThrow("Permission denied");
    
    await vi.advanceTimersByTimeAsync(10);
    await rejectsPromise;

    expect(state.value).toBe("OFF");
    expect(isReady.value).toBe(false);
  });

  it("should not reject when the mic is merely blocked — that is a state, not a fault", async () => {
    // Ca thật: webview sandbox (Browser pane của công cụ dev) chặn cứng quyền mic.
    // getUserMedia ném DOMException NotAllowedError; pipeline phải tắt êm chứ không
    // đẩy một rejection mà mọi call site đều phải nuốt.
    const denied = new Error("Permission denied");
    denied.name = "NotAllowedError";
    mockGetUserMedia.mockImplementation(() => Promise.reject(denied));

    const { state, isReady, startPipeline, pipelineError, pipelineErrorKind, stopPipeline } = useVoicePipeline();

    const startPromise = startPipeline({} as any);
    await vi.advanceTimersByTimeAsync(10);
    await expect(startPromise).resolves.toBeUndefined();

    expect(state.value).toBe("OFF");
    expect(isReady.value).toBe(false);
    expect(pipelineErrorKind.value).toBe("permission");
    expect(pipelineError.value).not.toBe("");
    // Chuỗi phơi ra màn hình là câu người đọc được, không phải `message` của DOMException.
    expect(pipelineError.value).not.toContain("Permission denied");

    await stopPipeline();
    expect(pipelineErrorKind.value).toBe("none");
  });

  it("should skip getUserMedia entirely when permission is already denied", async () => {
    // Quyền đã bị từ chối thì gọi getUserMedia chỉ tổ khiến trình duyệt dựng thêm
    // một banner "trang này xin quyền micro" rồi ném đúng lỗi đã biết trước.
    const originalPermissions = (globalThis.navigator as any).permissions;
    (globalThis.navigator as any).permissions = {
      query: vi.fn().mockResolvedValue({ state: "denied" }),
    };

    try {
      const { state, startPipeline, pipelineErrorKind, stopPipeline } = useVoicePipeline();

      const startPromise = startPipeline({} as any);
      await vi.advanceTimersByTimeAsync(10);
      await startPromise;

      expect(mockGetUserMedia).not.toHaveBeenCalled();
      expect(state.value).toBe("OFF");
      expect(pipelineErrorKind.value).toBe("permission");

      await stopPipeline();
    } finally {
      (globalThis.navigator as any).permissions = originalPermissions;
    }
  });

  it("should release microphone and AudioContext when worklet initialization fails", async () => {
    const stopTrack = vi.fn();
    const mockStream = {
      getTracks: vi.fn().mockReturnValue([{ stop: stopTrack }]),
    };
    mockGetUserMedia.mockResolvedValue(mockStream);
    mockAudioContext.audioWorklet.addModule.mockRejectedValueOnce(new Error("worklet load failed"));

    const { startPipeline } = useVoicePipeline();
    const startPromise = startPipeline({} as WebSocket);
    const rejectsPromise = expect(startPromise).rejects.toThrow("worklet load failed");

    await vi.advanceTimersByTimeAsync(10);
    await rejectsPromise;

    expect(stopTrack).toHaveBeenCalledOnce();
    expect(mockAudioContext.close).toHaveBeenCalledOnce();
  });

  it("should share concurrent startup instead of creating duplicate workers or microphones", async () => {
    const firstPipeline = useVoicePipeline();
    await firstPipeline.stopPipeline();
    mockWorkers.length = 0;

    const mockStream = {
      getTracks: vi.fn().mockReturnValue([{ stop: vi.fn() }]),
    };
    mockGetUserMedia.mockResolvedValue(mockStream);

    const mockWs = { readyState: 1, send: vi.fn() } as any;
    void firstPipeline.startPipeline(mockWs);
    void firstPipeline.startPipeline(mockWs);

    await vi.advanceTimersByTimeAsync(10);

    expect(mockWorkers).toHaveLength(1);
    expect(mockGetUserMedia).toHaveBeenCalledOnce();
  });

  it("should not resurrect a pipeline stopped while microphone permission is pending", async () => {
    const stopTrack = vi.fn();
    const mockStream = {
      getTracks: vi.fn().mockReturnValue([{ stop: stopTrack }]),
    };
    let resolveStream!: (stream: typeof mockStream) => void;
    mockGetUserMedia.mockReturnValue(new Promise((resolve) => {
      resolveStream = resolve;
    }));

    const pipeline = useVoicePipeline();
    const startPromise = pipeline.startPipeline({ readyState: 1, send: vi.fn() } as any);
    await vi.advanceTimersByTimeAsync(10);
    expect(mockGetUserMedia).toHaveBeenCalledOnce();

    await pipeline.stopPipeline();
    resolveStream(mockStream);
    await startPromise;

    expect(pipeline.state.value).toBe("OFF");
    expect(pipeline.isReady.value).toBe(false);
    expect(stopTrack).toHaveBeenCalledOnce();
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

  describe("Web Speech API Fallback", () => {
    let mockSpeechRecognitionInstance: any;
    let originalSpeechRecognition: any;
    let instantiations = 0;

    beforeEach(() => {
      instantiations = 0;
      mockSpeechRecognitionInstance = {
        start: vi.fn(),
        stop: vi.fn(),
        lang: "",
        continuous: false,
        interimResults: false,
        onresult: null,
        onerror: null,
        onend: null,
      };

      originalSpeechRecognition = (globalThis as any).SpeechRecognition;
      (globalThis as any).SpeechRecognition = class {
        constructor() {
          instantiations++;
          return mockSpeechRecognitionInstance;
        }
      };
    });

    afterEach(() => {
      if (originalSpeechRecognition) {
        (globalThis as any).SpeechRecognition = originalSpeechRecognition;
      } else {
        delete (globalThis as any).SpeechRecognition;
      }
    });

    it("should manage Web Speech fallback state correctly", () => {
      const { webSpeechFallbackActive, activateWebSpeechFallback, deactivateWebSpeechFallback } = useVoicePipeline();

      expect(webSpeechFallbackActive.value).toBe(false);

      activateWebSpeechFallback();
      expect(webSpeechFallbackActive.value).toBe(true);
      expect(instantiations).toBe(1);

      deactivateWebSpeechFallback();
      expect(webSpeechFallbackActive.value).toBe(false);
      expect(mockSpeechRecognitionInstance.stop).toHaveBeenCalled();
    });

    it("should start speech recognition when pipeline state transitions to ACTIVE or PROCESSING", async () => {
      const mockStream = {
        getTracks: vi.fn().mockReturnValue([{ stop: vi.fn() }]),
      };
      mockGetUserMedia.mockResolvedValue(mockStream);

      const { state, startPipeline, activateWebSpeechFallback } = useVoicePipeline();
      const mockWs = {
        readyState: 1,
        send: vi.fn(),
      } as any;

      const startPromise = startPipeline(mockWs);
      await vi.advanceTimersByTimeAsync(10);
      await startPromise;

      expect(state.value).toBe("PASSIVE");

      activateWebSpeechFallback();
      // Should not start yet since state is PASSIVE
      expect(mockSpeechRecognitionInstance.start).not.toHaveBeenCalled();

      // Transition to ACTIVE
      state.value = "ACTIVE";
      await vi.advanceTimersByTimeAsync(10);

      // Now it should start
      expect(mockSpeechRecognitionInstance.start).toHaveBeenCalled();
    });

    it("should trigger onresult and send text through websocket", async () => {
      const mockStream = {
        getTracks: vi.fn().mockReturnValue([{ stop: vi.fn() }]),
      };
      mockGetUserMedia.mockResolvedValue(mockStream);

      const { state, startPipeline, activateWebSpeechFallback } = useVoicePipeline();
      const mockWs = {
        readyState: 1, // WebSocket.OPEN
        send: vi.fn(),
      } as any;

      const startPromise = startPipeline(mockWs);
      await vi.advanceTimersByTimeAsync(10);
      await startPromise;

      state.value = "ACTIVE";
      activateWebSpeechFallback();

      const mockEvent = {
        resultIndex: 0,
        results: [
          [{ transcript: "Xin chào LIVA" }]
        ]
      } as any;
      mockEvent.results[0].isFinal = true;

      mockSpeechRecognitionInstance.onresult(mockEvent);

      expect(mockWs.send).toHaveBeenCalled();
    });

    it("should handle error and end events in speech recognition", async () => {
      const { state, activateWebSpeechFallback } = useVoicePipeline();
      state.value = "ACTIVE";
      activateWebSpeechFallback();

      mockSpeechRecognitionInstance.onerror({ error: "network" });
      mockSpeechRecognitionInstance.onend();
      
      expect(mockSpeechRecognitionInstance.start).toHaveBeenCalledTimes(2);
    });
  });

  describe("Audio Processing and WebSocket Valve", () => {
    it("should process audio chunks and send to worker or websocket based on state", async () => {
      const mockStream = {
        getTracks: vi.fn().mockReturnValue([{ stop: vi.fn() }]),
      };
      mockGetUserMedia.mockResolvedValue(mockStream);

      const { state, startPipeline, stopPipeline } = useVoicePipeline();
      const mockWs = {
        readyState: 1, // WebSocket.OPEN
        send: vi.fn(),
      } as any;

      const startPromise = startPipeline(mockWs);
      await vi.advanceTimersByTimeAsync(10);
      await startPromise;

      const mockProcessor = mockAudioWorkletNodes[0];
      expect(mockProcessor).toBeDefined();
      expect(mockAudioContext.audioWorklet.addModule).toHaveBeenCalledOnce();
      expect(mockAudioContext.createScriptProcessor).not.toHaveBeenCalled();
      expect(mockProcessor.name).toBe("liva-mic-capture");
      expect(mockProcessor.options?.processorOptions).toEqual({
        frameSize: 512,
      });
      expect(mockProcessor.port.onmessage).toBeTypeOf("function");

      // 1. Passive state with audio (rms > 0.002) -> should post to worker
      state.value = "PASSIVE";
      const noisyFrame = new Float32Array(512);
      noisyFrame.fill(0.1); // High RMS
      mockProcessor.port.onmessage?.({ data: noisyFrame } as MessageEvent<Float32Array>);

      // 2. Active state with audio -> should send raw PCM to websocket
      state.value = "ACTIVE";
      mockProcessor.port.onmessage?.({ data: noisyFrame } as MessageEvent<Float32Array>);
      expect(mockWs.send).toHaveBeenCalled();

      // 3. Off state -> should return early
      state.value = "OFF";
      mockWs.send.mockClear();
      mockProcessor.port.onmessage?.({ data: noisyFrame } as MessageEvent<Float32Array>);
      expect(mockWs.send).not.toHaveBeenCalled();

      await stopPipeline();
      expect(mockProcessor.port.onmessage).toBeNull();
    });
  });

  describe("Chống tự nghe khi loa LIVA đang phát", () => {
    /**
     * Bộ dò wake-word chỉ nhìn RMS energy nên tiếng TTS vọng vào mic là đủ để nó
     * báo "Hey Liva". Cổng `state === 'PASSIVE'` một mình không chắn được ca gõ
     * chat: state ở PASSIVE suốt lúc LIVA đọc câu trả lời.
     */
    async function startFreshPipeline() {
      const pipeline = useVoicePipeline();
      // Ép tạo worker mới để postMessage đếm được từ 0 (worker là module-scope).
      await pipeline.stopPipeline();
      mockWorkers.length = 0;
      mockAudioWorkletNodes.length = 0;

      mockGetUserMedia.mockResolvedValue({
        getTracks: vi.fn().mockReturnValue([{ stop: vi.fn() }]),
      });

      const startPromise = pipeline.startPipeline({ readyState: 1, send: vi.fn() } as any);
      await vi.advanceTimersByTimeAsync(10);
      await startPromise;

      const loudFrame = new Float32Array(512);
      loudFrame.fill(0.2); // rms 0,2 — vượt xa cổng 0,002

      return {
        pipeline,
        worker: mockWorkers[0],
        feedMic: () =>
          mockAudioWorkletNodes[0].port.onmessage?.({ data: loudFrame } as MessageEvent<Float32Array>),
      };
    }

    const countSentTo = (worker: MockWorker, type: string) =>
      worker.postMessage.mock.calls.filter(([msg]) => msg?.type === type).length;

    it("vẫn nạp mic cho bộ wake-word khi PASSIVE và loa im", async () => {
      const { pipeline, worker, feedMic } = await startFreshPipeline();
      expect(pipeline.state.value).toBe("PASSIVE");

      feedMic();

      expect(countSentTo(worker, "audio")).toBe(1);
      await pipeline.stopPipeline();
    });

    it("ngưng nạp lúc loa phát, xoá cửa sổ trượt và giữ chặn qua đuôi vọng", async () => {
      const { pipeline, worker, feedMic } = await startFreshPipeline();

      pipeline.muteWakeWord();
      feedMic();
      feedMic();
      expect(countSentTo(worker, "audio")).toBe(0);

      pipeline.unmuteWakeWord();
      // Cửa sổ trượt còn lẫn tiếng loa — phải bị xoá, không thì đoạn ghép rời rạc
      // lại thành một bậc năng lượng giống cụm wake-word.
      expect(countSentTo(worker, "reset")).toBe(1);
      feedMic();
      expect(countSentTo(worker, "audio")).toBe(0);

      await vi.advanceTimersByTimeAsync(500); // qua đuôi vọng 400 ms
      feedMic();
      expect(countSentTo(worker, "audio")).toBe(1);

      await pipeline.stopPipeline();
    });

    it("muteWakeWordFor chỉ nới dài mốc chặn, không rút ngắn", async () => {
      const { pipeline, worker, feedMic } = await startFreshPipeline();

      pipeline.muteWakeWordFor(1000);
      pipeline.muteWakeWordFor(100); // không được kéo mốc về gần
      await vi.advanceTimersByTimeAsync(500);
      feedMic();
      expect(countSentTo(worker, "audio")).toBe(0);

      await vi.advanceTimersByTimeAsync(600);
      feedMic();
      expect(countSentTo(worker, "audio")).toBe(1);

      await pipeline.stopPipeline();
    });
  });
});
