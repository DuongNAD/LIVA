import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { mount } from "@vue/test-utils";
import { ref } from "vue";

// Mock Node url to prevent JSDOM path resolution crashes
vi.mock("url", async (importOriginal) => {
  const original = await importOriginal<typeof import("url")>();
  return {
    ...original,
    fileURLToPath: () => "C:\\dummy",
  };
});

// Mock platform
vi.mock("../../src/platform/index", () => ({
  detectPlatform: () => ({
    platformName: "web",
    getWindowSize: () => Promise.resolve({ width: 800, height: 600 }),
    toggleGhostMode: vi.fn(),
    minimizeToTray: vi.fn(),
    quitApp: vi.fn(),
    readVaultKey: vi.fn(),
    writeVaultKey: vi.fn(),
    onGatewayReady: vi.fn(),
    invokeBackend: vi.fn().mockResolvedValue(null),
  }),
}));

// Mock useGateway
vi.mock("../../src/composables/useGateway", () => ({
  useGateway: () => ({
    userProfile: ref({ name: "User", language: "vi-VN" }),
    isConnected: ref(true),
    systemStatus: ref({}),
    configData: ref({}),
    sendMsg: vi.fn(),
    saveUserProfile: vi.fn(),
    registerCallback: vi.fn(),
    unregisterCallback: vi.fn(),
  }),
}));

// Mock useI18n
vi.mock("../../src/composables/useI18n", () => ({
  useI18n: () => ({
    t: (key: string) => key,
    currentLang: ref("vi-VN"),
  }),
}));

// Mock useVoicePipeline
vi.mock("../../src/composables/useVoicePipeline", () => ({
  useVoicePipeline: () => ({
    state: ref("OFF"),
    isReady: ref(false),
    pipelineState: ref("IDLE"),
    isSpeaking: ref(false),
    transcript: ref(""),
    aiResponse: ref(""),
    audioLevel: ref(0),
    isSupported: ref(true),
    startPipeline: vi.fn().mockResolvedValue(undefined),
    stopPipeline: vi.fn().mockResolvedValue(undefined),
    setPassive: vi.fn(),
    setProcessing: vi.fn(),
    keepAlive: vi.fn(),
    setLanguage: vi.fn(),
    setTtsVoice: vi.fn(),
    onWakeWordDetected: vi.fn(),
  }),
}));

// Mock use3DModel
vi.mock("../../src/composables/use3DModel", () => ({
  use3DModel: () => ({
    vrm: ref(null),
    currentModelFormat: ref(null),
    loadModel: vi.fn(),
    initRenderer: vi.fn(),
    startRenderLoop: vi.fn(),
    stopRenderLoop: vi.fn(),
    dispose: vi.fn(),
    updateLookAt: vi.fn(),
    updateExpressions: vi.fn(),
  }),
}));

// Mock useFaceTracking
vi.mock("../../src/composables/useFaceTracking", () => ({
  useFaceTracking: () => ({
    isActive: ref(false),
    startTracking: vi.fn(),
    stopTracking: vi.fn(),
    expressions: ref({}),
    lookAt: ref({ x: 0, y: 0, z: 0 }),
  }),
}));

import WidgetApp from "../../src/WidgetApp.vue";

const mockSockets: MockWebSocket[] = [];

class MockWebSocket {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSED = 3;

  readyState = MockWebSocket.CONNECTING;
  binaryType: BinaryType = "blob";
  onopen: ((event: Event) => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onerror: ((event: Event) => void) | null = null;
  onclose: ((event: CloseEvent) => void) | null = null;
  send = vi.fn();
  close = vi.fn(() => {
    this.readyState = MockWebSocket.CLOSED;
  });

  constructor(public readonly url: string) {
    mockSockets.push(this);
  }
}

describe("WidgetApp.vue", () => {
  beforeEach(() => {
    mockSockets.length = 0;
    vi.stubGlobal("WebSocket", MockWebSocket);
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("should mount and render widget layout", () => {
    const wrapper = mount(WidgetApp, {
      global: {
        provide: {
          platform: {
            platformName: "web",
            getWindowSize: () => Promise.resolve({ width: 800, height: 600 }),
            toggleGhostMode: vi.fn(),
            minimizeToTray: vi.fn(),
            quitApp: vi.fn(),
            readVaultKey: vi.fn(),
            writeVaultKey: vi.fn(),
            onGatewayReady: vi.fn(),
            invokeBackend: vi.fn().mockResolvedValue(null),
          },
        },
        stubs: {
          Live2DEngine: true,
          VRMEngine: true,
          VisionSensor: true,
          svg: true,
          use: true,
        },
      },
    });
    expect(wrapper.exists()).toBe(true);
    wrapper.unmount();
  });

  it("should reconnect the gateway after an unexpected socket close", async () => {
    vi.useFakeTimers();
    const wrapper = mount(WidgetApp, {
      global: {
        provide: {
          platform: {
            platformName: "web",
            invokeBackend: vi.fn().mockResolvedValue(null),
          },
        },
        stubs: {
          Live2DEngine: true,
          VRMEngine: true,
          VisionSensor: true,
        },
      },
    });

    await vi.advanceTimersByTimeAsync(0);
    expect(mockSockets).toHaveLength(1);
    mockSockets[0].onclose?.(new CloseEvent("close"));
    await vi.advanceTimersByTimeAsync(500);

    expect(mockSockets).toHaveLength(2);

    wrapper.unmount();
    mockSockets[1].onclose?.(new CloseEvent("close"));
    await vi.advanceTimersByTimeAsync(5_000);
    expect(mockSockets).toHaveLength(2);
  });
});
