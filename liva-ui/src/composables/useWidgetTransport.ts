import { ref, type Ref, shallowRef, onUnmounted } from 'vue';
import { logger } from '../utils/logger';
import { unpack } from 'msgpackr';
import type { IPlatformAdapter } from '../platform/IPlatformAdapter';
import type { GatewayMessage, MessageDraft } from '../types/gateway';
import {
  OP_FLUSH,
  OP_SPEAKER_OUT,
  OP_VISME,
  VOICE_FRAME_HEADER_SIZE,
  parseSpeakerPayload,
} from '../utils/speakerFrame';

// `MessageDraft` nay ở `types/gateway.ts` cùng phần còn lại của hình dạng gói
// tin. Re-export để nơi gọi cũ không phải đổi đường import.
export type { MessageDraft } from '../types/gateway';

export interface UseWidgetTransportOptions {
  /** `undefined` khi chạy ngoài vỏ Tauri — nhánh `?.` bên dưới dựa vào đó. */
  platform: IPlatformAdapter | undefined;
  engineStatus: Ref<string>;
  allowWsReconnect?: boolean;
  onConnected: (ws: WebSocket) => void;
  onDisconnected: () => void;
  onJsonMessage: (data: GatewayMessage) => void;
  onSpeakerBinary: (payload: Uint8Array, turnEpoch: number) => void;
  onFlushBinary: (turnEpoch: number) => void;
  /** VC-8: timeline phoneme→viseme đi trước audio của cùng mẩu. */
  onVisemeBinary?: (payload: Uint8Array) => void;
}

export function useWidgetTransport(options: UseWidgetTransportOptions) {
  const ws = shallowRef<WebSocket | null>(null);
  const pendingDraft = ref<MessageDraft | null>(null);
  const draftBusy = ref(false);

  let wsReconnectTimer: ReturnType<typeof setTimeout> | null = null;
  let wsReconnectAttempt = 0;
  let allowWsReconnect = options.allowWsReconnect ?? true;
  let wsConnectPending = false;

  const generateMsgId = () => {
    if (typeof crypto !== 'undefined' && crypto.randomUUID) {
      return crypto.randomUUID();
    }
    return `msg-${Date.now()}-${Math.random().toString(36).substring(2, 9)}`;
  };

  const sendMsg = (event: string, payload: Record<string, unknown> = {}) => {
    if (ws.value && ws.value.readyState === WebSocket.OPEN) {
      ws.value.send(JSON.stringify({ event, payload }));
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

  const connectWebSocket = async () => {
    if (
      !allowWsReconnect ||
      wsConnectPending ||
      (ws.value && (ws.value.readyState === WebSocket.CONNECTING || ws.value.readyState === WebSocket.OPEN))
    ) {
      return;
    }

    const port = 8002;
    let sessionQuery = '';
    if (options.platform?.platformName === 'tauri') {
      wsConnectPending = true;
      try {
        const ticket = (await options.platform.invokeBackend('issue_websocket_session')) as {
          token?: unknown;
        } | null;
        const token = ticket?.token;
        if (typeof token !== 'string' || !/^[a-f0-9]{64}$/i.test(token)) {
          throw new Error('Tauri không trả session ticket WebSocket hợp lệ');
        }
        sessionQuery = `?session=${encodeURIComponent(token)}`;
      } catch (error) {
        const delay = Math.min(500 * 2 ** wsReconnectAttempt, 5_000);
        wsReconnectAttempt += 1;
        options.engineStatus.value = 'websocket-session-error';
        logger.warn(
          '[Widget]',
          `Không xin được WebSocket session; thử lại sau ${delay}ms:`,
          error instanceof Error ? error.message : String(error)
        );
        if (allowWsReconnect) {
          if (wsReconnectTimer) clearTimeout(wsReconnectTimer);
          wsReconnectTimer = setTimeout(() => {
            wsReconnectTimer = null;
            void connectWebSocket();
          }, delay);
        }
        return;
      } finally {
        wsConnectPending = false;
      }
    }

    if (
      !allowWsReconnect ||
      (ws.value && (ws.value.readyState === WebSocket.CONNECTING || ws.value.readyState === WebSocket.OPEN))
    ) {
      return;
    }

    const wsUrl = `ws://127.0.0.1:${port}/ws${sessionQuery}`;
    const socket = new WebSocket(wsUrl);
    ws.value = socket;
    socket.binaryType = 'arraybuffer';
    options.engineStatus.value = 'websocket-connecting';

    socket.onopen = () => {
      if (ws.value !== socket) return;
      wsReconnectAttempt = 0;
      logger.info('[Widget]', `WSS Connected to Gateway on port ${port}`);
      options.engineStatus.value = 'websocket-open';
      options.onConnected(socket);
    };

    socket.onmessage = async (event) => {
      try {
        // Kiểu ở đây là một lời KHẲNG ĐỊNH, không phải bảo đảm: dữ liệu đến từ
        // mạng, và cả `unpack` lẫn `JSON.parse` đều không kiểm hình dạng. Nơi
        // tiêu thụ vẫn phải tự phòng (`data.payload?.x`) y như trước khi tách
        // composable — việc gắn kiểu không làm thay đổi điều đó.
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
                logger.error('[Widget]', 'Lỗi unpack MsgPack:', unpackErr);
                return;
              }
            } else if (type === OP_FLUSH) {
              if (arrayBuffer.byteLength >= VOICE_FRAME_HEADER_SIZE) {
                options.onFlushBinary(view.getUint32(1, true));
              } else {
                options.onFlushBinary(0); // Fallback
              }
              return;
            } else if (type === OP_VISME && options.onVisemeBinary) {
              // VC-8: timeline viseme — payload là JSON, không phải PCM.
              if (arrayBuffer.byteLength >= VOICE_FRAME_HEADER_SIZE) {
                const payloadSize = view.getUint32(5, true);
                if (
                  payloadSize > 0 &&
                  arrayBuffer.byteLength >= VOICE_FRAME_HEADER_SIZE + payloadSize
                ) {
                  options.onVisemeBinary(
                    new Uint8Array(arrayBuffer, VOICE_FRAME_HEADER_SIZE, payloadSize),
                  );
                }
              }
              return;
            } else {
              return;
            }
          } else {
            return;
          }
        } else if (typeof event.data === 'string') {
          if (event.data.trim() === '[INTERRUPT]') {
            // Tin TỰ SINH, không đến từ dây: nó cố tình thiếu `text`/`audio`/
            // `payload` mà `GatewayPayload` khai là bắt buộc, nên phải ép kiểu
            // ở đúng chỗ này thay vì nới lỏng kiểu cho mọi gói tin thật.
            options.onJsonMessage({ event: 'interrupt' } as unknown as GatewayMessage);
            return;
          }
          try {
            data = JSON.parse(event.data) as GatewayMessage;
          } catch (e) {
            logger.error('[Widget]', 'Lỗi phân giải JSON:', e);
            return;
          }
        } else {
          return;
        }

        if (data) {
          options.onJsonMessage(data);
        }
      } catch (parseErr: unknown) {
        logger.warn(
          '[Widget]',
          'WebSocket message parse error:',
          parseErr instanceof Error ? parseErr.message : String(parseErr)
        );
      }
    };

    socket.onerror = () => {
      logger.warn('[Widget]', `Gateway socket error on port ${port}`);
    };

    socket.onclose = () => {
      if (ws.value !== socket) return;
      ws.value = null;
      options.engineStatus.value = 'websocket-disconnected';
      
      options.onDisconnected();

      if (!allowWsReconnect) return;

      const delay = Math.min(500 * 2 ** wsReconnectAttempt, 5_000);
      wsReconnectAttempt += 1;
      if (wsReconnectTimer) clearTimeout(wsReconnectTimer);
      wsReconnectTimer = setTimeout(() => {
        wsReconnectTimer = null;
        void connectWebSocket();
      }, delay);
      logger.warn('[Widget]', `Gateway disconnected; reconnecting in ${delay}ms`);
    };
  };

  const closeTransport = () => {
    allowWsReconnect = false;
    if (wsReconnectTimer) {
      clearTimeout(wsReconnectTimer);
      wsReconnectTimer = null;
    }
    if (ws.value) {
      const socket = ws.value;
      ws.value = null;
      socket.close();
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
