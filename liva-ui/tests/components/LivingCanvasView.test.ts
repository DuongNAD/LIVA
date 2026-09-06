import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount } from '@vue/test-utils';
import LivingCanvasView from '../../src/views/LivingCanvasView.vue';
import DiffReviewer from '../../src/components/DiffReviewer.vue';
import GenerativeUIFrame from '../../src/components/GenerativeUIFrame.vue';
import { useLivingCanvas } from '../../src/composables/useLivingCanvas';

vi.mock('../../src/platform', () => ({
  getPlatformAdapter: () => ({
    invokeBackend: vi.fn().mockResolvedValue({}),
  }),
}));

describe('LivingCanvasView.vue', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders correctly and mounts child components', () => {
    const wrapper = mount(LivingCanvasView, {
      global: {
        stubs: {
          DiffReviewer: true,
          GenerativeUIFrame: true,
        },
      },
    });

    expect(wrapper.find('.living-canvas-view').exists()).toBe(true);
    expect(wrapper.findComponent(DiffReviewer).exists()).toBe(true);
    expect(wrapper.findComponent(GenerativeUIFrame).exists()).toBe(true);
  });

  it('switches layout modes between diff, canvas, and hybrid', async () => {
    const wrapper = mount(LivingCanvasView, {
      global: {
        stubs: {
          DiffReviewer: true,
          GenerativeUIFrame: true,
        },
      },
    });

    const canvas = useLivingCanvas();

    // Mode buttons
    const modeButtons = wrapper.findAll('.mode-btn');
    expect(modeButtons.length).toBe(3);

    // Click Diff mode
    await modeButtons[0].trigger('click');
    expect(canvas.layoutMode.value).toBe('diff');

    // Click Canvas mode
    await modeButtons[1].trigger('click');
    expect(canvas.layoutMode.value).toBe('canvas');

    // Click Hybrid mode
    await modeButtons[2].trigger('click');
    expect(canvas.layoutMode.value).toBe('hybrid');
  });

  it('updates split ratio with preset buttons in hybrid mode', async () => {
    const wrapper = mount(LivingCanvasView, {
      global: {
        stubs: {
          DiffReviewer: true,
          GenerativeUIFrame: true,
        },
      },
    });

    const canvas = useLivingCanvas();
    canvas.setLayoutMode('hybrid');
    await wrapper.vm.$nextTick();

    const presetButtons = wrapper.findAll('.preset-btn');
    expect(presetButtons.length).toBe(3);

    // Click 30 / 70
    await presetButtons[0].trigger('click');
    expect(canvas.splitRatio.value).toBe(0.3);

    // Click 70 / 30
    await presetButtons[2].trigger('click');
    expect(canvas.splitRatio.value).toBe(0.7);
  });
});
