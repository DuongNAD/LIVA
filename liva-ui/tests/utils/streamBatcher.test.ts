import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { shallowRef } from 'vue';
import { StreamBatcher, type StreamBatcherMessage } from '../../src/utils/streamBatcher';

describe('StreamBatcher', () => {
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

  it('accumulates thought and text chunks into the last assistant message', () => {
    const batcher = new StreamBatcher({
      messagesRef,
      onFlush: flushCallback,
      processThoughtChunk: (chunk) => chunk.replace(/<\/?thought>/g, ''),
    });

    batcher.pushChunk('<thought>Suy nghĩ 1. </thought>', true);
    batcher.pushChunk('Suy nghĩ 2.', true);
    batcher.pushChunk('Xin chào', false);
    batcher.pushChunk(' các bạn!', false);

    expect(messagesRef.value.length).toBe(1);
    const msg = messagesRef.value[0];
    expect(msg.role).toBe('assistant');
    expect(msg.thinking).toBe('Suy nghĩ 1. Suy nghĩ 2.');
    expect(msg.text).toBe('Xin chào các bạn!');
    expect(msg.isThinkingActive).toBe(false);
  });

  it('coalesces multiple rapid chunks into a single RAF trigger', () => {
    const batcher = new StreamBatcher({
      messagesRef,
      onFlush: flushCallback,
    });

    // Push 50 rapid chunks in 1ms
    for (let i = 0; i < 50; i++) {
      batcher.pushChunk(`chunk_${i} `, false);
    }

    // Flush callback should not have been called yet because RAF is pending
    expect(flushCallback).not.toHaveBeenCalled();

    // Advance 1 frame (16.6ms)
    vi.advanceTimersByTime(17);

    // RAF executed once
    expect(flushCallback).toHaveBeenCalledTimes(1);
    expect(batcher.totalFlushes).toBe(1);
  });

  it('synchronously drains pending notifications on flush()', () => {
    const batcher = new StreamBatcher({
      messagesRef,
      onFlush: flushCallback,
    });

    batcher.pushChunk('Token A', false);
    batcher.pushChunk('Token B', false);
    expect(flushCallback).not.toHaveBeenCalled();

    batcher.flush();
    expect(flushCallback).toHaveBeenCalledTimes(1);
    expect(messagesRef.value[0].text).toBe('Token AToken B');

    // Calling flush again when no pending notification does not re-trigger
    batcher.flush();
    expect(flushCallback).toHaveBeenCalledTimes(1);
  });

  it('marks thinking as finished on finishThinking()', () => {
    const batcher = new StreamBatcher({
      messagesRef,
      onFlush: flushCallback,
    });

    batcher.pushChunk('Thinking step...', true);
    expect(messagesRef.value[0].isThinkingActive).toBe(true);

    batcher.finishThinking();
    expect(messagesRef.value[0].isThinkingActive).toBe(false);
    expect(typeof messagesRef.value[0].thinkingEndTime).toBe('number');
  });

  it('resets internal state cleanly on reset()', () => {
    const batcher = new StreamBatcher({
      messagesRef,
      onFlush: flushCallback,
    });

    batcher.pushChunk('Chunk before reset', false);
    batcher.reset();

    // Advancing timers should not trigger flush callback because RAF was cancelled
    vi.advanceTimersByTime(20);
    expect(flushCallback).not.toHaveBeenCalled();
    expect(batcher.hasPending).toBe(false);
  });
});
