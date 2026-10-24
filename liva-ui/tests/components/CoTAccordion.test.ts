import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount } from '@vue/test-utils';
import CoTAccordion from '../../src/components/CoTAccordion.vue';

describe('CoTAccordion.vue', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('renders reasoning text, estimated tokens, and pulse indicator when streaming', () => {
    const thinking = 'Bước 1: Phân tích yêu cầu và kiểm tra cơ sở dữ liệu.';
    const wrapper = mount(CoTAccordion, {
      props: {
        thinking,
        isStreaming: true,
        startTime: 1000,
      },
    });

    expect(wrapper.find('[data-testid="cot-accordion"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="cot-pulse-dot"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="cot-status-text"]').text()).toBe('Reasoning Process...');

    const expectedTokens = Math.max(1, Math.round(thinking.length / 3.2));
    expect(wrapper.find('[data-testid="cot-tokens"]').text()).toBe(`${expectedTokens} tokens`);

    expect(wrapper.find('[data-testid="cot-body"]').text()).toContain('Bước 1: Phân tích yêu cầu');
  });

  it('advances elapsed timer while streaming is active', async () => {
    vi.setSystemTime(1000);
    const startTime = 1000;

    const wrapper = mount(CoTAccordion, {
      props: {
        thinking: 'Suy nghĩ...',
        isStreaming: true,
        startTime,
      },
    });

    expect(wrapper.find('[data-testid="cot-elapsed"]').text()).toBe('0.0s');

    vi.advanceTimersByTime(1500);
    await wrapper.vm.$nextTick();

    expect(wrapper.find('[data-testid="cot-elapsed"]').text()).toBe('1.5s');
  });

  it('toggles open/closed state on button click', async () => {
    const wrapper = mount(CoTAccordion, {
      attachTo: document.body,
      props: {
        thinking: 'Dữ liệu suy luận',
        isStreaming: true,
      },
    });

    const toggleBtn = wrapper.find('[data-testid="cot-toggle-btn"]');
    const chevron = wrapper.find('[data-testid="cot-chevron"]');
    const content = wrapper.find('[data-testid="cot-content"]');

    expect((wrapper.vm as any).isOpen).toBe(true);
    expect(chevron.classes()).toContain('rotate-90');
    expect(content.element.style.display).not.toBe('none');

    // Click to close
    await toggleBtn.trigger('click');
    await wrapper.vm.$nextTick();
    expect((wrapper.vm as any).isOpen).toBe(false);
    expect(chevron.classes()).not.toContain('rotate-90');
    expect(content.element.style.display).toBe('none');

    // Click to re-open
    await toggleBtn.trigger('click');
    await wrapper.vm.$nextTick();
    expect((wrapper.vm as any).isOpen).toBe(true);
    expect(chevron.classes()).toContain('rotate-90');
    expect(content.element.style.display).not.toBe('none');

    wrapper.unmount();
  });

  it('auto-collapses when streaming completes if not manually toggled', async () => {
    const wrapper = mount(CoTAccordion, {
      attachTo: document.body,
      props: {
        thinking: 'Đang suy luận chuỗi giải pháp...',
        isStreaming: true,
      },
    });

    expect((wrapper.vm as any).isOpen).toBe(true);
    expect(wrapper.find('[data-testid="cot-content"]').element.style.display).not.toBe('none');

    // Transition streaming to false
    await wrapper.setProps({ isStreaming: false });
    await wrapper.vm.$nextTick();

    expect((wrapper.vm as any).isOpen).toBe(false);
    expect(wrapper.find('[data-testid="cot-content"]').element.style.display).toBe('none');
    expect(wrapper.find('[data-testid="cot-status-text"]').text()).toBe('Reasoning Process');

    wrapper.unmount();
  });

  it('preserves user open state if manually toggled before streaming completes', async () => {
    const wrapper = mount(CoTAccordion, {
      attachTo: document.body,
      props: {
        thinking: 'Đang suy luận chuỗi giải pháp...',
        isStreaming: true,
      },
    });

    // Manually toggle: close then re-open
    await wrapper.find('[data-testid="cot-toggle-btn"]').trigger('click');
    await wrapper.vm.$nextTick();
    await wrapper.find('[data-testid="cot-toggle-btn"]').trigger('click');
    await wrapper.vm.$nextTick();
    expect((wrapper.vm as any).isOpen).toBe(true);

    // When streaming completes, it should stay open because user manually intervened
    await wrapper.setProps({ isStreaming: false });
    await wrapper.vm.$nextTick();
    expect((wrapper.vm as any).isOpen).toBe(true);
    expect(wrapper.find('[data-testid="cot-content"]').element.style.display).not.toBe('none');

    wrapper.unmount();
  });

  it('safely escapes HTML characters in thinking content', () => {
    const unsafeThinking = '<script>alert("xss")</script> & <b>test</b>';
    const wrapper = mount(CoTAccordion, {
      props: {
        thinking: unsafeThinking,
        isStreaming: false,
      },
    });

    const bodyHtml = wrapper.find('[data-testid="cot-body"]').html();
    expect(bodyHtml).not.toContain('<script>');
    expect(bodyHtml).toContain('&lt;script&gt;');
  });
});
