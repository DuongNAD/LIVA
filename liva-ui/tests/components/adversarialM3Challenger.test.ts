import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { shallowRef } from 'vue';
import CoTAccordion from '../../src/components/CoTAccordion.vue';
import { StreamBatcher, type StreamBatcherMessage } from '../../src/utils/streamBatcher';
import { renderSafeRichText } from '../../src/utils/richText';

describe('Adversarial M3 Challenger Stress Suite', () => {
  // =========================================================================
  // 1. STREAM BATCHER: RAPID TOKEN BURSTS & MEMORY STABILITY
  // =========================================================================
  describe('StreamBatcher Burst & Coalescing Stress', () => {
    let messagesRef = shallowRef<StreamBatcherMessage[]>([]);
    let flushCallback: ReturnType<typeof vi.fn>;

    beforeEach(() => {
      messagesRef = shallowRef<StreamBatcherMessage[]>([]);
      flushCallback = vi.fn();
      vi.useFakeTimers();
    });

    afterEach(() => {
      vi.useRealTimers();
    });

    it('coalesces a burst of 500 rapid chunks in a single animation frame with zero data loss', () => {
      const batcher = new StreamBatcher({
        messagesRef,
        onFlush: flushCallback,
      });

      // Rapidly fire 500 token chunks synchronously
      for (let i = 0; i < 500; i++) {
        batcher.pushChunk(`token_${i} `, false);
      }

      // Notification must be pending, but callback NOT called until RAF ticks
      expect(batcher.hasPending).toBe(true);
      expect(flushCallback).toHaveBeenCalledTimes(0);

      // Advance 1 frame (16.6ms)
      vi.advanceTimersByTime(17);

      // Coalesced into exactly 1 flush
      expect(flushCallback).toHaveBeenCalledTimes(1);
      expect(batcher.totalFlushes).toBe(1);
      expect(batcher.hasPending).toBe(false);

      // Memory stability: exactly 1 message object created in messagesRef
      expect(messagesRef.value.length).toBe(1);
      const msg = messagesRef.value[0];
      expect(msg.role).toBe('assistant');

      // Verify zero chunk loss across all 500 tokens
      expect(msg.text.startsWith('token_0 ')).toBe(true);
      expect(msg.text.endsWith('token_499 ')).toBe(true);
      const tokenMatches = msg.text.match(/token_\d+/g);
      expect(tokenMatches?.length).toBe(500);
    });

    it('handles high-frequency mixed thought & text bursts (100 thoughts + 100 text) in 1 frame', () => {
      const batcher = new StreamBatcher({
        messagesRef,
        onFlush: flushCallback,
      });

      // 100 thought tokens
      for (let i = 0; i < 100; i++) {
        batcher.pushChunk(`thought_${i} `, true);
      }
      expect(batcher.isThinkingActive).toBe(true);

      // 100 text tokens
      for (let i = 0; i < 100; i++) {
        batcher.pushChunk(`ans_${i} `, false);
      }
      expect(batcher.isThinkingActive).toBe(false);

      // Before RAF: zero flushes
      expect(flushCallback).toHaveBeenCalledTimes(0);

      // Advance 1 frame
      vi.advanceTimersByTime(17);

      expect(flushCallback).toHaveBeenCalledTimes(1);
      expect(messagesRef.value.length).toBe(1);

      const msg = messagesRef.value[0];
      expect(msg.thinking?.match(/thought_\d+/g)?.length).toBe(100);
      expect(msg.text.match(/ans_\d+/g)?.length).toBe(100);
      expect(msg.isThinkingActive).toBe(false);
      expect(typeof msg.thinkingStartTime).toBe('number');
      expect(typeof msg.thinkingEndTime).toBe('number');
      expect(msg.thinkingEndTime!).toBeGreaterThanOrEqual(msg.thinkingStartTime!);
    });

    it('simulates 60 FPS streaming across 60 discrete frames with 600 total tokens', () => {
      const batcher = new StreamBatcher({
        messagesRef,
        onFlush: flushCallback,
      });

      // 60 consecutive frames, 10 tokens per frame
      for (let frame = 0; frame < 60; frame++) {
        for (let t = 0; t < 10; t++) {
          batcher.pushChunk(`f${frame}_t${t} `, false);
        }
        // Advance exactly 1 frame
        vi.advanceTimersByTime(17);
      }

      // Exactly 60 RAF flushes (1 per frame), strictly preventing 600 separate layout thrashing reflows
      expect(flushCallback).toHaveBeenCalledTimes(60);
      expect(batcher.totalFlushes).toBe(60);
      expect(messagesRef.value.length).toBe(1);
    });

    it('ignores empty chunks safely without dirtying state or queueing RAF', () => {
      const batcher = new StreamBatcher({
        messagesRef,
        onFlush: flushCallback,
      });

      batcher.pushChunk('', false);
      batcher.pushChunk('', true);

      expect(batcher.hasPending).toBe(false);
      vi.advanceTimersByTime(50);
      expect(flushCallback).toHaveBeenCalledTimes(0);
      expect(messagesRef.value.length).toBe(0);
    });
  });

  // =========================================================================
  // 2. COT ACCORDION & RENDER SAFE RICH TEXT: MALICIOUS XSS ATTACK VECTORS
  // =========================================================================
  describe('CoTAccordion & renderSafeRichText XSS Adversarial Defense', () => {
    it('neutralizes classic script injection and prevents script DOM node generation', () => {
      const maliciousPayloads = [
        '<script>alert("XSS")</script>',
        '<script src="https://evil.com/payload.js"></script>',
        '<SCRIPT>window.__EXPLOITED__ = true;</SCRIPT>',
        '<script/x>alert(1)</script>',
      ];

      for (const payload of maliciousPayloads) {
        const wrapper = mount(CoTAccordion, {
          props: {
            thinking: payload,
            isStreaming: false,
          },
        });

        const bodyEl = wrapper.find('[data-testid="cot-body"]').element;
        // Strict assertion: zero script elements in DOM tree
        expect(bodyEl.querySelectorAll('script').length).toBe(0);
        // HTML must contain entity escaped version
        expect(bodyEl.innerHTML).not.toContain('<script');
        expect(bodyEl.innerHTML).not.toContain('<SCRIPT');
        expect(bodyEl.innerHTML).toContain('&lt;');
        expect(bodyEl.innerHTML).toContain('&gt;');
        wrapper.unmount();
      }
    });

    it('neutralizes event handler injections and prevents any executable element generation', () => {
      const attributeAttacks = [
        '<img src="invalid_img.png" onerror="alert(1)">',
        '<svg onload="alert(1)" xmlns="http://www.w3.org/2000/svg"><circle r="10"/></svg>',
        '<input autofocus onfocus="alert(1)">',
        '<details open ontoggle="alert(1)"><summary>X</summary></details>',
        '<video><source onerror="alert(1)"></video>',
        '<iframe src="javascript:alert(1)"></iframe>',
        '<a href="javascript:alert(1)">Click for free tokens</a>',
        '<body onload="alert(1)">',
      ];

      for (const payload of attributeAttacks) {
        const wrapper = mount(CoTAccordion, {
          props: {
            thinking: payload,
            isStreaming: false,
          },
        });

        const bodyEl = wrapper.find('[data-testid="cot-body"]').element;
        // Zero dangerous tags rendered into the DOM tree
        expect(bodyEl.querySelectorAll('img, svg, input, details, video, iframe, a, body').length).toBe(0);

        // Every element in the subtree (if any) must not have any event handler attributes
        const allElements = Array.from(bodyEl.querySelectorAll('*'));
        for (const el of allElements) {
          expect(el.hasAttribute('onerror')).toBe(false);
          expect(el.hasAttribute('onload')).toBe(false);
          expect(el.hasAttribute('onfocus')).toBe(false);
          expect(el.hasAttribute('ontoggle')).toBe(false);
          expect(el.hasAttribute('onclick')).toBe(false);
        }
        wrapper.unmount();
      }
    });

    it('neutralizes malformed and obfuscated tag bypasses, ensuring raw markup is neutralized', () => {
      const obfuscatedAttacks = [
        '<br onerror=alert(1)>',
        '<br\tonerror=alert(1)>',
        '<br/onerror=alert(1)>',
        '<br onclick="alert(1)">',
        '<<script>alert(1)</script>',
        '<scr<script>ipt>alert(1)</script>',
        '<div style="background-image: url(javascript:alert(1))">',
      ];

      for (const payload of obfuscatedAttacks) {
        const html = renderSafeRichText(payload);
        // Only clean <br/> tags are permitted
        const nonCleanBr = html.match(/<br[^>]*>/gi)?.filter((tag) => tag !== '<br/>');
        expect(nonCleanBr?.length ?? 0).toBe(0);

        // Mount and verify DOM has zero unescaped executable elements
        const wrapper = mount(CoTAccordion, {
          props: {
            thinking: payload,
            isStreaming: false,
          },
        });
        const bodyEl = wrapper.find('[data-testid="cot-body"]').element;
        expect(bodyEl.querySelectorAll('script, div').length).toBe(0);
        const elements = Array.from(bodyEl.querySelectorAll('*'));
        for (const el of elements) {
          expect(el.hasAttribute('onerror')).toBe(false);
          expect(el.hasAttribute('onclick')).toBe(false);
        }
        wrapper.unmount();
      }
    });

    it('sanitizes channel spoofing attempts within thought content', () => {
      const spoofPayload = `
      Thinking process:
      - 💬 Zalo" onclick="alert(1)"
      - 📘 Messenger <script>alert(2)</script>
      - 📧 Email<img src=x onerror=alert(3)>
      `;

      const wrapper = mount(CoTAccordion, {
        props: {
          thinking: spoofPayload,
          isStreaming: false,
        },
      });

      const bodyEl = wrapper.find('[data-testid="cot-body"]').element;
      // Script and img tags are never rendered into DOM
      expect(bodyEl.querySelectorAll('script, img').length).toBe(0);

      // Verify that all generated button elements have strict, safe attributes
      const buttons = bodyEl.querySelectorAll('button[data-liva-channel]');
      for (const btn of Array.from(buttons)) {
        const channel = btn.getAttribute('data-liva-channel');
        expect(['Zalo', 'Messenger', 'Email']).toContain(channel);
        expect(btn.hasAttribute('onclick')).toBe(false);
        expect(btn.hasAttribute('onerror')).toBe(false);
      }
      wrapper.unmount();
    });
  });

  // =========================================================================
  // 3. STREAM TERMINATION, RESET & ZERO-ORPHAN-TIMER VERIFICATION
  // =========================================================================
  describe('Stream Termination, Reset & Zero-Orphan-Timer Lifecycle', () => {
    let messagesRef = shallowRef<StreamBatcherMessage[]>([]);
    let flushCallback: ReturnType<typeof vi.fn>;

    beforeEach(() => {
      messagesRef = shallowRef<StreamBatcherMessage[]>([]);
      flushCallback = vi.fn();
      vi.useFakeTimers();
    });

    afterEach(() => {
      vi.useRealTimers();
    });

    it('guarantees reset() during pending RAF cancels the frame and leaves zero orphan triggers', () => {
      const batcher = new StreamBatcher({
        messagesRef,
        onFlush: flushCallback,
      });

      // Push chunk to schedule RAF
      batcher.pushChunk('Incoming chunk', false);
      expect(batcher.hasPending).toBe(true);

      // Abort/Reset immediately (e.g. barge-in or stream reset event)
      batcher.reset();
      expect(batcher.hasPending).toBe(false);
      expect(batcher.isThinkingActive).toBe(false);

      // Advance timers far into the future (10 seconds)
      vi.advanceTimersByTime(10000);

      // RAF must have been cancelled; 0 flushes allowed
      expect(flushCallback).toHaveBeenCalledTimes(0);
      expect(batcher.totalFlushes).toBe(0);
    });

    it('guarantees flush() idempotency and cancels scheduled RAF to prevent double execution', () => {
      const batcher = new StreamBatcher({
        messagesRef,
        onFlush: flushCallback,
      });

      batcher.pushChunk('Chunk 1', false);
      batcher.pushChunk('Chunk 2', false);

      // Synchronous flush
      batcher.flush();
      expect(flushCallback).toHaveBeenCalledTimes(1);
      expect(batcher.hasPending).toBe(false);

      // Second consecutive flush call must be a no-op
      batcher.flush();
      expect(flushCallback).toHaveBeenCalledTimes(1);

      // Advance timers past frame boundary (50ms)
      vi.advanceTimersByTime(50);

      // No second flush occurred
      expect(flushCallback).toHaveBeenCalledTimes(1);
      expect(batcher.totalFlushes).toBe(1);
    });

    it('survives 100 rapid cycles of pushChunk -> reset -> pushChunk -> flush without state corruption', () => {
      const batcher = new StreamBatcher({
        messagesRef,
        onFlush: flushCallback,
      });

      for (let i = 0; i < 100; i++) {
        batcher.pushChunk(`cycle_${i}_a`, false);
        batcher.reset();
        batcher.pushChunk(`cycle_${i}_b`, false);
        batcher.flush();
      }

      expect(flushCallback).toHaveBeenCalledTimes(100);
      expect(batcher.totalFlushes).toBe(100);
      expect(batcher.hasPending).toBe(false);

      // Advance time to verify no dangling callbacks
      vi.advanceTimersByTime(1000);
      expect(flushCallback).toHaveBeenCalledTimes(100);
    });

    it('stops CoTAccordion elapsed timer cleanly on unmount with zero orphan intervals', () => {
      vi.setSystemTime(1000);
      const wrapper = mount(CoTAccordion, {
        props: {
          thinking: 'Deep reasoning in progress...',
          isStreaming: true,
          startTime: 1000,
        },
      });

      expect(wrapper.find('[data-testid="cot-elapsed"]').text()).toBe('0.0s');

      // Tick 500ms
      vi.advanceTimersByTime(500);
      expect((wrapper.vm as any).localElapsedMs).toBe(500);

      // Unmount the component while streaming is still active
      wrapper.unmount();

      // Advance time by 5,000ms
      vi.advanceTimersByTime(5000);

      // The interval was cleared in onUnmounted -> no orphan intervals running
      expect(vi.getTimerCount()).toBe(0);
    });

    it('handles rapid isStreaming toggle jitter without creating stacked intervals', async () => {
      vi.setSystemTime(1000);
      const wrapper = mount(CoTAccordion, {
        props: {
          thinking: 'Deep reasoning...',
          isStreaming: true,
          startTime: 1000,
        },
      });

      // Rapidly toggle isStreaming 20 times (simulating socket reconnect / jitter)
      for (let i = 0; i < 20; i++) {
        await wrapper.setProps({ isStreaming: i % 2 === 0 });
      }

      // Settle on isStreaming: true
      await wrapper.setProps({ isStreaming: true });

      // There should be exactly 1 active timer interval, not 20
      expect(vi.getTimerCount()).toBe(1);

      // Advance by 1,000ms
      vi.advanceTimersByTime(1000);
      await wrapper.vm.$nextTick();

      // Elapsed time should match 1.0s, not multiplied by 20 stacked intervals
      expect(wrapper.find('[data-testid="cot-elapsed"]').text()).toBe('1.0s');

      // When stopped, timer stops completely
      await wrapper.setProps({ isStreaming: false });
      expect(vi.getTimerCount()).toBe(0);

      vi.advanceTimersByTime(5000);
      await wrapper.vm.$nextTick();
      expect(wrapper.find('[data-testid="cot-elapsed"]').text()).toBe('1.0s');

      wrapper.unmount();
    });

    it('renders static duration without starting timer when isStreaming is false and start/endTime are provided', async () => {
      const wrapper = mount(CoTAccordion, {
        props: {
          thinking: 'Cached reasoning step',
          isStreaming: false,
          startTime: 2000,
          endTime: 5400,
        },
      });

      // Let Vue's onMounted reactivity update the DOM
      await wrapper.vm.$nextTick();

      // Exactly 3.4s (5400 - 2000)
      expect(wrapper.find('[data-testid="cot-elapsed"]').text()).toBe('3.4s');
      // No active timer interval
      expect(vi.getTimerCount()).toBe(0);

      vi.advanceTimersByTime(5000);
      await wrapper.vm.$nextTick();
      expect(wrapper.find('[data-testid="cot-elapsed"]').text()).toBe('3.4s');
      expect(vi.getTimerCount()).toBe(0);

      wrapper.unmount();
    });
  });

  // =========================================================================
  // 4. COMPLEX UNICODE, EDGE INVOCATIONS & ACCORDION USER OVERRIDES
  // =========================================================================
  describe('Complex Unicode & Manual Interaction Overrides', () => {
    let messagesRef = shallowRef<StreamBatcherMessage[]>([]);
    let flushCallback: ReturnType<typeof vi.fn>;

    beforeEach(() => {
      messagesRef = shallowRef<StreamBatcherMessage[]>([]);
      flushCallback = vi.fn();
      vi.useFakeTimers();
    });

    afterEach(() => {
      vi.useRealTimers();
    });

    it('safely streams complex Vietnamese diacritics, multi-byte emojis and zero-width characters', () => {
      const batcher = new StreamBatcher({
        messagesRef,
        onFlush: flushCallback,
      });

      const complexPayloads = [
        'Tiếng Việt có dấu: ắ ằ ẳ ẵ ặ ế ề ể ễ ệ ố ồ ổ ỗ ộ ',
        'Biểu tượng cảm xúc: 🚀 🧠 🤖 💬 ✨ 🌈 🔥 ⚡ ',
        'Ký tự điều khiển: \u200B\u200C\u200D\uFEFF kết thúc.',
      ];

      for (const chunk of complexPayloads) {
        batcher.pushChunk(chunk, false);
      }

      vi.advanceTimersByTime(17);
      expect(flushCallback).toHaveBeenCalledTimes(1);

      const msg = messagesRef.value[0];
      expect(msg.text).toBe(complexPayloads.join(''));
    });

    it('maintains safe execution when finishThinking() is called unexpectedly', () => {
      const batcher = new StreamBatcher({
        messagesRef,
        onFlush: flushCallback,
      });

      // Call finishThinking with empty messages - should not throw
      expect(() => batcher.finishThinking()).not.toThrow();

      // Push non-assistant message
      messagesRef.value.push({
        role: 'user',
        text: 'Hello',
      });
      expect(() => batcher.finishThinking()).not.toThrow();

      // Push assistant message but without thinking active
      messagesRef.value.push({
        role: 'assistant',
        text: 'World',
        isThinkingActive: false,
      });
      expect(() => batcher.finishThinking()).not.toThrow();
    });

    it('preserves manual collapsed state if user collapses accordion during streaming', async () => {
      const wrapper = mount(CoTAccordion, {
        attachTo: document.body,
        props: {
          thinking: 'Đang suy luận giải pháp...',
          isStreaming: true,
        },
      });

      expect((wrapper.vm as any).isOpen).toBe(true);

      // User manually collapses it during streaming
      await wrapper.find('[data-testid="cot-toggle-btn"]').trigger('click');
      await wrapper.vm.$nextTick();
      expect((wrapper.vm as any).isOpen).toBe(false);

      // Streaming completes
      await wrapper.setProps({ isStreaming: false });
      await wrapper.vm.$nextTick();

      // Must remain collapsed because manualToggle was set
      expect((wrapper.vm as any).isOpen).toBe(false);
      expect(wrapper.find('[data-testid="cot-content"]').element.style.display).toBe('none');

      wrapper.unmount();
    });
  });
});
