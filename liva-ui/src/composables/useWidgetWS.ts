import { shallowRef, triggerRef, onMounted, onUnmounted, type Ref } from "vue";
import { logger } from "../utils/logger";
import { pack, unpack } from "msgpackr";

export interface UseWidgetWSOptions {
  engineStatus: Ref<string>;
  voice: any;
  gateway: any;
  isThinking: Ref<boolean>;
  messages: Ref<any[]>;
  engineRef: Ref<any>;
  t: (key: string) => string;
  handleBinaryAudioChunk: (chunk: Uint8Array) => void;
  handleBase64AudioChunk: (base64: string) => void;
  stopQueuedAudio: (clearQueue?: boolean) => void;
  allowIncomingChunks: () => void;
  isPlayingAudio: Ref<boolean>;
  activeAudioSources: Ref<any[]>;
  duckAudio: (vol: number) => void;
  applyWidgetConfig: (config: any, source: string) => void;
  scrollToBottom: () => void;
}

export function useWidgetWS(options: UseWidgetWSOptions) {
  const ws = shallowRef<WebSocket | null>(null);

  const sendMsg = (event: string, payload: any = {}) => {
    if (ws.value && ws.value.readyState === WebSocket.OPEN) {
      const packed = pack({ event, payload });
      const message = new Uint8Array(1 + packed.byteLength);
      message[0] = 0x02; // MessagePack event
      message.set(new Uint8Array(packed), 1);
      ws.value.send(message);
    }
  };

  const connect = () => {
    if (ws.value) return;

    const port = 8082;
    const wsUrl = `ws://127.0.0.1:${port}`;
    const socket = new WebSocket(wsUrl);
    socket.binaryType = "arraybuffer";

    socket.onopen = () => {
      logger.info('[Widget]', `WSS Connected to Gateway on port ${port}`);
      options.engineStatus.value = 'websocket-open';
      sendMsg("get_config");
      sendMsg("get_avatar_models");
      sendMsg("get_user_profile");
      options.voice.startPipeline(socket).catch((e: unknown) =>
        logger.warn('[Widget]', 'Voice pipeline start failed:', e instanceof Error ? e.message : String(e))
      );
    };

    socket.onmessage = async (event) => {
      try {
        let data: any = null;
        if (event.data instanceof ArrayBuffer) {
          const arrayBuffer = event.data;
          if (arrayBuffer.byteLength > 0) {
            const view = new DataView(arrayBuffer);
            const type = view.getUint8(0);
            if (type === 0x02) {
              if (arrayBuffer.byteLength >= 9) {
                const payloadSize = view.getUint32(5, true);
                if (payloadSize === arrayBuffer.byteLength - 9 && payloadSize > 0) {
                  const audioPayload = new Uint8Array(arrayBuffer, 9, payloadSize);
                  options.handleBinaryAudioChunk(audioPayload);
                  return;
                }
              }
              try {
                data = unpack(new Uint8Array(arrayBuffer, 1));
              } catch (unpackErr) {
                logger.error('[Widget]', 'Lỗi unpack MsgPack:', unpackErr);
                return;
              }
            } else {
              return;
            }
          } else {
            return;
          }
        } else if (typeof event.data === "string") {
          if (event.data.trim() === "[INTERRUPT]") {
            options.stopQueuedAudio(true);
            return;
          }
          try {
            data = JSON.parse(event.data);
          } catch (e) {
            logger.error('[Widget]', 'Lỗi phân giải JSON:', e);
            return;
          }
        } else {
          return;
        }

        if (!data) return;

        if (data.event === "config_data" || data.event === "config_updated") {
          const conf = data.payload || data;
          options.applyWidgetConfig(conf, data.event);
        } else if (data.event === "user_profile" || data.event === "profile_updated_success") {
          if (data.payload) {
            options.gateway.userProfile.value = data.payload;
          }
        } else if (data.event === "eco_mode_changed") {
          const enabled = !!data.payload?.enabled;
          (window as any).LIVA_ECO_MODE = enabled;
          logger.info('[Widget]', `Eco Mode status changed: ${enabled}. Throttling avatar renderer.`);
        } else if (data.event === "avatar_demote") {
          const level = data.payload?.level as string;
          const fps = data.payload?.fps as number;
          if (level === 'eco') {
            (window as any).LIVA_ECO_MODE = true;
            (window as any).LIVA_AVATAR_DEMOTE_LEVEL = 'eco';
            logger.info('[Widget]', `VRAM Protection: Avatar demoted to ECO (${fps}fps)`);
          } else if (level === 'freeze') {
            (window as any).LIVA_ECO_MODE = true;
            (window as any).LIVA_AVATAR_DEMOTE_LEVEL = 'freeze';
            logger.info('[Widget]', 'VRAM Protection: Avatar FROZEN (0fps)');
          } else if (level === 'preempted') {
            (window as any).LIVA_ECO_MODE = true;
            (window as any).LIVA_AVATAR_DEMOTE_LEVEL = 'preempted';
            logger.warn('[Widget]', 'VRAM Protection: Avatar PREEMPTED (hard stop)');
          }
        } else if (data.event === "avatar_restore") {
          (window as any).LIVA_ECO_MODE = false;
          (window as any).LIVA_AVATAR_DEMOTE_LEVEL = 'normal';
          logger.info('[Widget]', 'VRAM Protection: Avatar restored to normal rendering');
        } else if (data.event === "debug_log") {
          logger.info('[Widget]', 'Gateway debug', data.payload ?? data);
        } else if (data.event === "stt_fallback_activated") {
          options.voice.activateWebSpeechFallback();
        } else if (data.event === "stt_fallback_deactivated") {
          options.voice.deactivateWebSpeechFallback();
        } else if (data.event === "ai_thinking_start") {
          options.isThinking.value = true;
          options.stopQueuedAudio(true);
          options.scrollToBottom();
          options.voice.setProcessing();
        } else if (data.event === "ai_thinking_end") {
          options.isThinking.value = false;
        } else if (data.event === "ai_stream_reset") {
          if (options.messages.value.length > 0 && options.messages.value[options.messages.value.length - 1].role === "assistant") {
            options.messages.value.pop();
            triggerRef(options.messages);
          }
        } else if (data.event === "ai_stream_start") {
          options.allowIncomingChunks();
          options.isThinking.value = false;

          let thinkingText = "";
          const lastUserIdx = options.messages.value.map(msg => msg.role).lastIndexOf("user");
          const filteredMsgs = options.messages.value.filter((msg, idx) => {
            if (lastUserIdx !== -1 && idx <= lastUserIdx) return true;

            const isThinkingMsg = msg.role === "assistant" && (
              msg.text.includes("sys-thinking-flag") ||
              msg.text.includes("sys-skill-flag") ||
              msg.text.includes("LIVA đang") ||
              msg.text.includes("Identify Tool") ||
              msg.text.includes("Determine Parameters") ||
              msg.text.includes("Execute Tool Call") ||
              msg.thinking
            );
            if (isThinkingMsg) {
              if (msg.thinking) {
                thinkingText = msg.thinking;
              } else {
                const matches = [...msg.text.matchAll(/<i [^>]*class="sys-(?:thinking|skill)-flag"[^>]*>([\s\S]*?)(?:<\/i>|$)/g)];
                if (matches.length > 0) {
                  thinkingText = matches.map(m => m[1]).join("\n\n");
                } else {
                  thinkingText = msg.text;
                }
              }
              return false;
            }
            return true;
          });

          let cleanThinking = "";
          if (thinkingText) {
            cleanThinking = thinkingText
              .replace(/<br\s*\/?>/gi, "\n")
              .replace(/<[^>]+>/g, "")
              .trim();
          }

          options.messages.value = [...filteredMsgs, { role: "assistant", text: "", thinking: cleanThinking || "" }];
          triggerRef(options.messages);
          options.scrollToBottom();
        } else if (data.event === "ai_stream_chunk") {
          if (options.messages.value.length > 0) {
            const lastMsg = options.messages.value[options.messages.value.length - 1];
            let chunk = data.payload.textChunk as string;
            const isThoughtChunk = !!data.payload.isThought;

            if (isThoughtChunk) {
              chunk = chunk.replace(/<\/?thought>/gi, "")
                           .replace(/<\|channel>thought/gi, "")
                           .replace(/<\/channel_thought>/gi, "")
                           .replace(/<\/?scratchpad>/gi, "");

              if (lastMsg.thinking === undefined) {
                lastMsg.thinking = "";
              }
              lastMsg.thinking += chunk;
            } else {
              chunk = chunk.replace(/\[\[SYS_THINKING\]\]/g, options.t('sys_thinking'));
              chunk = chunk.replace(/\[\[SYS_USING_SKILL\]\]/g, options.t('sys_using_skill'));

              const emotionMatch = chunk.match(/^\[(happy|sad|angry|surprised|neutral|relaxed)\]/);
              if (emotionMatch) {
                const emotion = emotionMatch[1];
                chunk = chunk.replace(/^\[(.*?)\]/, '');
                if (options.engineRef.value?.setExpression) {
                  options.engineRef.value.setExpression(emotion);
                }
              }
              chunk = chunk.replace(/\n/g, "<br/>");
              lastMsg.text += chunk;
            }
            triggerRef(options.messages);
            options.scrollToBottom();
            options.voice.keepAlive();
          }
        } else if (data.event === "ai_spoken_response") {
          options.allowIncomingChunks();
          options.isThinking.value = false;
          if (options.activeAudioSources.value.length === 0 && !options.isPlayingAudio.value) {
            options.voice.setPassive();
          }

          let finalReply = data.payload.text.replace(/\n/g, "<br/>");

          let thinkingText = "";
          const lastUserIdx = options.messages.value.map(msg => msg.role).lastIndexOf("user");
          const filteredMsgs = options.messages.value.filter((msg, idx) => {
            if (lastUserIdx !== -1 && idx <= lastUserIdx) return true;

            const isThinkingMsg = msg.role === "assistant" && (
              msg.text.includes("sys-thinking-flag") ||
              msg.text.includes("sys-skill-flag") ||
              msg.text.includes("LIVA đang") ||
              msg.text.includes("Identify Tool") ||
              msg.text.includes("Determine Parameters") ||
              msg.text.includes("Execute Tool Call") ||
              msg.thinking
            );
            if (isThinkingMsg && !msg.thinking) {
              const matches = [...msg.text.matchAll(/<i [^>]*class="sys-(?:thinking|skill)-flag"[^>]*>([\s\S]*?)(?:<\/i>|$)/g)];
              if (matches.length > 0) {
                thinkingText = matches.map(m => m[1]).join("\n\n");
              } else {
                thinkingText = msg.text;
              }
              return false;
            }
            return true;
          });

          const lastMsg = filteredMsgs[filteredMsgs.length - 1];
          if (lastMsg && lastMsg.role === "assistant") {
            lastMsg.text = finalReply;
            if (thinkingText) {
              lastMsg.thinking = thinkingText
                .replace(/<br\s*\/?>/gi, "\n")
                .replace(/<[^>]+>/g, "")
                .trim();
            }
            options.messages.value = [...filteredMsgs];
          } else {
            let cleanThinking = "";
            if (thinkingText) {
              cleanThinking = thinkingText
                .replace(/<br\s*\/?>/gi, "\n")
                .replace(/<[^>]+>/g, "")
                .trim();
            }
            options.messages.value = [...filteredMsgs, { role: "assistant", text: finalReply, thinking: cleanThinking || undefined }];
          }
          triggerRef(options.messages);
          options.scrollToBottom();
        } else if (data.event === "audio_ducking") {
          const vol = typeof data.payload?.volume === 'number' ? data.payload.volume : 1.0;
          options.duckAudio(vol);
        } else if (data.event === "ai_audio_chunk") {
          options.handleBase64AudioChunk(data.payload.audio);
        }
      } catch (parseErr: unknown) {
        logger.warn('[Widget]', 'WebSocket message parse error:', parseErr instanceof Error ? parseErr.message : String(parseErr));
      }
    };

    ws.value = socket;
  };

  const disconnect = () => {
    if (ws.value) {
      ws.value.close();
      ws.value = null;
    }
  };

  onMounted(() => {
    connect();
  });

  onUnmounted(() => {
    disconnect();
  });

  return {
    ws,
    sendMsg,
    connect,
    disconnect,
  };
}
