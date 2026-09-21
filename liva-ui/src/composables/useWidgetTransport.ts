import { ref, type Ref, shallowRef, onUnmounted } from 'vue';
import { logger } from '../utils/logger';
import { Channel, invoke } from '@tauri-apps/api/core';
import { unpack } from 'msgpackr';
import {
  OP_SPEAKER_OUT,
  OP_FLUSH,
  OP_VISME,
  VOICE_FRAME_HEADER_SIZE,
  parseSpeakerPayload,
} from '../utils/speakerFrame';
import type { IPlatformAdapter } from '../platform/IPlatformAdapter';
import type { GatewayMessage, MessageDraft } from '../types/gateway';

// Re-export MessageDraft for downstream compatibility
export type { MessageDraft } from '../types/gateway';

export type VoiceIpcEvent =
  | {
      type: 'Speaker' | 'speaker';
      data: {
        turn_epoch?: number;
        sample_rate?: number;
        samples: number[];
      };
    }
  | {
      type: 'Viseme' | 'viseme';
      data: {
        turn_epoch?: number;
        base_seq_id?: number;
        visemes: unknown[];
      };
    }
  | {
      type: 'Flush' | 'flush';
      data: {
        seq_id?: number;
      };
    }
  | {
      type: 'TextEvent' | 'text_event';
      data: {
        event: string;
        payload: unknown;
      };
    };

export interface UseWidgetTransportOptions {
  /** Platform adapter instance */
  platform: IPlatformAdapter | undefined;
  engineStatus: Ref<string>;
  allowWsReconnect?: boolean;
  onConnected: (transport: unknown) => void;
  onDisconnected: () => void;
  onJsonMessage: (data: GatewayMessage) => void;
  onSpeakerBinary: (payload: Uint8Array, turnEpoch: number) => void;
  onFlushBinary: (turnEpoch: number) => void;
  /** Timeline phoneme->viseme preceding audio playback */
  onVisemeBinary?: (payload: Uint8Array) => void;
}

function createVoiceChannel(): Channel<VoiceIpcEvent> {
  if (typeof window !== 'undefined' && !(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__) {
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
      transformCallback: () => 1,
      unregisterCallback: () => {},
      invoke: () => Promise.resolve(),
    };
  }
  return new Channel<VoiceIpcEvent>();
}

export function useWidgetTransport(options: UseWidgetTransportOptions) {
  const ws = shallowRef<unknown | null>(null);
  const pendingDraft = ref<MessageDraft | null>(null);
  const draftBusy = ref(false);

  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  let wsReconnectAttempt = 0;
  let isConnecting = false;
  let isClosed = false;

  const generateMsgId = () => {
    if (typeof crypto !== 'undefined' && crypto.randomUUID) {
      return crypto.randomUUID();
    }
    return `msg-${Date.now()}-${Math.random().toString(36).substring(2, 9)}`;
  };

  const sendMsg = (event: string, payload: Record<string, unknown> = {}) => {
    const isTauri = options.platform?.platformName === 'tauri';

    if (isTauri) {
      if (options.platform?.invokeBackend) {
        options.platform
          .invokeBackend('native_ipc_call', {
            command: event,
            payload,
          })
          .catch((err) => {
            logger.warn('[WidgetTransport]', `Failed to send IPC message ${event}:`, err);
          });
      } else {
        invoke('native_ipc_call', {
          command: event,
          payload,
        }).catch((err) => {
          logger.warn('[WidgetTransport]', `Failed to send IPC message ${event}:`, err);
        });
      }
      return;
    }

    if (ws.value && typeof (ws.value as { send?: (d: string) => void }).send === 'function') {
      (ws.value as { send: (d: string) => void }).send(JSON.stringify({ event, payload }));
    }
  };

  const refreshPendingDraft = () => sendMsg('message:pending');

  const confirmDraft = () => {
    if (!pendingDraft.value || draftBusy.value) return;
    draftBusy.value = true;
    sendMsg('message:confirm', { draftId: pendingDraft.value.draft_id });
  };

  const cancelDraft = () => {
    if (!pendingDraft.value || draftBusy.value) return;
    draftBusy.value = true;
    sendMsg('message:cancel', { draftId: pendingDraft.value.draft_id });
  };

  const handleVoiceIpcEvent = (event: VoiceIpcEvent) => {
    const eventType = (event.type || '').toLowerCase();
    if (eventType === 'speaker') {
      const data = event.data as { turn_epoch?: number; sample_rate?: number; samples?: number[] };
      const samples = data.samples || [];
      const sampleCount = samples.length;
      const turnEpoch = data.turn_epoch ?? 0;
      const sampleRate = data.sample_rate ?? 24000;

      const buffer = new ArrayBuffer(8 + sampleCount * 4);
      const view = new DataView(buffer);
      view.setUint32(0, turnEpoch, true);
      view.setUint32(4, sampleRate, true);
      const f32View = new Float32Array(buffer, 8, sampleCount);
      for (let i = 0; i < sampleCount; i++) {
        f32View[i] = samples[i];
      }
      options.onSpeakerBinary(new Uint8Array(buffer), turnEpoch);
    } else if (eventType === 'viseme') {
      const data = event.data as { turn_epoch?: number; base_seq_id?: number; visemes?: unknown[] };
      if (options.onVisemeBinary) {
        const jsonStr = JSON.stringify({
          turnEpoch: data.turn_epoch ?? 0,
          baseSeqId: data.base_seq_id ?? 0,
          cues: data.visemes ?? [],
        });
        options.onVisemeBinary(new TextEncoder().encode(jsonStr));
      }
    } else if (eventType === 'flush') {
      const data = event.data as { seq_id?: number };
      options.onFlushBinary(data.seq_id ?? 0);
    } else if (eventType === 'textevent' || eventType === 'text_event') {
      const data = event.data as { event: string; payload: unknown };
      options.onJsonMessage({
        event: data.event,
        payload: data.payload as unknown as GatewayPayload,
      } as GatewayMessage);
    }
  };

  const connectWebSocket = async () => {
    if (isConnecting || ws.value || isClosed) return;
    isConnecting = true;
    options.engineStatus.value = 'connecting';

    const isTauri = options.platform?.platformName === 'tauri';

    if (isTauri) {
      try {
        const channel = createVoiceChannel();
        channel.onmessage = (event: VoiceIpcEvent) => {
          handleVoiceIpcEvent(event);
        };

        if (options.platform?.invokeBackend) {
          await options.platform.invokeBackend('voice_subscribe', { channel });
        } else {
          await invoke('voice_subscribe', { channel });
        }

        // Transport wrapper with WebSocket-compatible interface for existing UI checks
        const transportWrapper = {
          channel,
          readyState: 1, // OPEN
          OPEN: 1,
          send: (data: string) => {
            try {
              const parsed = JSON.parse(data);
              sendMsg(parsed.event || parsed.command, parsed.payload || {});
            } catch (e) {
              logger.warn('[WidgetTransport]', 'Failed to parse send payload:', e);
            }
          },
          close: () => {
            closeTransport();
          },
        };

        ws.value = transportWrapper;
        wsReconnectAttempt = 0;
        options.engineStatus.value = 'ready';
        logger.info('[WidgetTransport]', 'Subscribed to VoiceCoordinator IPC channel successfully');
        options.onConnected(transportWrapper);
        isConnecting = false;
        return;
      } catch (err) {
        logger.warn('[WidgetTransport]', 'Failed to subscribe to voice channel via IPC:', err);
      }
    }

    // Web / Test environment with WebSocket mock fallback
    if (typeof WebSocket !== 'undefined') {
      try {
        const wsUrl = `ws://127.0.0.1:8002/ws`;
        const socket = new WebSocket(wsUrl);
        ws.value = socket;
        socket.binaryType = 'arraybuffer';
        options.engineStatus.value = 'websocket-connecting';

        socket.onopen = () => {
          if (ws.value !== socket) return;
          wsReconnectAttempt = 0;
          logger.info('[WidgetTransport]', 'Connected via WebSocket fallback');
          options.engineStatus.value = 'websocket-open';
          options.onConnected(socket);
        };

        socket.onmessage = async (event: MessageEvent) => {
          try {
            let data: GatewayMessage | null = null;
            if (event.data instanceof ArrayBuffer) {
              const arrayBuffer = event.data;
              if (arrayBuffer.byteLength > 0) {
                const view = new DataView(arrayBuffer);
                const type = view.getUint8(0);
                if (type === OP_SPEAKER_OUT) {
                  if (arrayBuffer.byteLength >= VOICE_FRAME_HEADER_SIZE) {
                    const payloadSize = view.getUint32(5, true);
                    if (
                      payloadSize === arrayBuffer.byteLength - VOICE_FRAME_HEADER_SIZE &&
                      payloadSize > 0
                    ) {
                      const payload = new Uint8Array(arrayBuffer, VOICE_FRAME_HEADER_SIZE, payloadSize);
                      const chunk = parseSpeakerPayload(payload);
                      if (!chunk) return;
                      options.onSpeakerBinary(payload, chunk.turnEpoch);
                      return;
                    }
                  }
                  try {
                    data = unpack(new Uint8Array(arrayBuffer, 1)) as GatewayMessage;
                  } catch (unpackErr) {
                    logger.error('[WidgetTransport]', 'Lỗi unpack MsgPack:', unpackErr);
                    return;
                  }
                } else if (type === OP_FLUSH) {
                  if (arrayBuffer.byteLength >= VOICE_FRAME_HEADER_SIZE) {
                    options.onFlushBinary(view.getUint32(1, true));
                  } else {
                    options.onFlushBinary(0);
                  }
                  return;
                } else if (type === OP_VISME && options.onVisemeBinary) {
                  if (arrayBuffer.byteLength >= VOICE_FRAME_HEADER_SIZE) {
                    const payloadSize = view.getUint32(5, true);
                    if (
                      payloadSize > 0 &&
                      arrayBuffer.byteLength >= VOICE_FRAME_HEADER_SIZE + payloadSize
                    ) {
                      options.onVisemeBinary(
                        new Uint8Array(arrayBuffer, VOICE_FRAME_HEADER_SIZE, payloadSize)
                      );
                    }
                  }
                  return;
                }
              }
            } else if (typeof event.data === 'string') {
              if (event.data.trim() === '[INTERRUPT]') {
                options.onJsonMessage({ event: 'interrupt' } as unknown as GatewayMessage);
                return;
              }
              try {
                data = JSON.parse(event.data) as GatewayMessage;
              } catch (e) {
                logger.error('[WidgetTransport]', 'Lỗi phân giải JSON:', e);
                return;
              }
            }
            if (data) {
              options.onJsonMessage(data);
            }
          } catch (parseErr) {
            logger.warn('[WidgetTransport]', 'WebSocket message parse error:', parseErr);
          }
        };

        socket.onerror = () => {
          logger.warn('[WidgetTransport]', 'Socket error');
        };

        socket.onclose = () => {
          if (ws.value !== socket) return;
          ws.value = null;
          options.engineStatus.value = 'websocket-disconnected';
          options.onDisconnected();

          if (options.allowWsReconnect && !isClosed) {
            const delay = Math.min(500 * 2 ** wsReconnectAttempt, 5000);
            wsReconnectAttempt += 1;
            if (reconnectTimer) clearTimeout(reconnectTimer);
            reconnectTimer = setTimeout(() => {
              reconnectTimer = null;
              void connectWebSocket();
            }, delay);
          }
        };

        isConnecting = false;
        return;
      } catch (e) {
        logger.warn('[WidgetTransport]', 'Failed to create fallback WebSocket:', e);
      }
    }

    options.engineStatus.value = 'error';
    isConnecting = false;
  };

  const closeTransport = () => {
    isClosed = true;
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    if (ws.value) {
      const socket = ws.value as { close?: () => void };
      ws.value = null;
      if (typeof socket.close === 'function') {
        socket.close();
      }
      options.engineStatus.value = 'closed';
      options.onDisconnected();
    }
  };

  onUnmounted(() => {
    closeTransport();
  });

  return {
    ws,
    pendingDraft,
    draftBusy,
    generateMsgId,
    sendMsg,
    refreshPendingDraft,
    confirmDraft,
    cancelDraft,
    connectWebSocket,
    closeTransport,
  };
}
