import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { defineComponent, h, nextTick, ref } from 'vue';

// Mock Node url to prevent JSDOM path resolution crashes
vi.mock('url', async (importOriginal) => {
  const original = await importOriginal<typeof import('url')>();
  return {
    ...original,
    fileURLToPath: () => 'C:\\dummy',
  };
});

// Mock platform
vi.mock('../../src/platform/index', () => ({
  detectPlatform: () => ({
    platformName: 'web',
    getWindowSize: () => Promise.resolve({ width: 800, height: 600 }),
    toggleGhostMode: vi.fn(),
    minimizeToTray: vi.fn(),
    quitApp: vi.fn(),
    hasVaultSecret: vi.fn(),
    storeVaultSecret: vi.fn(),
    deleteVaultSecret: vi.fn(),
    onGatewayReady: vi.fn(),
    invokeBackend: vi.fn().mockResolvedValue(null),
  }),
}));

// Mock useGateway
vi.mock('../../src/composables/useGateway', () => ({
  useGateway: () => ({
    userProfile: ref({ name: 'User', language: 'vi-VN' }),
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
vi.mock('../../src/composables/useI18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
    currentLang: ref('vi-VN'),
  }),
}));

const voiceMock = {
  state: ref('OFF'),
  isReady: ref(false),
  pipelineState: ref('IDLE'),
  isSpeaking: ref(false),
  transcript: ref(''),
  aiResponse: ref(''),
  audioLevel: ref(0),
  volumeLevel: ref(0),
  wakeWordThreshold: ref(0.5),
  wakeProbeFeedback: ref({ outcome: 'idle', score: null, transcript: '' }),
  diagnosticsPanelRef: ref(null),
  isSupported: ref(true),
  pipelineError: ref(''),
  pipelineErrorKind: ref('none'),
  startPipeline: vi.fn().mockResolvedValue(undefined),
  stopPipeline: vi.fn().mockResolvedValue(undefined),
  setPassive: vi.fn(),
  setProcessing: vi.fn(),
  keepAlive: vi.fn(),
  setLanguage: vi.fn(),
  setTtsVoice: vi.fn(),
  setWakeWordThreshold: vi.fn(),
  toggleVoice: vi.fn(),
  onWakeWordDetected: vi.fn(),
  muteWakeWord: vi.fn(),
  unmuteWakeWord: vi.fn(),
  muteWakeWordFor: vi.fn(),
  activateWebSpeechFallback: vi.fn(),
  deactivateWebSpeechFallback: vi.fn(),
};

// Mock useVoicePipeline
vi.mock('../../src/composables/useVoicePipeline', () => ({
  useVoicePipeline: () => voiceMock,
}));

const speakerMock = {
  stop: vi.fn(),
  flush: vi.fn(),
  close: vi.fn(),
  unblock: vi.fn(),
  isBlocked: vi.fn(() => false),
  isPlaying: vi.fn(() => false),
  hasActiveSources: vi.fn(() => false),
  setMasterVolume: vi.fn(),
  enqueueSpeakerPayload: vi.fn(),
  enqueueEncodedAudio: vi.fn().mockResolvedValue(undefined),
};

const avatarEngineMock = {
  setExpression: vi.fn(),
  playGesture: vi.fn(),
  moveTo: vi.fn(),
  inspectScreenPoint: vi.fn(),
  clearInspection: vi.fn(),
  jump: vi.fn(),
  stopMoving: vi.fn(),
  setWander: vi.fn(),
  setThinking: vi.fn(),
  triggerMotion: vi.fn(),
  getScreenBounds: vi.fn(() => null),
};

const AvatarEngineStub = defineComponent({
  name: 'VRMEngine',
  setup(_, { expose }) {
    expose(avatarEngineMock);
    return () => h('div', { 'data-testid': 'avatar-engine' });
  },
});

vi.mock('../../src/composables/useSpeakerPlayback', () => ({
  useSpeakerPlayback: () => speakerMock,
}));

// Mock use3DModel
vi.mock('../../src/composables/use3DModel', () => ({
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
vi.mock('../../src/composables/useFaceTracking', () => ({
  useFaceTracking: () => ({
    isActive: ref(false),
    startTracking: vi.fn(),
    stopTracking: vi.fn(),
    expressions: ref({}),
    lookAt: ref({ x: 0, y: 0, z: 0 }),
  }),
}));

import WidgetApp from '../../src/WidgetApp.vue';

const mockSockets: MockWebSocket[] = [];

class MockWebSocket {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSED = 3;

  readyState = MockWebSocket.CONNECTING;
  binaryType: BinaryType = 'blob';
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

describe('WidgetApp.vue', () => {
  beforeEach(() => {
    mockSockets.length = 0;
    vi.clearAllMocks();
    voiceMock.state.value = 'OFF';
    voiceMock.isReady.value = false;
    voiceMock.pipelineError.value = '';
    voiceMock.wakeProbeFeedback.value = { outcome: 'idle', score: null, transcript: '' };
    vi.stubGlobal('WebSocket', MockWebSocket);
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it('should mount and render widget layout', () => {
    const wrapper = mount(WidgetApp, {
      global: {
        provide: {
          platform: {
            platformName: 'web',
            getWindowSize: () => Promise.resolve({ width: 800, height: 600 }),
            toggleGhostMode: vi.fn(),
            minimizeToTray: vi.fn(),
            quitApp: vi.fn(),
            hasVaultSecret: vi.fn(),
            storeVaultSecret: vi.fn(),
            deleteVaultSecret: vi.fn(),
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

  it('renders the tool panel shell when tool state is present', async () => {
    const wrapper = mount(WidgetApp, {
      global: {
        provide: {
          platform: { platformName: 'web', invokeBackend: vi.fn().mockResolvedValue(null) },
        },
        stubs: {
          Live2DEngine: true,
          VRMEngine: true,
          VisionSensor: true,
          ResourceMeter: true,
        },
      },
    });
    const vm = wrapper.vm as unknown as {
      toolPanel: { tool: string; state: 'loading' | 'done' | 'error'; payload: unknown } | null;
    };
    vm.toolPanel = {
      tool: 'get_weather',
      state: 'loading',
      payload: null,
    };
    await nextTick();

    expect(wrapper.find('.tool-panel').exists()).toBe(true);
    wrapper.unmount();
  });

  it('removes the tool panel when its close button is pressed', async () => {
    const wrapper = mount(WidgetApp, {
      global: {
        provide: {
          platform: { platformName: 'web', invokeBackend: vi.fn().mockResolvedValue(null) },
        },
        stubs: {
          Live2DEngine: true,
          VRMEngine: true,
          VisionSensor: true,
          ResourceMeter: true,
        },
      },
    });
    const vm = wrapper.vm as unknown as {
      toolPanel: { tool: string; state: 'loading' | 'done' | 'error'; payload: unknown } | null;
    };
    vm.toolPanel = { tool: 'get_weather', state: 'loading', payload: null };
    await nextTick();

    await wrapper.get('button[aria-label="Đóng bảng công cụ"]').trigger('click');
    expect(vm.toolPanel).toBeNull();
    expect(wrapper.find('.tool-panel').exists()).toBe(false);
    wrapper.unmount();
  });

  it('registers the tool panel as interactive only while it is visible', async () => {
    const invokeBackend = vi.fn().mockResolvedValue(null);
    const wrapper = mount(WidgetApp, {
      global: {
        provide: {
          platform: { platformName: 'web', invokeBackend },
        },
        stubs: {
          Live2DEngine: true,
          VRMEngine: true,
          VisionSensor: true,
          ResourceMeter: true,
        },
      },
    });
    const vm = wrapper.vm as unknown as {
      toolPanel: { tool: string; state: 'loading' | 'done' | 'error'; payload: unknown } | null;
      updateInteractiveZones: () => void;
    };

    vm.toolPanel = { tool: 'get_weather', state: 'loading', payload: null };
    await nextTick();

    const panelZone = wrapper.get('.tool-panel-zone').element as HTMLElement;
    vi.spyOn(panelZone, 'getBoundingClientRect').mockReturnValue({
      x: 24,
      y: 100,
      left: 24,
      top: 100,
      right: 344,
      bottom: 280,
      width: 320,
      height: 180,
      toJSON: () => ({}),
    });
    invokeBackend.mockClear();
    vm.updateInteractiveZones();

    expect(invokeBackend).toHaveBeenLastCalledWith('update_interactive_zones', {
      zones: expect.arrayContaining([{ x: 24, y: 100, width: 320, height: 180 }]),
    });

    invokeBackend.mockClear();
    await wrapper.get('button[aria-label="Đóng bảng công cụ"]').trigger('click');
    await nextTick();

    expect(invokeBackend).toHaveBeenCalledWith('update_interactive_zones', {
      zones: expect.not.arrayContaining([{ x: 24, y: 100, width: 320, height: 180 }]),
    });
    wrapper.unmount();
  });

  it('hiển thị kết quả probe gần nhất trong diagnostics thay vì im lặng', async () => {
    voiceMock.wakeProbeFeedback.value = {
      outcome: 'rejected',
      score: 0.372,
      transcript: '',
    };
    const wrapper = mount(WidgetApp, {
      global: {
        provide: {
          platform: { platformName: 'web', invokeBackend: vi.fn().mockResolvedValue(null) },
        },
        stubs: { Live2DEngine: true, VRMEngine: true, VisionSensor: true },
      },
    });
    const vm = wrapper.vm as unknown as { isCollapsed: boolean; showDiagnostics: boolean };
    vm.isCollapsed = false;
    vm.showDiagnostics = true;
    await nextTick();

    const feedback = wrapper.find('[data-testid="wake-probe-feedback"]');
    expect(feedback.exists()).toBe(true);
    expect(feedback.text()).toContain('Đã nghe nhưng chưa khớp');
    expect(feedback.text()).toContain('37.2%');
    expect(feedback.text()).toContain('STT không ra chữ');
    wrapper.unmount();
  });

  it('should reconnect the gateway after an unexpected socket close', async () => {
    vi.useFakeTimers();
    const wrapper = mount(WidgetApp, {
      global: {
        provide: {
          platform: {
            platformName: 'web',
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
    mockSockets[0].onclose?.(new CloseEvent('close'));
    await vi.advanceTimersByTimeAsync(500);

    expect(mockSockets).toHaveLength(2);

    wrapper.unmount();
    mockSockets[1].onclose?.(new CloseEvent('close'));
    await vi.advanceTimersByTimeAsync(5_000);
    expect(mockSockets).toHaveLength(2);
  });

  it('walks to and inspects the tool panel when tool_start arrives', async () => {
    vi.useFakeTimers();
    const wrapper = mount(WidgetApp, {
      global: {
        provide: {
          platform: { platformName: 'web', invokeBackend: vi.fn().mockResolvedValue(null) },
        },
        stubs: {
          Live2DEngine: true,
          VRMEngine: AvatarEngineStub,
          VisionSensor: true,
          ResourceMeter: true,
        },
      },
    });
    await vi.advanceTimersByTimeAsync(0);
    const socket = mockSockets[0];
    socket.readyState = MockWebSocket.OPEN;
    socket.onopen?.(new Event('open'));

    await socket.onmessage?.({
      data: JSON.stringify({ event: 'ai_thinking_start', payload: {} }),
    } as MessageEvent);
    await nextTick();
    expect(avatarEngineMock.setThinking).toHaveBeenLastCalledWith(true);
    await socket.onmessage?.({
      data: JSON.stringify({
        event: 'tool_start',
        payload: { tool: 'get_weather', label: 'Đang xem thời tiết…' },
      }),
    } as MessageEvent);
    await nextTick();
    await nextTick();

    const vm = wrapper.vm as unknown as {
      toolPanel: { tool: string; state: string; payload: unknown } | null;
    };
    expect(vm.toolPanel).toEqual({
      tool: 'get_weather',
      state: 'loading',
      payload: { label: 'Đang xem thời tiết…' },
    });
    expect(avatarEngineMock.setWander).toHaveBeenLastCalledWith(false);
    expect(avatarEngineMock.moveTo).toHaveBeenCalledWith(expect.any(Number), 0.96, { run: false });
    expect(avatarEngineMock.inspectScreenPoint).toHaveBeenCalledWith(
      expect.any(Number),
      expect.any(Number)
    );
    await socket.onmessage?.({
      data: JSON.stringify({ event: 'ai_thinking_end', payload: {} }),
    } as MessageEvent);
    await nextTick();
    expect(avatarEngineMock.setThinking).toHaveBeenLastCalledWith(false);
    expect(avatarEngineMock.setWander).toHaveBeenLastCalledWith(false);
    wrapper.unmount();
  });

  it('shows the tool result and turns back before speaking', async () => {
    vi.useFakeTimers();
    const wrapper = mount(WidgetApp, {
      global: {
        provide: {
          platform: { platformName: 'web', invokeBackend: vi.fn().mockResolvedValue(null) },
        },
        stubs: {
          Live2DEngine: true,
          VRMEngine: AvatarEngineStub,
          VisionSensor: true,
          ResourceMeter: true,
        },
      },
    });
    await vi.advanceTimersByTimeAsync(0);
    const socket = mockSockets[0];
    socket.readyState = MockWebSocket.OPEN;

    const emit = async (event: string, payload: Record<string, unknown>) => {
      await socket.onmessage?.({ data: JSON.stringify({ event, payload }) } as MessageEvent);
      await nextTick();
    };
    await emit('tool_start', { tool: 'get_weather', label: 'Đang xem thời tiết…' });
    avatarEngineMock.clearInspection.mockClear();
    speakerMock.unblock.mockClear();
    await emit('tool_result', {
      tool: 'get_weather',
      ok: true,
      data: {
        content: [{ type: 'text', text: 'Hà Nội: 31°C, có mây, độ ẩm 70%.' }],
        isError: false,
      },
    });

    const vm = wrapper.vm as unknown as { toolPanel: { state: string; payload: unknown } | null };
    expect(vm.toolPanel?.state).toBe('done');
    expect(wrapper.get('.tool-panel__weather').text()).toContain('Hà Nội');
    expect(avatarEngineMock.clearInspection).toHaveBeenCalledTimes(1);
    expect(speakerMock.unblock).not.toHaveBeenCalled();

    await emit('ai_spoken_response', { text: 'Hà Nội đang có mây.' });
    expect(speakerMock.unblock).toHaveBeenCalledTimes(1);
    expect(avatarEngineMock.clearInspection.mock.invocationCallOrder[0]).toBeLessThan(
      speakerMock.unblock.mock.invocationCallOrder[0]
    );
    wrapper.unmount();
  });

  it('exits loading with a shake on tool error or after thirty seconds', async () => {
    vi.useFakeTimers();
    const wrapper = mount(WidgetApp, {
      global: {
        provide: {
          platform: { platformName: 'web', invokeBackend: vi.fn().mockResolvedValue(null) },
        },
        stubs: {
          Live2DEngine: true,
          VRMEngine: AvatarEngineStub,
          VisionSensor: true,
          ResourceMeter: true,
        },
      },
    });
    await vi.advanceTimersByTimeAsync(0);
    const socket = mockSockets[0];
    socket.readyState = MockWebSocket.OPEN;
    const emit = async (event: string, payload: Record<string, unknown>) => {
      await socket.onmessage?.({ data: JSON.stringify({ event, payload }) } as MessageEvent);
      await nextTick();
    };

    await emit('tool_start', { tool: 'get_weather', label: 'Đang xem thời tiết…' });
    await emit('tool_result', { tool: 'get_weather', ok: false, reason: 'Mất kết nối.' });
    let vm = wrapper.vm as unknown as { toolPanel: { state: string; payload: unknown } | null };
    expect(vm.toolPanel).toEqual({
      tool: 'get_weather',
      state: 'error',
      payload: { message: 'Mất kết nối.' },
    });
    expect(avatarEngineMock.playGesture).toHaveBeenLastCalledWith('shake');

    await emit('tool_start', { tool: 'get_weather', label: 'Đang xem thời tiết…' });
    await vi.advanceTimersByTimeAsync(29_999);
    vm = wrapper.vm as unknown as { toolPanel: { state: string; payload: unknown } | null };
    expect(vm.toolPanel?.state).toBe('loading');
    await vi.advanceTimersByTimeAsync(1);
    expect(vm.toolPanel?.state).toBe('error');
    expect(wrapper.get('.tool-panel__error').text()).toContain('30 giây');
    expect(avatarEngineMock.clearInspection).toHaveBeenCalled();
    expect(avatarEngineMock.playGesture).toHaveBeenLastCalledWith('shake');
    wrapper.unmount();
  });

  it('executes split avatar tags without leaking them or adding an unrequested nod', async () => {
    vi.useFakeTimers();
    const wrapper = mount(WidgetApp, {
      global: {
        provide: {
          platform: { platformName: 'web', invokeBackend: vi.fn().mockResolvedValue(null) },
        },
        stubs: {
          Live2DEngine: true,
          VRMEngine: AvatarEngineStub,
          VisionSensor: true,
          ResourceMeter: true,
        },
      },
    });
    await vi.advanceTimersByTimeAsync(0);
    const socket = mockSockets[0];
    socket.readyState = MockWebSocket.OPEN;
    socket.onopen?.(new Event('open'));

    const emit = async (event: string, payload: Record<string, unknown> = {}) => {
      await socket.onmessage?.({
        data: JSON.stringify({ event, payload }),
      } as MessageEvent);
      await nextTick();
    };

    await emit('ai_thinking_start');
    await emit('ai_stream_start');
    await emit('ai_stream_chunk', { textChunk: '[wa', isThought: false });
    expect(avatarEngineMock.playGesture).not.toHaveBeenCalled();
    await emit('ai_stream_chunk', { textChunk: 've]Xin chào', isThought: false });
    await emit('ai_spoken_response', { text: '[wave]Xin chào' });

    const vm = wrapper.vm as unknown as { messages: Array<{ text: string }> };
    expect(vm.messages.at(-1)?.text).toBe('Xin chào');
    expect(avatarEngineMock.playGesture).toHaveBeenCalledTimes(1);
    expect(avatarEngineMock.playGesture).toHaveBeenCalledWith('wave');
    expect(avatarEngineMock.moveTo).not.toHaveBeenCalled();

    await emit('ai_thinking_start');
    await emit('ai_stream_start');
    await emit('ai_stream_chunk', { textChunk: '2 + 2 bằng 4.', isThought: false });
    await emit('ai_spoken_response', { text: '2 + 2 bằng 4.' });

    expect(avatarEngineMock.playGesture).toHaveBeenCalledTimes(1);
    wrapper.unmount();
  });

  it('maps each supported avatar control independently', async () => {
    vi.useFakeTimers();
    const wrapper = mount(WidgetApp, {
      global: {
        provide: {
          platform: { platformName: 'web', invokeBackend: vi.fn().mockResolvedValue(null) },
        },
        stubs: {
          Live2DEngine: true,
          VRMEngine: AvatarEngineStub,
          VisionSensor: true,
          ResourceMeter: true,
        },
      },
    });
    await vi.advanceTimersByTimeAsync(0);
    const socket = mockSockets[0];
    socket.readyState = MockWebSocket.OPEN;

    const emit = async (event: string, payload: Record<string, unknown> = {}) => {
      await socket.onmessage?.({
        data: JSON.stringify({ event, payload }),
      } as MessageEvent);
      await nextTick();
    };

    await emit('ai_stream_start');
    await emit('ai_stream_chunk', {
      textChunk: '[happy][nod][jump][come_closer][step_back]Xong',
      isThought: false,
    });

    expect(avatarEngineMock.setExpression).toHaveBeenCalledWith('happy');
    expect(avatarEngineMock.playGesture).toHaveBeenCalledWith('nod');
    expect(avatarEngineMock.jump).toHaveBeenCalledTimes(1);
    expect(avatarEngineMock.moveTo).toHaveBeenNthCalledWith(1, 0.55, 0.9, { run: false });
    expect(avatarEngineMock.moveTo).toHaveBeenNthCalledWith(2, 0.88, 1, { run: false });

    await emit('ai_stream_start');
    await emit('ai_stream_chunk', {
      textChunk: '[anim:201]Chào bằng animation ID',
      isThought: false,
    });
    expect(avatarEngineMock.playGesture).toHaveBeenLastCalledWith('wave');
    wrapper.unmount();
  });

  it('xin session ticket Tauri trước khi mở WebSocket đặc quyền', async () => {
    const invokeBackend = vi.fn().mockResolvedValue({
      token: 'a'.repeat(64),
      expires_in_ms: 30_000,
    });
    const wrapper = mount(WidgetApp, {
      global: {
        provide: {
          platform: {
            platformName: 'tauri',
            invokeBackend,
          },
        },
        stubs: {
          Live2DEngine: true,
          VRMEngine: true,
          VisionSensor: true,
          ResourceMeter: true,
        },
      },
    });

    await vi.waitFor(() => expect(mockSockets).toHaveLength(1));

    expect(invokeBackend).toHaveBeenCalledWith('issue_websocket_session');
    expect(mockSockets[0].url).toBe(`ws://127.0.0.1:8002/ws?session=${'a'.repeat(64)}`);

    wrapper.unmount();
  });

  it('thực thi contract tương tác và các event gateway chính', async () => {
    vi.useFakeTimers();
    const invokeBackend = vi.fn().mockResolvedValue(null);
    const wrapper = mount(WidgetApp, {
      global: {
        provide: {
          platform: { platformName: 'web', invokeBackend },
        },
        stubs: {
          Live2DEngine: true,
          VRMEngine: true,
          VisionSensor: true,
          ResourceMeter: true,
        },
      },
    });
    await vi.advanceTimersByTimeAsync(0);
    const socket = mockSockets[0];
    socket.readyState = MockWebSocket.OPEN;
    socket.onopen?.(new Event('open'));
    await Promise.resolve();

    const bootstrapEvents = socket.send.mock.calls.map(([raw]: [string]) => JSON.parse(raw).event);
    expect(bootstrapEvents).toEqual(
      expect.arrayContaining([
        'get_config',
        'get_avatar_models',
        'get_user_profile',
        'message:pending',
      ])
    );
    expect(voiceMock.startPipeline).toHaveBeenCalled();

    const vm = wrapper.vm as any;
    vm.toggleTheme();
    expect(document.documentElement.getAttribute('data-theme')).toBe('light');

    vm.isCollapsed = false;
    vm.inputText = 'xin chào LIVA';
    await wrapper.vm.$nextTick();
    await vi.advanceTimersByTimeAsync(500);
    expect(socket.send.mock.calls.map(([raw]: [string]) => JSON.parse(raw).event)).not.toContain(
      'user_typing'
    );

    vm.sendMessage();
    expect(vm.messages.at(-1)).toMatchObject({ role: 'user', text: 'xin chào LIVA' });
    expect(socket.send.mock.calls.map(([raw]: [string]) => JSON.parse(raw).event)).toContain(
      'user_voice_command'
    );
    expect(socket.send.mock.calls.map(([raw]: [string]) => JSON.parse(raw).event)).not.toContain(
      'user_typing_cancelled'
    );

    (window as any).sendLIVAMessage('mở dashboard');
    vm.openDashboard();
    expect(invokeBackend).toHaveBeenCalledWith('open_dashboard');

    const emit = async (event: string, payload: Record<string, unknown> = {}) => {
      await socket.onmessage?.({
        data: JSON.stringify({ event, payload }),
      } as MessageEvent);
      await wrapper.vm.$nextTick();
    };

    await emit('config_data', {
      ui: { avatarMode: '2D', activeModel: { filename: 'models/live2d/a.json', type: '2d' } },
    });
    expect(vm.engineMode).toBe('2D');
    expect(vm.activeModelConfig.filename).toContain('live2d');

    await emit('user_profile', { name: 'Hien', language: 'vi-VN' });
    await emit('eco_mode_changed', { enabled: true });
    expect((window as any).LIVA_ECO_MODE).toBe(true);
    for (const level of ['eco', 'freeze', 'preempted']) {
      await emit('avatar_demote', { level, fps: 5 });
      expect((window as any).LIVA_AVATAR_DEMOTE_LEVEL).toBe(level);
    }
    await emit('avatar_restore');
    expect((window as any).LIVA_AVATAR_DEMOTE_LEVEL).toBe('normal');

    await emit('stt_fallback_activated');
    await emit('stt_fallback_deactivated');
    expect(voiceMock.activateWebSpeechFallback).toHaveBeenCalled();
    expect(voiceMock.deactivateWebSpeechFallback).toHaveBeenCalled();

    await emit('ai_thinking_start');
    expect(vm.isThinking).toBe(true);
    await emit('ai_thinking_end');
    expect(vm.isThinking).toBe(false);
    await emit('ai_stream_start');
    await emit('ai_stream_chunk', { textChunk: '<thought>kế hoạch</thought>', isThought: true });
    await emit('ai_stream_chunk', { textChunk: '[happy]Xin chào\nbạn', isThought: false });
    expect(vm.messages.at(-1).text).toContain('Xin chào');
    await emit('ai_spoken_response', { text: 'Kết quả cuối' });
    expect(vm.messages.at(-1).text).toContain('Kết quả cuối');
    expect(voiceMock.keepAlive).toHaveBeenCalled();

    await emit('message:pending_response', {
      drafts: [
        {
          draft_id: 'dr_event',
          platform: 'telegram',
          display_name: 'Minh',
          handle: '123',
          text: 'hello',
        },
      ],
    });
    expect(vm.pendingDraft.draft_id).toBe('dr_event');
    await emit('message:confirm_error', { error: 'hết hạn' });
    expect(vm.messages.at(-1).text).toContain('hết hạn');
    await emit('message:confirm_response', { detail: 'đã gửi' });
    expect(vm.pendingDraft).toBeNull();
    expect(vm.messages.at(-1).text).toContain('đã gửi');
    await emit('message:cancel_response');
    await emit('message:pending_error');

    await emit('audio_ducking', { volume: 0.25 });
    expect(speakerMock.setMasterVolume).toHaveBeenCalledWith(0.25);
    await emit('ai_audio_chunk', { audio: btoa('abcd') });
    expect(speakerMock.enqueueEncodedAudio).toHaveBeenCalled();

    await socket.onmessage?.({ data: '[INTERRUPT]' } as MessageEvent);
    await socket.onmessage?.({ data: '{not-json' } as MessageEvent);
    expect(speakerMock.stop).toHaveBeenCalled();

    vm.onDragStart(new MouseEvent('mousedown', { clientX: 10, clientY: 20 }));
    vm.onDragMove(new MouseEvent('mousemove', { clientX: 40, clientY: 60 }));
    vm.onDragEnd();
    expect(['left', 'right']).toContain(vm.snapPosition);
    vm.toggleCollapse();
    vm.snapToEdge();
    vm.interruptLIVA();
    expect(socket.send).toHaveBeenCalledWith('[INTERRUPT]');

    wrapper.unmount();
  });

  it('thực thi các điều khiển chat mở rộng qua DOM', async () => {
    vi.useFakeTimers();
    const wrapper = mount(WidgetApp, {
      global: {
        provide: {
          platform: { platformName: 'web', invokeBackend: vi.fn().mockResolvedValue(null) },
        },
        stubs: {
          Live2DEngine: true,
          VRMEngine: AvatarEngineStub,
          VisionSensor: true,
          ResourceMeter: true,
        },
      },
    });
    await vi.advanceTimersByTimeAsync(0);
    const socket = mockSockets[0];
    socket.readyState = MockWebSocket.OPEN;
    socket.onopen?.(new Event('open'));

    const vm = wrapper.vm as any;
    vm.isCollapsed = false;
    await nextTick();

    const input = wrapper.get('input[type="text"]');
    await input.setValue('  kiểm tra DOM  ');
    await input.trigger('keyup.enter');
    expect(socket.send.mock.calls.map(([raw]: [string]) => JSON.parse(raw))).toContainEqual({
      event: 'user_voice_command',
      payload: { text: 'kiểm tra DOM' },
    });
    expect((input.element as HTMLInputElement).value).toBe('');

    await wrapper.get('button[title="Diagnostics"]').trigger('click');
    expect(vm.showDiagnostics).toBe(true);
    await wrapper.get('input[type="range"]').setValue('0.025');
    expect(voiceMock.setWakeWordThreshold).toHaveBeenCalledWith(0.025);

    voiceMock.state.value = 'PASSIVE';
    await wrapper.get('button[title="wg_start_mic"]').trigger('click');
    expect(voiceMock.toggleVoice).toHaveBeenCalled();

    vm.isThinking = true;
    await nextTick();
    await wrapper.get('button[title="wg_interrupt"]').trigger('click');
    expect(socket.send).toHaveBeenCalledWith('[INTERRUPT]');
    expect(avatarEngineMock.triggerMotion).toHaveBeenCalled();
    wrapper.unmount();
  });

  it('phát phản hồi wake word và force trigger qua pipeline đang mở', async () => {
    const oscillator = {
      connect: vi.fn(),
      type: '',
      frequency: { value: 0 },
      start: vi.fn(),
      stop: vi.fn(),
    };
    const gain = {
      connect: vi.fn(),
      gain: {
        setValueAtTime: vi.fn(),
        linearRampToValueAtTime: vi.fn(),
        exponentialRampToValueAtTime: vi.fn(),
      },
    };
    class FakeAudioContext {
      state = 'suspended';
      currentTime = 1;
      destination = {};
      resume = vi.fn();
      createOscillator = vi.fn(() => oscillator);
      createGain = vi.fn(() => gain);
    }
    vi.stubGlobal('AudioContext', FakeAudioContext);

    const wrapper = mount(WidgetApp, {
      global: {
        provide: {
          platform: { platformName: 'web', invokeBackend: vi.fn().mockResolvedValue(null) },
        },
        stubs: {
          Live2DEngine: true,
          VRMEngine: true,
          VisionSensor: true,
          ResourceMeter: true,
        },
      },
    });
    await vi.waitFor(() => expect(mockSockets).toHaveLength(1));
    const socket = mockSockets[0];
    socket.readyState = MockWebSocket.OPEN;

    const wakeCallback = voiceMock.onWakeWordDetected.mock.calls.at(-1)?.[0] as () => void;
    wakeCallback();
    expect(voiceMock.muteWakeWordFor).toHaveBeenCalledWith(750);
    expect(oscillator.start).toHaveBeenCalledTimes(2);

    voiceMock.state.value = 'PASSIVE';
    await (wrapper.vm as any).forceTriggerWakeWord();
    expect(voiceMock.state.value).toBe('ACTIVE');
    // Đảo chiều khẳng định ngày 16/08/2026. Test cũ đòi widget PHẢI gửi
    // `wake_word_triggered` lên gateway. Lời gọi đó không đi tới đâu cả:
    // backend không có handler (không nằm trong danh sách nào của
    // `authorization.rs`), gateway không broadcast, và `wake_word_triggered`
    // thực chất là event *server→client* — lõi báo lên kèm `{ score, transcript }`
    // và UI nhận ở `useVoicePipeline.ts:458`. Nay khoá theo chiều ngược lại để
    // không ai nối lại nhầm.
    expect(socket.send.mock.calls.map(([raw]: [string]) => JSON.parse(raw).event)).not.toContain(
      'wake_word_triggered'
    );
    expect((wrapper.vm as any).messages.at(-1).text).toBe('wg_wake_word_ack');
    wrapper.unmount();
  });

  it('xử lý phím sensory capture và dọn timer khi unmount', async () => {
    vi.useFakeTimers();
    const fetchMock = vi.fn().mockResolvedValue(new Response(null, { status: 204 }));
    vi.stubGlobal('fetch', fetchMock);
    const wrapper = mount(WidgetApp, {
      global: {
        provide: {
          platform: { platformName: 'web', invokeBackend: vi.fn().mockResolvedValue(null) },
        },
        stubs: {
          Live2DEngine: true,
          VRMEngine: true,
          VisionSensor: true,
          ResourceMeter: true,
        },
      },
    });

    globalThis.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'S', ctrlKey: true, shiftKey: true })
    );
    await vi.advanceTimersByTimeAsync(0);
    expect(fetchMock).toHaveBeenCalledWith(
      'http://127.0.0.1:3000/api/sensory-capture',
      expect.objectContaining({ method: 'POST' })
    );
    expect((wrapper.vm as any).isSensing).toBe(true);

    wrapper.unmount();
    await vi.runAllTimersAsync();
    expect((wrapper.vm as any).isSensing).toBe(true);
  });

  /**
   * Thẻ xác nhận gửi tin — thứ DUY NHẤT trong widget gây ra hành động không
   * hoàn tác được. Test dựng thẳng bản nháp vào state rồi đọc DOM, vì lõi thật
   * mới là nơi sinh ra nó và ở đây lõi đã bị mock.
   */
  describe('thẻ xác nhận gửi tin nhắn', () => {
    const banNhap = {
      draft_id: 'dr_1',
      platform: 'telegram',
      display_name: 'Minh Hiến',
      handle: '123456789',
      text: 'ngủ đi',
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
            platform: { platformName: 'web', invokeBackend: vi.fn().mockResolvedValue(null) },
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

    it('không có bản nháp thì không có thẻ, dù thanh chat đã mở', async () => {
      const wrapper = await mountWidget(null);
      // Thanh chat có dựng thật — nếu không, khẳng định dưới vô nghĩa.
      expect(wrapper.find('.chat-capsule').exists()).toBe(true);
      expect(wrapper.find('.draft-card').exists()).toBe(false);
      wrapper.unmount();
    });

    it('hiện cả tên lẫn địa chỉ đích, và nói rõ CHƯA gửi', async () => {
      const wrapper = await mountVoiBanNhap();
      const the = wrapper.find('.draft-card');
      expect(the.exists()).toBe(true);
      expect(the.text()).toContain('Minh Hiến');
      // Địa chỉ đích phải hiện: tên đúng mà số sai vẫn là gửi nhầm người.
      expect(the.text()).toContain('123456789');
      expect(the.text()).toContain('ngủ đi');
      expect(the.text()).toContain('telegram');
      // `useI18n` bị mock trả về chính key, nên khẳng định theo KEY. Chữ thật
      // ("Đã soạn tin — CHƯA gửi") được khoá ở `useI18n.ts`, không phải ở đây.
      expect(the.text()).toContain('wg_draft_title');
      expect(wrapper.find('.draft-btn-send').exists()).toBe(true);
      expect(wrapper.find('.draft-btn-cancel').exists()).toBe(true);
      wrapper.unmount();
    });

    it('bấm xác nhận gửi message:confirm kèm đúng draftId', async () => {
      const wrapper = await mountVoiBanNhap();
      const socket = mockSockets[mockSockets.length - 1];
      socket.send.mockClear();

      await wrapper.find('.draft-btn-send').trigger('click');

      const goiDi = socket.send.mock.calls
        .map(([raw]: [string]) => JSON.parse(raw))
        .filter((m: { event: string }) => m.event === 'message:confirm');
      expect(goiDi).toHaveLength(1);
      expect(goiDi[0].payload.draftId).toBe('dr_1');
      wrapper.unmount();
    });

    it('bấm huỷ gửi message:cancel, KHÔNG gửi message:confirm', async () => {
      const wrapper = await mountVoiBanNhap();
      const socket = mockSockets[mockSockets.length - 1];
      socket.send.mockClear();

      await wrapper.find('.draft-btn-cancel').trigger('click');

      const events = socket.send.mock.calls.map(([raw]: [string]) => JSON.parse(raw).event);
      expect(events).toContain('message:cancel');
      expect(events).not.toContain('message:confirm');
      wrapper.unmount();
    });

    /** Bấm hai lần không được gửi hai lần — lõi cũng chặn, đây là lớp thứ hai. */
    it('bấm xác nhận hai lần chỉ gửi một lệnh', async () => {
      const wrapper = await mountVoiBanNhap();
      const socket = mockSockets[mockSockets.length - 1];
      socket.send.mockClear();

      const nut = wrapper.find('.draft-btn-send');
      await nut.trigger('click');
      await nut.trigger('click');

      const goiDi = socket.send.mock.calls
        .map(([raw]: [string]) => JSON.parse(raw))
        .filter((m: { event: string }) => m.event === 'message:confirm');
      expect(goiDi).toHaveLength(1);
      wrapper.unmount();
    });
  });

  describe('Lifecycle KeepAlive', () => {
    it('kích hoạt onActivated và onDeactivated', async () => {
      const App = {
        components: { WidgetApp },
        template: '<keep-alive><WidgetApp v-if="show" /></keep-alive>',
        data() { return { show: true }; }
      };
      const wrapper = mount(App, {
        global: {
        provide: {
          platform: {
            platformName: 'web',
            getWindowSize: () => Promise.resolve({ width: 800, height: 600 }),
            toggleGhostMode: vi.fn(),
            minimizeToTray: vi.fn(),
            quitApp: vi.fn(),
            hasVaultSecret: vi.fn(),
            storeVaultSecret: vi.fn(),
            deleteVaultSecret: vi.fn(),
            invokeBackend: vi.fn(() => Promise.resolve()),
          },
        },
        }
      });
      await nextTick();
      await wrapper.setData({ show: false });
      wrapper.unmount();
    });
  });
});
