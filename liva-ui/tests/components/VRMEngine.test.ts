import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { ref } from 'vue';

const initRendererMock = vi.fn();
const loadModelMock = vi.fn().mockResolvedValue(true);
const loadAnimationClipsMock = vi.fn().mockResolvedValue({ loaded: [], failures: {} });
const startRenderLoopMock = vi.fn();
const setFrameUpdateMock = vi.fn();
const startAutoBlinkMock = vi.fn();
const startLipSyncMock = vi.fn();
const stopLipSyncMock = vi.fn();
const startAudioDrivenLipSyncMock = vi.fn();
const stopAudioDrivenLipSyncMock = vi.fn();
const triggerMotionMock = vi.fn();
const updateLookAtMock = vi.fn();
const lookAtScreenPointMock = vi.fn();
const setInspectingMock = vi.fn();
const setThinkingMock = vi.fn();
const setFacingMock = vi.fn();
const setScreenPositionMock = vi.fn();
const setLocomotionStateMock = vi.fn();
const updateExpressionsMock = vi.fn();
const setFaceTrackingActiveMock = vi.fn();
const disposeVRMMock = vi.fn();

vi.mock('../../src/composables/use3DModel', () => ({
  use3DModel: () => ({
    currentModelFormat: ref('vrm'),
    initRenderer: initRendererMock,
    loadModel: loadModelMock,
    loadAnimationClips: loadAnimationClipsMock,
    startRenderLoop: startRenderLoopMock,
    setFrameUpdate: setFrameUpdateMock,
    startAutoBlink: startAutoBlinkMock,
    startLipSync: startLipSyncMock,
    stopLipSync: stopLipSyncMock,
    startAudioDrivenLipSync: startAudioDrivenLipSyncMock,
    stopAudioDrivenLipSync: stopAudioDrivenLipSyncMock,
    triggerMotion: triggerMotionMock,
    updateLookAt: updateLookAtMock,
    lookAtScreenPoint: lookAtScreenPointMock,
    setInspecting: setInspectingMock,
    setThinking: setThinkingMock,
    setFacing: setFacingMock,
    setScreenPosition: setScreenPositionMock,
    setLocomotionState: setLocomotionStateMock,
    updateExpressions: updateExpressionsMock,
    setFaceTrackingActive: setFaceTrackingActiveMock,
    dispose: disposeVRMMock,
  }),
}));

const faceDataRef = ref<any>({ isDetected: false, head: { yaw: 0, pitch: 0 }, expressions: {} });
const isTrackingRef = ref(false);
const startTrackingMock = vi.fn();
const stopTrackingMock = vi.fn();
const captureFrameMock = vi.fn();

vi.mock('../../src/composables/useFaceTracking', () => ({
  useFaceTracking: () => ({
    faceData: faceDataRef,
    isTracking: isTrackingRef,
    startTracking: startTrackingMock,
    stopTracking: stopTrackingMock,
    captureFrame: captureFrameMock,
  }),
}));

import VRMEngine from '../../src/components/VRMEngine.vue';

describe('VRMEngine.vue', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
    faceDataRef.value = { isDetected: false, head: { yaw: 0, pitch: 0 }, expressions: {} };
    isTrackingRef.value = false;
  });

  it('should mount and initialize the 3D engine and load the default model', async () => {
    const wrapper = mount(VRMEngine, {
      props: {
        modelConfig: { filename: 'avatar.vrm' },
      },
    });

    expect(wrapper.exists()).toBe(true);
    expect(initRendererMock).toHaveBeenCalled();
    expect(loadModelMock).toHaveBeenCalledWith('/models/vrm/avatar.vrm');

    await flushPromises();
    expect(loadAnimationClipsMock).toHaveBeenCalledTimes(1);
    expect(startRenderLoopMock).toHaveBeenCalled();
    expect(startAutoBlinkMock).toHaveBeenCalled();
  });

  it('drives locomotion from the renderer frame callback instead of a second RAF loop', async () => {
    mount(VRMEngine, {
      props: { modelConfig: { filename: 'avatar.vrm' } },
    });
    await flushPromises();

    expect(setFrameUpdateMock).toHaveBeenCalledTimes(1);
    const update = setFrameUpdateMock.mock.calls[0][0] as (delta: number) => void;
    expect(update).toBeTypeOf('function');

    update(1 / 60);

    expect(setScreenPositionMock).toHaveBeenCalledTimes(1);
    expect(setLocomotionStateMock).toHaveBeenCalledTimes(1);
  });

  it('should resolve absolute/url modelConfig path', async () => {
    const wrapper = mount(VRMEngine, {
      props: {
        modelConfig: { filename: 'http://example.com/avatar.vrm' },
      },
    });
    expect(loadModelMock).toHaveBeenCalledWith('http://example.com/avatar.vrm');
  });

  it('should resolve windows absolute paths', async () => {
    const wrapper = mount(VRMEngine, {
      props: {
        modelConfig: { filename: 'C:\\models\\my-avatar.vrm' },
      },
    });
    expect(loadModelMock).toHaveBeenCalledWith('file:///C:/models/my-avatar.vrm');
  });

  it('should expose start/stop lip sync APIs', () => {
    const wrapper = mount(VRMEngine, {
      props: { modelConfig: { filename: 'avatar.vrm' } },
    });

    wrapper.vm.startLipSync();
    expect(startLipSyncMock).toHaveBeenCalled();

    wrapper.vm.stopLipSync();
    expect(stopLipSyncMock).toHaveBeenCalled();
  });

  it('should expose audio lip sync APIs', () => {
    const wrapper = mount(VRMEngine, {
      props: { modelConfig: { filename: 'avatar.vrm' } },
    });

    // Engine chỉ ĐỌC analyser do useSpeakerPlayback sở hữu; nó không được nhận
    // AudioContext hay source, vì nhận là mở lại cửa cho việc tự nối đồ thị âm
    // thanh — đúng hai lỗi U24 (nhân đôi tiếng, analyser bám chunk chưa phát).
    const mockAnalyser = {} as AnalyserNode;

    wrapper.vm.startAudioLipSync(mockAnalyser);
    expect(startAudioDrivenLipSyncMock).toHaveBeenCalledWith(mockAnalyser);

    wrapper.vm.stopAudioLipSync();
    expect(stopAudioDrivenLipSyncMock).toHaveBeenCalled();
  });

  it('should expose persistent inspection focus and clear it back to the user', () => {
    const wrapper = mount(VRMEngine, {
      props: { modelConfig: { filename: 'avatar.vrm' } },
    });

    wrapper.vm.inspectScreenPoint(0.2, 0.6);
    expect(setInspectingMock).toHaveBeenCalledWith(true);
    expect(lookAtScreenPointMock).toHaveBeenCalledWith(0.2, 0.6);

    wrapper.vm.clearInspection();
    expect(setInspectingMock).toHaveBeenCalledWith(false);
    expect(setFacingMock).toHaveBeenCalledWith(1, false);
    expect(updateLookAtMock).toHaveBeenCalledWith(0, 0);
  });

  it('should toggle camera face tracking', async () => {
    // Spy on requestAnimationFrame
    const spyRaf = vi.spyOn(window, 'requestAnimationFrame');

    const wrapper = mount(VRMEngine, {
      props: { modelConfig: { filename: 'avatar.vrm' } },
    });

    // Initial state: camera off
    expect(wrapper.vm.isCameraOn).toBe(false);

    // Turn ON
    isTrackingRef.value = true;
    await wrapper.find('button.camera-toggle').trigger('click');
    expect(startTrackingMock).toHaveBeenCalled();
    expect(setFaceTrackingActiveMock).toHaveBeenCalledWith(true);
    expect(wrapper.vm.isCameraOn).toBe(true);

    // Face tracking loop execution
    faceDataRef.value = {
      isDetected: true,
      head: { yaw: 0.1, pitch: -0.2 },
      expressions: { blink: 0.5 },
    };

    // Trigger RAF callback
    const rafCallback = spyRaf.mock.calls[0]?.[0];
    if (rafCallback) {
      rafCallback(100);
      expect(updateLookAtMock).toHaveBeenCalledWith(-0.1, -0.2);
      expect(updateExpressionsMock).toHaveBeenCalledWith({ blink: 0.5 });
    }

    // Turn OFF
    isTrackingRef.value = false;
    await wrapper.find('button.camera-toggle').trigger('click');
    expect(stopTrackingMock).toHaveBeenCalled();
    expect(setFaceTrackingActiveMock).toHaveBeenCalledWith(false);
    expect(wrapper.vm.isCameraOn).toBe(false);

    spyRaf.mockRestore();
  });

  it('should handle expressions and decay them back to neutral', () => {
    const wrapper = mount(VRMEngine, {
      props: { modelConfig: { filename: 'avatar.vrm' } },
    });

    wrapper.vm.setExpression('happy');
    expect(triggerMotionMock).toHaveBeenCalled();

    // Advance time to trigger expression decay
    vi.advanceTimersByTime(4000);
  });

  it('should capture frame for AI', () => {
    const wrapper = mount(VRMEngine, {
      props: { modelConfig: { filename: 'avatar.vrm' } },
    });

    wrapper.vm.captureFrameForAI();
    expect(captureFrameMock).toHaveBeenCalled();
  });

  it('should clean up on unmount', () => {
    const wrapper = mount(VRMEngine, {
      props: { modelConfig: { filename: 'avatar.vrm' } },
    });

    wrapper.unmount();
    expect(setFrameUpdateMock).toHaveBeenLastCalledWith(null);
    expect(disposeVRMMock).toHaveBeenCalled();
  });
});
