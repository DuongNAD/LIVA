import { triggerRef, type ShallowRef } from 'vue';

export interface StreamBatcherMessage {
  id?: string;
  role: 'user' | 'assistant' | 'system';
  text: string;
  thinking?: string;
  thinkingStartTime?: number;
  thinkingEndTime?: number;
  isThinkingActive?: boolean;
}

export interface StreamBatcherOptions<T extends StreamBatcherMessage> {
  messagesRef: ShallowRef<T[]>;
  onFlush?: () => void;
  createAssistantMessage?: () => T;
  processTextChunk?: (chunk: string) => string;
  processThoughtChunk?: (chunk: string) => string;
}

/**
 * 60 FPS RAF Stream Accumulator
 *
 * Coalesces rapid stream token chunks into a single animation frame tick (~16.6ms),
 * invoking triggerRef and DOM scrolling at most once per frame to eliminate
 * layout thrashing and maintain 60 FPS UI responsiveness.
 */
export class StreamBatcher<T extends StreamBatcherMessage = StreamBatcherMessage> {
  private messagesRef: ShallowRef<T[]>;
  private onFlush?: () => void;
  private createAssistantMessage: () => T;
  private processTextChunk?: (chunk: string) => string;
  private processThoughtChunk?: (chunk: string) => string;

  private rafId: number | null = null;
  private isThinking = false;
  private hasPendingNotification = false;
  private flushCount = 0;

  constructor(options: StreamBatcherOptions<T>) {
    this.messagesRef = options.messagesRef;
    this.onFlush = options.onFlush;
    this.createAssistantMessage =
      options.createAssistantMessage ??
      (() =>
        ({
          id: `msg-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
          role: 'assistant',
          text: '',
          thinking: '',
          thinkingStartTime: performance.now(),
          isThinkingActive: true,
        }) as T);
    this.processTextChunk = options.processTextChunk;
    this.processThoughtChunk = options.processThoughtChunk;
  }

  /**
   * Push an incoming stream chunk into the active message in memory and schedule
   * a single 60 FPS RAF reactive trigger + layout scroll.
   */
  public pushChunk(rawChunk: string, isThought: boolean): void {
    if (!rawChunk) return;

    const msgs = this.messagesRef.value;
    let lastMsg = msgs.length > 0 ? msgs[msgs.length - 1] : null;

    if (!lastMsg || lastMsg.role !== 'assistant') {
      const newMsg = this.createAssistantMessage();
      msgs.push(newMsg);
      lastMsg = newMsg;
    }

    if (isThought) {
      this.isThinking = true;
      const chunk = this.processThoughtChunk ? this.processThoughtChunk(rawChunk) : rawChunk;
      if (lastMsg.thinking === undefined) {
        lastMsg.thinking = '';
      }
      if (!lastMsg.thinkingStartTime) {
        lastMsg.thinkingStartTime = performance.now();
      }
      lastMsg.isThinkingActive = true;
      lastMsg.thinking += chunk;
    } else {
      this.isThinking = false;
      const chunk = this.processTextChunk ? this.processTextChunk(rawChunk) : rawChunk;
      if (lastMsg.isThinkingActive) {
        lastMsg.isThinkingActive = false;
        lastMsg.thinkingEndTime = performance.now();
      }
      lastMsg.text += chunk;
    }

    this.hasPendingNotification = true;

    // Coalesce reactive notification and scroll to 1 tick per animation frame
    if (this.rafId === null) {
      if (typeof requestAnimationFrame === 'function') {
        this.rafId = requestAnimationFrame(() => {
          this.rafId = null;
          this.flush();
        });
      } else {
        // Fallback for non-browser environments without RAF
        this.flush();
      }
    }
  }

  /**
   * Synchronously invoke reactive triggerRef and scroll callback.
   */
  public flush(): void {
    if (this.rafId !== null) {
      if (typeof cancelAnimationFrame === 'function') {
        cancelAnimationFrame(this.rafId);
      }
      this.rafId = null;
    }

    if (!this.hasPendingNotification) return;

    this.hasPendingNotification = false;
    this.flushCount++;

    triggerRef(this.messagesRef);
    if (this.onFlush) {
      this.onFlush();
    }
  }

  /**
   * Reset internal state on stream boundary or reset.
   */
  public reset(): void {
    if (this.rafId !== null) {
      if (typeof cancelAnimationFrame === 'function') {
        cancelAnimationFrame(this.rafId);
      }
      this.rafId = null;
    }
    this.hasPendingNotification = false;
    this.isThinking = false;
  }

  /**
   * Mark thinking as complete on the active assistant message if open.
   */
  public finishThinking(): void {
    const msgs = this.messagesRef.value;
    const lastMsg = msgs.length > 0 ? msgs[msgs.length - 1] : null;
    if (lastMsg && lastMsg.role === 'assistant' && lastMsg.isThinkingActive) {
      lastMsg.isThinkingActive = false;
      lastMsg.thinkingEndTime = performance.now();
      triggerRef(this.messagesRef);
    }
  }

  public get isThinkingActive(): boolean {
    return this.isThinking;
  }

  public get hasPending(): boolean {
    return this.hasPendingNotification;
  }

  public get totalFlushes(): number {
    return this.flushCount;
  }
}
