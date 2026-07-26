import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { mount } from "@vue/test-utils";
import { nextTick, ref } from "vue";

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
    // ResourceMeter (nhúng trong WidgetApp) gọi `gateway.init()` trong
    // onMounted để tự đảm bảo có kết nối; thiếu nó thì mount đổ ngay.
    init: vi.fn(),
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
    pipelineError: ref(""),
    pipelineErrorKind: ref("none"),
    startPipeline: vi.fn().mockResolvedValue(undefined),
    stopPipeline: vi.fn().mockResolvedValue(undefined),
    setPassive: vi.fn(),
    setProcessing: vi.fn(),
    keepAlive: vi.fn(),
    setLanguage: vi.fn(),
    setTtsVoice: vi.fn(),
    onWakeWordDetected: vi.fn(),
    muteWakeWord: vi.fn(),
    unmuteWakeWord: vi.fn(),
    muteWakeWordFor: vi.fn(),
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

  /**
   * Thẻ xác nhận gửi tin — thứ DUY NHẤT trong widget gây ra hành động không
   * hoàn tác được. Test dựng thẳng bản nháp vào state rồi đọc DOM, vì lõi thật
   * mới là nơi sinh ra nó và ở đây lõi đã bị mock.
   */
  describe("thẻ xác nhận gửi tin nhắn", () => {
    const banNhap = {
      draft_id: "dr_1",
      platform: "telegram",
      display_name: "Minh Hiến",
      handle: "123456789",
      text: "ngủ đi",
    };

    /**
     * `isCollapsed` mặc định `true`, tức cả thanh chat lẫn thẻ đều không dựng.
     * Phải mở ra, nếu không mọi khẳng định "không thấy thẻ" đều đúng vì lý do
     * sai — đó là kiểu test xanh mà không kiểm gì cả.
     */
    async function mountWidget(draft: typeof banNhap | null) {
      const wrapper = mount(WidgetApp, {
        global: {
          provide: {
            platform: { platformName: "web", invokeBackend: vi.fn().mockResolvedValue(null) },
          },
          stubs: { Live2DEngine: true, VRMEngine: true, VisionSensor: true },
        },
      });
      await nextTick();
      const vm = wrapper.vm as unknown as {
        isCollapsed: boolean;
        pendingDraft: typeof banNhap | null;
      };
      vm.isCollapsed = false;
      vm.pendingDraft = draft;
      // `sendMsg` bỏ im gói tin nếu socket chưa OPEN. Không mở ra thì test bấm
      // nút sẽ "xanh" ở phần dựng DOM mà chẳng kiểm được gói nào đi ra.
      const socket = mockSockets[mockSockets.length - 1];
      if (socket) socket.readyState = MockWebSocket.OPEN;
      await nextTick();
      return wrapper;
    }

    const mountVoiBanNhap = () => mountWidget(banNhap);

    it("không có bản nháp thì không có thẻ, dù thanh chat đã mở", async () => {
      const wrapper = await mountWidget(null);
      // Thanh chat có dựng thật — nếu không, khẳng định dưới vô nghĩa.
      expect(wrapper.find(".chat-capsule").exists()).toBe(true);
      expect(wrapper.find(".draft-card").exists()).toBe(false);
      wrapper.unmount();
    });

    it("hiện cả tên lẫn địa chỉ đích, và nói rõ CHƯA gửi", async () => {
      const wrapper = await mountVoiBanNhap();
      const the = wrapper.find(".draft-card");
      expect(the.exists()).toBe(true);
      expect(the.text()).toContain("Minh Hiến");
      // Địa chỉ đích phải hiện: tên đúng mà số sai vẫn là gửi nhầm người.
      expect(the.text()).toContain("123456789");
      expect(the.text()).toContain("ngủ đi");
      expect(the.text()).toContain("telegram");
      // `useI18n` bị mock trả về chính key, nên khẳng định theo KEY. Chữ thật
      // ("Đã soạn tin — CHƯA gửi") được khoá ở `useI18n.ts`, không phải ở đây.
      expect(the.text()).toContain("wg_draft_title");
      expect(wrapper.find(".draft-btn-send").exists()).toBe(true);
      expect(wrapper.find(".draft-btn-cancel").exists()).toBe(true);
      wrapper.unmount();
    });

    it("bấm xác nhận gửi message:confirm kèm đúng draftId", async () => {
      const wrapper = await mountVoiBanNhap();
      const socket = mockSockets[mockSockets.length - 1];
      socket.send.mockClear();

      await wrapper.find(".draft-btn-send").trigger("click");

      const goiDi = socket.send.mock.calls
        .map(([raw]: [string]) => JSON.parse(raw))
        .filter((m: { event: string }) => m.event === "message:confirm");
      expect(goiDi).toHaveLength(1);
      expect(goiDi[0].payload.draftId).toBe("dr_1");
      wrapper.unmount();
    });

    it("bấm huỷ gửi message:cancel, KHÔNG gửi message:confirm", async () => {
      const wrapper = await mountVoiBanNhap();
      const socket = mockSockets[mockSockets.length - 1];
      socket.send.mockClear();

      await wrapper.find(".draft-btn-cancel").trigger("click");

      const events = socket.send.mock.calls.map(([raw]: [string]) => JSON.parse(raw).event);
      expect(events).toContain("message:cancel");
      expect(events).not.toContain("message:confirm");
      wrapper.unmount();
    });

    /** Bấm hai lần không được gửi hai lần — lõi cũng chặn, đây là lớp thứ hai. */
    it("bấm xác nhận hai lần chỉ gửi một lệnh", async () => {
      const wrapper = await mountVoiBanNhap();
      const socket = mockSockets[mockSockets.length - 1];
      socket.send.mockClear();

      const nut = wrapper.find(".draft-btn-send");
      await nut.trigger("click");
      await nut.trigger("click");

      const goiDi = socket.send.mock.calls
        .map(([raw]: [string]) => JSON.parse(raw))
        .filter((m: { event: string }) => m.event === "message:confirm");
      expect(goiDi).toHaveLength(1);
      wrapper.unmount();
    });
  });
});
