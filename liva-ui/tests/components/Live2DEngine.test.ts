import { describe, it, expect, vi, beforeEach } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import { ref } from "vue";

vi.mock("pixi.js", () => {
  class Application {
    stage = {
      addChild: vi.fn(),
    };
    destroy = vi.fn();
  }
  return {
    Application,
  };
});

vi.mock("pixi-live2d-display/cubism2", () => {
  const Live2DModel = {
    from: vi.fn().mockResolvedValue({
      scale: { set: vi.fn() },
      x: 0,
      y: 0,
      on: vi.fn(),
      internalModel: {
        motionManager: {
          startRandomMotion: vi.fn(),
        },
      },
    }),
  };
  return {
    Live2DModel,
  };
});

import Live2DEngine from "../../src/components/Live2DEngine.vue";

describe("Live2DEngine.vue", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should mount and initialize the Live2D engine", async () => {
    const wrapper = mount(Live2DEngine, {
      props: {
        modelConfig: { filename: "pio.json" },
      },
    });

    expect(wrapper.exists()).toBe(true);

    // Wait for the onMounted timeout and dynamic imports to resolve
    await new Promise(resolve => setTimeout(resolve, 150));
    await flushPromises();

    // Call public methods exposed
    wrapper.vm.triggerMotion();
    wrapper.vm.startLipSync();
    wrapper.vm.stopLipSync();

    const dummyData = new Float32Array([0.1, 0.2]);
    const mockAudioContext = {} as AudioContext;
    wrapper.vm.playPrecalculatedLipSync(dummyData, 0, mockAudioContext);

    wrapper.unmount();
  });
});
