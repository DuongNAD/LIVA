<script setup lang="ts">
/**
 * WidgetApp.vue — Hybrid Dual-Engine Widget
 * ==========================================
 * Entry component cho cửa sổ widget (transparent overlay).
 * - Auto-detect GPU → lazy load đúng engine (Live2D hoặc VRM)
 * - Zero cross-contamination: engine không dùng = 0 bytes RAM
 * - Phantom Bounding Box Fix (Phương án 1: pointer-events + IPC)
 */
import {
  ref,
  shallowRef,
  triggerRef,
  defineAsyncComponent,
  onMounted,
  onUnmounted,
  onActivated,
  onDeactivated,
  nextTick,
  watch,
  inject,
  type Component,
} from 'vue';
import type { IPlatformAdapter } from './platform/IPlatformAdapter';
import { profileHardware, type EngineMode } from './utils/HardwareDetector';
import { computed } from 'vue';
import { useVoicePipeline } from './composables/useVoicePipeline';
import { useSpeakerPlayback } from './composables/useSpeakerPlayback';
import { logger } from './utils/logger';
import { safeFetch } from './utils/fetch';
import { readRichTextChannel, renderSafeRichText } from './utils/richText';
import ToolPanel from './components/ToolPanel.vue';
import {
  stripAvatarControlTags,
} from './utils/avatarControlTags';
import {
  SpeakerEpochGate,
  parseVisemePayload,
} from './utils/speakerFrame';
import {
  noteChunkScheduled,
  resetVisemes,
  setVisemeClock,
  setVisemeTimeline,
} from './utils/phonemeLipSync';
import { useWidgetAvatarControl, type AvatarEngineApi } from './composables/useWidgetAvatarControl';
import { useWidgetTransport } from './composables/useWidgetTransport';
import type {
  GatewayMessage,
  GatewayPayload,
  MessageDraft,
  WidgetAvatarConfig,
  WidgetModelConfig,
} from './types/gateway';
import { useWidgetWindow } from './composables/useWidgetWindow';
import type { ToolPanelView } from './types/ui';

const platform = inject<IPlatformAdapter>('platform');

// Hình dạng gói tin Gateway nay ở `types/gateway.ts` — `useWidgetTransport.ts`
// cần đúng những kiểu này cho `onJsonMessage`, mà kiểu khai bên trong một SFC
// thì không import được từ ngoài.

/** Các biến toàn cục LIVA gắn lên window để engine/renderer đọc */
type LivaWindow = typeof window & {
  sendLIVAMessage: (text: string) => void;
  LIVA_ECO_MODE: boolean;
  LIVA_AVATAR_DEMOTE_LEVEL: string;
};

const DEFAULT_WIDGET_MODEL = {
  filename: 'models/vrm/Liva.vrm',
  type: '3d',
  format: 'vrm',
};

// ═══════════════════════════════════════════════════════
//  Lazy Load Engines (defineAsyncComponent = 0 byte khi không dùng)
// ═══════════════════════════════════════════════════════
const Live2DEngine = defineAsyncComponent(() => import('./components/Live2DEngine.vue'));
const VRMEngine = defineAsyncComponent(() => import('./components/VRMEngine.vue'));

const activeEngine = shallowRef<Component | null>(null);
const engineMode = ref<EngineMode>('3D');
const activeModelConfig = ref<WidgetModelConfig | null>(null);
const hardwareInfo = ref<string>('');
const engineStatus = ref<string>('booting');

const resolveEngineFromConfig = (config: GatewayPayload) => {
  const avatarMode = config?.ui?.avatarMode ?? config?.avatarMode ?? config?.avatar?.engineMode;
  const activeModel = config?.ui?.activeModel ?? config?.activeModel ?? config?.avatar?.activeModel;

  if (avatarMode === '2D' || avatarMode === '3D') {
    return avatarMode;
  }

  if (activeModel?.type === '3d' || activeModel?.format === 'vrm' || activeModel?.format === 'fbx')
    return '3D';
  if (activeModel?.type === '2d') return '2D';

  return '3D';
};

const normalizeModelConfig = (config: GatewayPayload) => {
  const activeModel = config?.ui?.activeModel ?? config?.activeModel ?? config?.avatar?.activeModel;
  const avatar: WidgetAvatarConfig = config?.avatar ?? {};

  if (activeModel?.filename) return activeModel;

  const candidate = avatar.vrmModel || avatar.live2dModel;
  if (candidate) {
    const lower = String(candidate).toLowerCase();
    return {
      filename: candidate,
      type: lower.includes('/live2d/') ? '2d' : '3d',
      format: lower.endsWith('.fbx') ? 'fbx' : lower.endsWith('.vrm') ? 'vrm' : 'json',
    };
  }

  return DEFAULT_WIDGET_MODEL;
};

const applyWidgetConfig = (config: GatewayPayload, source: string) => {
  const nextEngine = resolveEngineFromConfig(config);
  const nextModelConfig = normalizeModelConfig(config);

  engineMode.value = nextEngine;
  activeModelConfig.value = nextModelConfig;
  activeEngine.value = nextEngine === '3D' ? VRMEngine : Live2DEngine;
  engineStatus.value = `config:${source}:${nextEngine}`;

  logger.info('[Widget]', `${source} → engine=${nextEngine}`, {
    avatarMode: config?.ui?.avatarMode ?? config?.avatarMode,
    activeModel: nextModelConfig,
  });
};

// ═══════════════════════════════════════════════════════
//  Chat State
// ═══════════════════════════════════════════════════════
const isThinking = ref(false);
const inputText = ref('');
const isCollapsed = ref(true);

// Theme Toggle
const isLightMode = ref(globalThis.localStorage?.getItem('theme') === 'light');
const toggleTheme = () => {
  isLightMode.value = !isLightMode.value;
  const newTheme = isLightMode.value ? 'light' : 'dark';
  globalThis.document?.documentElement.setAttribute('data-theme', newTheme);
  globalThis.document?.body.setAttribute('data-theme', newTheme);
  globalThis.localStorage?.setItem('theme', newTheme);
};

import { useI18n } from './composables/useI18n';
import { useGateway } from './composables/useGateway';
// U16 — dải hiện CHI PHÍ tài nguyên. Ở widget chứ không ở dashboard vì đây là
// thứ duy nhất còn hiện khi người dùng đang chạy game/render toàn màn hình,
// tức đúng lúc con số này đáng xem nhất.
import ResourceMeter from './components/ResourceMeter.vue';
const { t } = useI18n();
const gateway = useGateway();

interface Message {
  id: string;
  role: 'user' | 'assistant';
  text: string;
  thinking?: string;
}



const messages = shallowRef<Message[]>([
  {
    id: 'init-welcome',
    role: 'assistant',
    text: t('welcome_liva'),
  },
]);
const chatContainer = ref<HTMLElement | null>(null);

const startNewChat = () => {
  messages.value = [
    {
      id: 'init-welcome',
      role: 'assistant',
      text: t('welcome_liva'),
    },
  ];
  triggerRef(messages);
  speaker.stop(true);
  if (isCollapsed.value) {
    toggleCollapse();
  }
};
const isSensing = ref(false);
let sensingTimeout: ReturnType<typeof setTimeout> | null = null;
const isCameraActive = ref(false);


const toolPanel = ref<ToolPanelView | null>(null);
const TOOL_PRESENTATION_TIMEOUT_MS = 30_000;
let toolPresentationTimer: ReturnType<typeof setTimeout> | null = null;
const toolPresentationActive = ref(false);



// ═══════════════════════════════════════════════════════
//  Thẻ xác nhận gửi tin nhắn
//
//  Lõi KHÔNG đẩy sự kiện xuống đây: `AppState` không có kênh sự kiện, và thêm
//  một kênh vào đó đụng `lib.rs` — file mà repo này thường xuyên có hai phiên
//  sửa song song. Thay vào đó widget hỏi `message:pending` sau mỗi lượt trả
//  lời. Rẻ (một map trong RAM) và được thêm một thứ đường đẩy không có: bản
//  nháp sống sót qua reload widget, vì nó nằm ở lõi chứ không ở màn hình.
//
//  Bản nháp KHÔNG tự hết trên màn hình khi quá hạn — lõi mới là nơi giữ hạn.
//  Bấm xác nhận muộn thì `message:confirm` trả lỗi nói rõ đã quá hạn, và đó là
//  câu trả lời đúng: thà nói "hết hạn rồi" còn hơn im lặng làm thẻ biến mất
//  giữa lúc người dùng đang đọc.
// ═══════════════════════════════════════════════════════


/** Thêm một dòng của LIVA vào khung chat. Dùng cho kết quả gửi. */
const pushAssistantLine = (text: string) => {
  messages.value = [...messages.value, { id: generateMsgId(), role: 'assistant', text }];
  triggerRef(messages);
  scrollToBottom();
};

// ═══════════════════════════════════════════════════════
//  Voice Input (Microphone → STT)
// ═══════════════════════════════════════════════════════
const voice = useVoicePipeline();
const volumeLevel = voice.volumeLevel;
const wakeWordThreshold = voice.wakeWordThreshold;
const wakeProbeFeedback = voice.wakeProbeFeedback;
const setWakeWordThreshold = voice.setWakeWordThreshold;
const diagnosticsPanelRef = voice.diagnosticsPanelRef;
const pipelineError = voice.pipelineError;
const pipelineErrorKind = voice.pipelineErrorKind;
const showDiagnostics = ref(false);
const isListening = computed(() => voice.state.value === 'ACTIVE');

// Silence TS unused variable check for diagnosticsPanelRef (used in Vue template ref)
void diagnosticsPanelRef;

// ═══════════════════════════════════════════════════════
//  Wake Word Detection Sound (Web Audio API)
// ═══════════════════════════════════════════════════════
let wakeWordAudioCtx: AudioContext | null = null;

function playWakeWordSound() {
  try {
    if (!wakeWordAudioCtx) {
      const AudioContextCls =
        globalThis.AudioContext ||
        (globalThis as typeof globalThis & { webkitAudioContext: typeof AudioContext })
          .webkitAudioContext;
      wakeWordAudioCtx = new AudioContextCls();
    }
    if (wakeWordAudioCtx.state === 'suspended') {
      wakeWordAudioCtx.resume();
    }

    const playTone = (freq: number, startTime: number, duration: number) => {
      const oscillator = wakeWordAudioCtx!.createOscillator();
      const gainNode = wakeWordAudioCtx!.createGain();

      oscillator.connect(gainNode);
      gainNode.connect(wakeWordAudioCtx!.destination);

      oscillator.type = 'sine';
      oscillator.frequency.value = freq;

      gainNode.gain.setValueAtTime(0, startTime);
      gainNode.gain.linearRampToValueAtTime(0.3, startTime + 0.02);
      gainNode.gain.exponentialRampToValueAtTime(0.001, startTime + duration);

      oscillator.start(startTime);
      oscillator.stop(startTime + duration);
    };

    const now = wakeWordAudioCtx.currentTime;
    // Siri-like double chime
    playTone(415.3, now, 0.15); // G#4
    playTone(554.37, now + 0.15, 0.2); // C#5

    // Chime cũng ra loa và vọng vào mic. 350 ms cho hai nốt + đuôi vọng, kẻo
    // chính nó kích lại bộ wake-word (ca forceTriggerWakeWord vào thẳng từ
    // PASSIVE, chưa có cooldown nào của worker chắn hộ).
    voice.muteWakeWordFor(750);
  } catch (err) {
    logger.warn('[Widget]', 'Could not play wake word sound:', err);
  }
}

// ═══════════════════════════════════════════════════════
//  Wake Word Detection ("Hey Liva" → auto-activate voice)
//  [v25 Pillar 4] Using ONNX WASM for local inference
// ═══════════════════════════════════════════════════════
const handleWakeWordDetection = () => {
  logger.info('[Widget]', 'Wake Word detected!');

  // Play acknowledgment sound (Siri double-chime)
  playWakeWordSound();

  // Add visual feedback
  messages.value = [
    ...messages.value,
    { id: generateMsgId(), role: 'assistant', text: t('wg_wake_word_ack') },
  ];
  triggerRef(messages);
  scrollToBottom();
};

voice.onWakeWordDetected(handleWakeWordDetection);

const forceTriggerWakeWord = async () => {
  if (voice.state.value === 'OFF') {
    if (ws.value && ws.value.readyState === WebSocket.OPEN) {
      try {
        await voice.startPipeline(ws.value);
      } catch (e) {
        logger.warn('[Widget]', 'Failed to start voice pipeline on force trigger:', e);
        return;
      }
    }
  }
  handleWakeWordDetection();
  if (voice.state.value === 'PASSIVE') {
    voice.state.value = 'ACTIVE';
    sendMsg('wake_word_triggered');
  }
};

// Camera frame capture interval (send to AI every 10s)
let frameCaptureInterval: ReturnType<typeof setInterval> | null = null;


// ═══════════════════════════════════════════════════════
//  Audio Queue — gapless OP_SPEAKER_OUT PCM; encoded JSON audio uses a separate
//  path. FLUSH/barge-in (speaker.stop()/speaker.flush()) stops every
//  scheduled source and resets the scheduling cursor.
// ═══════════════════════════════════════════════════════
const speaker = useSpeakerPlayback({
  channel: '[Widget]',
  useMasterGain: true, // required for audio_ducking volume control
  enableAnalyser: true, // analyser nằm TRONG chuỗi ra, để dẫn khẩu hình
  // VC-8: mỗi chunk PCM được xếp lịch là mốc neo timeline viseme vào đồng hồ
  // AudioContext — đúng thời điểm audio thật bắt đầu, không phải lúc nhận frame.
  onChunkScheduled: ({ startTimeSec, durationSec }) => {
    noteChunkScheduled(startTimeSec, durationSec);
  },
  onPlaybackStarted: () => {
    // Mic đang mở nghe wake-word; giọng TTS vọng vào nó là nguồn dương-tính-giả
    // số một (xem khối "Chống tự nghe" trong useVoicePipeline.ts).
    voice.muteWakeWord();
    sendMsg('audio_play_started');
    // VC-8: cắm đồng hồ AudioContext cho registry viseme (một lần đủ — context
    // sống suốt phiên).
    setVisemeClock(() => speaker.getContext()?.currentTime ?? null);
    // Bám vào analyser MỘT lần cho cả lượt nói. Trước đây việc này nằm ở
    // onSourceStarted, tức chạy lại mỗi chunk — mà chunk được xếp lịch trước
    // khi kêu, nên analyser đọc nhầm nguồn còn im và miệng đóng giữa câu.
    const analyser = speaker.getAnalyser();
    if (analyser && engineRef.value?.startAudioLipSync) {
      engineRef.value.startAudioLipSync(analyser);
    }
  },
  onPlaybackFinished: () => {
    voice.unmuteWakeWord();
    sendMsg('audio_play_finished');
  },
  onQueueDrained: () => {
    if (engineRef.value?.stopAudioLipSync) {
      engineRef.value.stopAudioLipSync();
    }
    if (!isThinking.value && voice.state.value === 'PROCESSING') {
      voice.setPassive();
    }
  },
});

// ═══════════════════════════════════════════════════════
//  Engine ref for triggering motions
// ═══════════════════════════════════════════════════════
const engineRef = ref<AvatarEngineApi | null>(null);

const {
  avatarControlStream,
  restoreWanderWhenIdle,
  executeAvatarAction,
  executeRegisteredAvatarAnimation,
} = useWidgetAvatarControl({
  isThinking,
  toolPresentationActive,
  engineRef,
});

let speakerEpochGate = new SpeakerEpochGate();

const handleGatewayMessage = async (data: GatewayMessage) => {
    if (data.event === 'config_data' || data.event === 'config_updated') {
      const conf = data.payload || data;
      applyWidgetConfig(conf, data.event);
    } else if (data.event === 'user_profile' || data.event === 'profile_updated_success') {
      // Sync user profile (language, tone, etc.) to shared Gateway state
      // so useI18n reactive computed picks up the language change instantly
      if (data.payload) {
        gateway.userProfile.value = data.payload;
      }
    } else if (data.event === 'eco_mode_changed') {
      const enabled = !!data.payload?.enabled;
      (window as LivaWindow).LIVA_ECO_MODE = enabled;
      logger.info(
        '[Widget]',
        `Eco Mode status changed: ${enabled}. Throttling avatar renderer.`
      );
    } else if (data.event === 'avatar_demote') {
      // [Phase 3] Graduated VRAM Protection — reduce avatar rendering to free GPU resources
      const level = data.payload?.level as string;
      const fps = data.payload?.fps as number;
      if (level === 'eco') {
        (window as LivaWindow).LIVA_ECO_MODE = true;
        (window as LivaWindow).LIVA_AVATAR_DEMOTE_LEVEL = 'eco';
        logger.info('[Widget]', `VRAM Protection: Avatar demoted to ECO (${fps}fps)`);
      } else if (level === 'freeze') {
        (window as LivaWindow).LIVA_ECO_MODE = true;
        (window as LivaWindow).LIVA_AVATAR_DEMOTE_LEVEL = 'freeze';
        logger.info('[Widget]', 'VRAM Protection: Avatar FROZEN (0fps)');
      } else if (level === 'preempted') {
        (window as LivaWindow).LIVA_ECO_MODE = true;
        (window as LivaWindow).LIVA_AVATAR_DEMOTE_LEVEL = 'preempted';
        logger.warn('[Widget]', 'VRAM Protection: Avatar PREEMPTED (hard stop)');
      }
    } else if (data.event === 'avatar_restore') {
      // [Phase 3] Restore avatar rendering after VRAM pressure relieved
      (window as LivaWindow).LIVA_ECO_MODE = false;
      (window as LivaWindow).LIVA_AVATAR_DEMOTE_LEVEL = 'normal';
      logger.info('[Widget]', 'VRAM Protection: Avatar restored to normal rendering');
    } else if (data.event === 'debug_log') {
      logger.info('[Widget]', 'Gateway debug', data.payload ?? data);
    } else if (data.event === 'stt_fallback_activated') {
      voice.activateWebSpeechFallback();
    } else if (data.event === 'stt_fallback_deactivated') {
      voice.deactivateWebSpeechFallback();
    } else if (data.event === 'tool_start') {
      startToolPresentation(data.payload);
    } else if (data.event === 'tool_result') {
      finishToolPresentation(data.payload);
    } else if (data.event === 'ai_thinking_start') {
      isThinking.value = true;
      speaker.stop();
      scrollToBottom();
      voice.setProcessing();
    } else if (data.event === 'ai_thinking_end') {
      isThinking.value = false;
    } else if (data.event === 'ai_stream_reset') {
      avatarControlStream.reset();
      if (
        messages.value.length > 0 &&
        messages.value[messages.value.length - 1].role === 'assistant'
      ) {
        messages.value.pop();
        triggerRef(messages);
      }
    } else if (data.event === 'ai_stream_start') {
      avatarControlStream.reset();
      speaker.unblock();
      isThinking.value = false;

      // 1. Find and filter out any existing assistant message containing thinking/skills content
      let thinkingText = '';
      const lastUserIdx = messages.value.map((msg) => msg.role).lastIndexOf('user');
      const filteredMsgs = messages.value.filter((msg, idx) => {
        // Only filter out assistant messages that were added after the last user message in the current turn
        if (lastUserIdx !== -1 && idx <= lastUserIdx) return true;

        const isThinkingMsg =
          msg.role === 'assistant' &&
          (msg.text.includes('sys-thinking-flag') ||
            msg.text.includes('sys-skill-flag') ||
            msg.text.includes('LIVA đang') ||
            msg.text.includes('Identify Tool') ||
            msg.text.includes('Determine Parameters') ||
            msg.text.includes('Execute Tool Call') ||
            msg.thinking);
        if (isThinkingMsg) {
          if (msg.thinking) {
            thinkingText = msg.thinking;
          } else {
            const matches = [
              ...msg.text.matchAll(
                /<i [^>]*class="sys-(?:thinking|skill)-flag"[^>]*>([\s\S]*?)(?:<\/i>|$)/g
              ),
            ];
            if (matches.length > 0) {
              thinkingText = matches.map((m) => m[1]).join('\n\n');
            } else {
              thinkingText = msg.text;
            }
          }
          return false; // Remove this intermediate thinking bubble from history
        }
        return true;
      });

      // 2. Extract clean thinking text to store in the structured field
      let cleanThinking = '';
      if (thinkingText) {
        cleanThinking = thinkingText
          .replace(/<br\s*\/?>/gi, '\n')
          .replace(/<[^>]+>/g, '') // strip HTML tags
          .trim();
      }

      messages.value = [
        ...filteredMsgs,
        { id: generateMsgId(), role: 'assistant', text: '', thinking: cleanThinking || '' },
      ];
      triggerRef(messages);
      scrollToBottom();
    } else if (data.event === 'ai_stream_chunk') {
      if (messages.value.length > 0) {
        const lastMsg = messages.value[messages.value.length - 1];
        let chunk = data.payload.textChunk as string;
        const isThoughtChunk = !!data.payload.isThought;

        if (isThoughtChunk) {
          // Strip raw XML thought tags if any leak
          chunk = chunk
            .replace(/<\/?thought>/gi, '')
            .replace(/<\|channel>thought/gi, '')
            .replace(/<\/channel_thought>/gi, '')
            .replace(/<\/?scratchpad>/gi, '');

          if (lastMsg.thinking === undefined) {
            lastMsg.thinking = '';
          }
          lastMsg.thinking += chunk;
        } else {
          chunk = chunk.replace(/\[\[SYS_THINKING\]\]/g, t('sys_thinking'));
          chunk = chunk.replace(/\[\[SYS_USING_SKILL\]\]/g, t('sys_using_skill'));

          const parsed = avatarControlStream.push(chunk);
          chunk = parsed.text;
          for (const control of parsed.controls) {
            if (control.type === 'emotion') {
              engineRef.value?.setExpression?.(control.value);
            } else if (control.type === 'action') {
              executeAvatarAction(control.value);
            } else {
              executeRegisteredAvatarAnimation(control.value);
            }
          }
          chunk = chunk.replace(/\n/g, '<br/>');
          lastMsg.text += chunk;
        }
        triggerRef(messages);
        scrollToBottom();
        voice.keepAlive(); // [v26] Reset 15s timeout on AI stream activity
      }
    } else if (data.event === 'ai_spoken_response') {
      speaker.unblock();
      isThinking.value = false;
      // Only transition back to PASSIVE immediately if no audio is currently playing/queued.
      // Otherwise, let the source.onended handler switch it to PASSIVE once playback finishes
      // to prevent the microphone from feeding LIVA's own voice back to the wake worker.
      if (!speaker.hasActiveSources() && !speaker.isPlaying()) {
        voice.setPassive();
      }

      const cleanFinalReply = stripAvatarControlTags(data.payload.text);
      avatarControlStream.reset();
      const finalReply = cleanFinalReply.replace(/\n/g, '<br/>');

      // Clean up any remaining thinking bubbles if any got past the stream_start phase
      let thinkingText = '';
      const lastUserIdx = messages.value.map((msg) => msg.role).lastIndexOf('user');
      const filteredMsgs = messages.value.filter((msg, idx) => {
        // Only filter out assistant messages that were added after the last user message in the current turn
        if (lastUserIdx !== -1 && idx <= lastUserIdx) return true;

        const isThinkingMsg =
          msg.role === 'assistant' &&
          (msg.text.includes('sys-thinking-flag') ||
            msg.text.includes('sys-skill-flag') ||
            msg.text.includes('LIVA đang') ||
            msg.text.includes('Identify Tool') ||
            msg.text.includes('Determine Parameters') ||
            msg.text.includes('Execute Tool Call') ||
            msg.thinking);
        if (isThinkingMsg && !msg.thinking) {
          const matches = [
            ...msg.text.matchAll(
              /<i [^>]*class="sys-(?:thinking|skill)-flag"[^>]*>([\s\S]*?)(?:<\/i>|$)/g
            ),
          ];
          if (matches.length > 0) {
            thinkingText = matches.map((m) => m[1]).join('\n\n');
          } else {
            thinkingText = msg.text;
          }
          return false;
        }
        return true;
      });

      const lastMsg = filteredMsgs[filteredMsgs.length - 1];
      if (lastMsg && lastMsg.role === 'assistant') {
        lastMsg.text = finalReply;
        if (thinkingText) {
          lastMsg.thinking = thinkingText
            .replace(/<br\s*\/?>/gi, '\n')
            .replace(/<[^>]+>/g, '')
            .trim();
        }
        messages.value = [...filteredMsgs];
      } else {
        let cleanThinking = '';
        if (thinkingText) {
          cleanThinking = thinkingText
            .replace(/<br\s*\/?>/gi, '\n')
            .replace(/<[^>]+>/g, '')
            .trim();
        }
        messages.value = [
          ...filteredMsgs,
          {
            id: generateMsgId(),
            role: 'assistant',
            text: finalReply,
            thinking: cleanThinking || undefined,
          },
        ];
      }
      triggerRef(messages);
      scrollToBottom();
      // Lượt vừa xong có thể đã soạn một bản nháp — hỏi lõi xem có không.
      refreshPendingDraft();
    } else if (data.event === 'message:pending_response') {
      // Lấy bản nháp mới nhất; lõi đã sắp mới-trước.
      const drafts = (data.payload?.drafts ?? []) as MessageDraft[];
      pendingDraft.value = drafts.length > 0 ? drafts[0] : null;
      draftBusy.value = false;
    } else if (data.event === 'message:confirm_response') {
      pendingDraft.value = null;
      draftBusy.value = false;
      pushAssistantLine(data.payload?.detail || t('wg_draft_sent'));
    } else if (data.event === 'message:confirm_error') {
      // Không xoá thẻ: lỗi mạng thì bấm lại là gửi được. Chỉ khi lõi nói
      // bản nháp không còn nữa thì thẻ mới vô nghĩa, và lúc đó
      // `message:pending` ở lượt sau sẽ tự dọn.
      draftBusy.value = false;
      pushAssistantLine(`⚠️ ${data.payload?.error || t('wg_draft_failed')}`);
    } else if (data.event === 'message:cancel_response') {
      pendingDraft.value = null;
      draftBusy.value = false;
    } else if (
      data.event === 'message:cancel_error' ||
      data.event === 'message:pending_error'
    ) {
      draftBusy.value = false;
    } else if (data.event === 'audio_ducking') {
      // [v26] Stage 1 Barge-in: backend reduces TTS volume when user starts speaking
      const vol = typeof data.payload?.volume === 'number' ? data.payload.volume : 1.0;
      speaker.setMasterVolume(vol);
    } else if (data.event === 'ai_audio_chunk') {
      if (speaker.isBlocked()) return;
      try {
        const binaryStr = atob(data.payload.audio);
        const bytes = new Uint8Array(binaryStr.length);
        for (let i = 0; i < binaryStr.length; i++)
          bytes[i] = binaryStr.codePointAt(i) as number;

        await speaker.enqueueEncodedAudio(bytes.buffer);
      } catch (audioErr: unknown) {
        logger.warn(
          '[Widget]',
          'Audio decode/playback error:',
          audioErr instanceof Error ? audioErr.message : String(audioErr)
        );
      }
    }
};

const {
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
} = useWidgetTransport({
  platform,
  engineStatus,
  allowWsReconnect: true,
  onConnected: (socket) => {
    speakerEpochGate = new SpeakerEpochGate();
    sendMsg('get_config');
    sendMsg('get_avatar_models');
    sendMsg('get_user_profile');
    refreshPendingDraft();
    void (async () => {
      await voice.startPipeline(socket);
      if (
        ws.value === socket &&
        socket.readyState === WebSocket.OPEN &&
        !voice.isReady.value &&
        !voice.pipelineError.value
      ) {
        await voice.startPipeline(socket);
      }
    })().catch((e) => {
      logger.warn(
        '[Widget]',
        'Voice pipeline start failed:',
        e instanceof Error ? e.message : String(e)
      );
    });
  },
  onDisconnected: () => {
    const cleanup = voice.stopPipeline().catch((error) => {
      logger.warn(
        '[Widget]',
        'Voice cleanup after gateway disconnect failed:',
        error instanceof Error ? error.message : String(error)
      );
    });
    void cleanup;
  },
  onSpeakerBinary: (payload, turnEpoch) => {
    if (speakerEpochGate.accepts(turnEpoch)) {
      speaker.enqueueSpeakerPayload(payload);
    }
  },
  onVisemeBinary: (payload) => {
    const tl = parseVisemePayload(payload);
    if (tl && speakerEpochGate.accepts(tl.turnEpoch)) {
      setVisemeTimeline(tl.cues);
    }
  },
  onFlushBinary: (turnEpoch) => {
    speakerEpochGate.observeFlush(turnEpoch);
    resetVisemes(); // VC-8: barge-in xoá cả timeline viseme đang treo
    speaker.flush();
  },
  onJsonMessage: (data) => {
    handleGatewayMessage(data);
  },
});



// Vị trí nhân vật trên màn hình, chuẩn hoá [0,1]; y tính theo chân.
// Mặc định đứng sát mép phải-dưới — chỗ cũ của avatar trước khi mở toàn màn hình.
// Đây là biến mà lớp locomotion sẽ điều khiển để LIVA đi lại quanh desktop.
const avatarScreenPos = ref({ x: 0.85, y: 1.0 });

// Cỡ nhân vật so với khung nhìn. VRM giữ tỉ lệ người thật (~1.6m) nên trông nhỏ
// hơn FBX cùng hệ số — FBX bị autoScaleAndCenter kéo lên 1.9. 0.6 cho ra ~31%
// chiều cao màn hình, xấp xỉ cỡ avatar trước khi mở toàn màn hình.
const avatarScale = ref(0.6);

// ═══════════════════════════════════════════════════════
//  Platform Bridge (Agnostic IPC)
// ═══════════════════════════════════════════════════════

const chatUIRef = ref<HTMLElement | null>(null);
const miniIconsRef = ref<HTMLElement | null>(null);
const toolPanelZoneRef = ref<HTMLElement | null>(null);

const messagesLength = computed(() => messages.value.length);
const {
  dragOffset,
  isDragging,
  onDragStart,
  onDragMove,
  onDragEnd,
  snapPosition,
  verticalSnapPosition,
  snapToEdge,
  toggleCollapse,
  updateInteractiveZones,
  startZonesInterval,
  pauseZonesInterval,
} = useWidgetWindow({
  isCollapsed,
  messagesLength,
  toolPanel,
  chatUIRef,
  chatContainer,
  miniIconsRef,
  toolPanelZoneRef,
  engineRef,
  platform,
});

logger.debug('Window controls initialized', { onDragMove, onDragEnd, snapToEdge });

// ═══════════════════════════════════════════════════════
//  Thinking → trigger avatar motion (Extracted to useWidgetAvatarControl)
// ═══════════════════════════════════════════════════════

const clearToolPresentationTimer = () => {
  if (toolPresentationTimer) clearTimeout(toolPresentationTimer);
  toolPresentationTimer = null;
};

const toolPanelTarget = () => {
  const rect = toolPanelZoneRef.value?.getBoundingClientRect();
  const viewportWidth = Math.max(window.innerWidth, 1);
  const viewportHeight = Math.max(window.innerHeight, 1);
  const hasLayout = !!rect && rect.width > 0 && rect.height > 0;
  const clamp = (value: number, min: number, max: number) => Math.min(Math.max(value, min), max);

  return {
    standX: hasLayout ? clamp((rect.right + 90) / viewportWidth, 0.22, 0.45) : 0.38,
    focusX: hasLayout ? clamp((rect.left + rect.width / 2) / viewportWidth, 0, 1) : 0.18,
    focusY: hasLayout ? clamp((rect.top + rect.height / 2) / viewportHeight, 0, 1) : 0.72,
  };
};

const endToolPresentation = (withErrorGesture: boolean) => {
  toolPresentationActive.value = false;
  clearToolPresentationTimer();
  engineRef.value?.clearInspection?.();
  if (withErrorGesture) engineRef.value?.playGesture?.('shake');
  restoreWanderWhenIdle();
};

const startToolPresentation = (payload: GatewayPayload) => {
  if (!payload.tool) return;
  const tool = payload.tool;
  toolPresentationActive.value = true;
  clearToolPresentationTimer();
  toolPanel.value = {
    tool,
    state: 'loading',
    payload: { label: payload.label || `Đang dùng ${tool}…` },
  };
  engineRef.value?.setWander?.(false);

  nextTick(() => {
    if (!toolPresentationActive.value || toolPanel.value?.tool !== tool) return;
    const target = toolPanelTarget();
    engineRef.value?.moveTo?.(target.standX, 0.96, { run: false });
    engineRef.value?.inspectScreenPoint?.(target.focusX, target.focusY);
    updateInteractiveZones();
  });

  toolPresentationTimer = setTimeout(() => {
    if (!toolPresentationActive.value || toolPanel.value?.tool !== tool) return;
    toolPanel.value = {
      tool,
      state: 'error',
      payload: { message: 'Công cụ không phản hồi sau 30 giây.' },
    };
    endToolPresentation(true);
  }, TOOL_PRESENTATION_TIMEOUT_MS);
};

const finishToolPresentation = (payload: GatewayPayload) => {
  if (!toolPresentationActive.value || !payload.tool || toolPanel.value?.tool !== payload.tool) return;
  if (payload.ok) {
    toolPanel.value = { tool: payload.tool, state: 'done', payload: payload.data };
    endToolPresentation(false);
    return;
  }

  toolPanel.value = {
    tool: payload.tool,
    state: 'error',
    payload: { message: payload.reason || 'Công cụ không hoàn tất.' },
  };
  endToolPresentation(true);
};

const closeToolPanel = () => {
  if (toolPresentationActive.value) endToolPresentation(false);
  toolPanel.value = null;
  nextTick(updateInteractiveZones);
};

const handleRichTextClick = (event: MouseEvent) => {
  const channel = readRichTextChannel(event.target);
  if (channel) {
    (window as LivaWindow).sendLIVAMessage(channel);
  }
};

// Watch camera state from engine
watch(
  () => engineRef.value?.isCameraOn?.value,
  (val) => {
    isCameraActive.value = !!val;
    if (val) {
      startFrameCapture();
    } else {
      stopFrameCapture();
    }
  }
);

// ═══════════════════════════════════════════════════════
//  Camera Frame Capture → AI Vision
// ═══════════════════════════════════════════════════════

/** Send webcam frame to Gateway every 10s for AI multimodal processing */
function startFrameCapture() {
  if (frameCaptureInterval) return;
  frameCaptureInterval = setInterval(() => {
    if (!ws.value || ws.value.readyState !== WebSocket.OPEN) return;
    if (!engineRef.value?.captureFrameForAI) return;

    const frame = engineRef.value.captureFrameForAI();
    if (frame) {
      sendMsg('camera_frame', { image: frame, timestamp: Date.now() });
    }
  }, 10000); // Every 10 seconds
}

function stopFrameCapture() {
  if (frameCaptureInterval) {
    clearInterval(frameCaptureInterval);
    frameCaptureInterval = null;
  }
}

const scrollToBottom = async () => {
  await nextTick();
  if (chatContainer.value) {
    chatContainer.value.scrollTop = chatContainer.value.scrollHeight;
  }
};

// ═══════════════════════════════════════════════════════
//  Sensory Capture (Ctrl+Shift+S)
// ═══════════════════════════════════════════════════════
const handleKeydown = async (e: KeyboardEvent) => {
  if (e.ctrlKey && e.shiftKey && e.key.toLowerCase() === 's') {
    isSensing.value = true;
    try {
      await safeFetch('http://127.0.0.1:3000/api/sensory-capture', { method: 'POST' });
    } catch {
      // ignore
    }
    sensingTimeout = setTimeout(() => {
      isSensing.value = false;
    }, 30000);
  }
};

// ═══════════════════════════════════════════════════════
//  Voice Toggle (Push-to-talk)
//  When PTT starts → pause wake word (audio goes to full STT)
//  When PTT stops → restart wake word ("Hey Liva" listens again)
// ═══════════════════════════════════════════════════════
const toggleVoice = () => {
  if (voice.state.value === 'OFF') {
    if (ws.value && ws.value.readyState === WebSocket.OPEN) {
      logger.info('[Widget]', 'Manually starting voice pipeline...');
      voice
        .startPipeline(ws.value)
        .then(() => {
          if (voice.state.value === 'PASSIVE') {
            voice.toggleVoice();
          }
        })
        .catch((e: unknown) => {
          logger.warn('[Widget]', 'Failed to start voice pipeline on toggle:', e);
        });
    } else {
      logger.warn('[Widget]', 'Cannot start voice pipeline: WebSocket not ready');
    }
  } else {
    voice.toggleVoice();
  }
};

// Interrupt: if user clicks mic while LIVA is speaking
const interruptLIVA = () => {
  speaker.stop();

  if (ws.value && ws.value.readyState === WebSocket.OPEN) {
    ws.value.send('[INTERRUPT]');
  }
};

// ═══════════════════════════════════════════════════════
//  Send Message
// ═══════════════════════════════════════════════════════
const sendMessage = () => {
  if (!inputText.value.trim() || !ws.value || ws.value.readyState !== WebSocket.OPEN) return;

  speaker.stop();

  const text = inputText.value.trim();
  messages.value = [...messages.value, { id: generateMsgId(), role: 'user', text }];
  triggerRef(messages);

  ws.value.send(
    JSON.stringify({
      event: 'user_voice_command',
      payload: { text },
    })
  );

  inputText.value = '';
  scrollToBottom();
};

// ═══════════════════════════════════════════════════════
//  Open Dashboard
// ═══════════════════════════════════════════════════════
const openDashboard = () => {
  if (platform) platform.invokeBackend('open_dashboard');
};

// ═══════════════════════════════════════════════════════
//  Lifecycle
// ═══════════════════════════════════════════════════════
onMounted(() => {
  globalThis.addEventListener('keydown', handleKeydown);

  // Initialize theme properly on mount so the first click doesn't bug out
  const initialTheme = isLightMode.value ? 'light' : 'dark';
  globalThis.document?.documentElement.setAttribute('data-theme', initialTheme);
  globalThis.document?.body.setAttribute('data-theme', initialTheme);

  const hw = profileHardware();
  hardwareInfo.value = `GPU=${hw.gpu}; RAM=${hw.ram}GB; Cores=${hw.cores}; WebGL=${hw.webglVersion}; MaxTex=${hw.maxTextureSize}; Recommended=${hw.recommendedEngine}`;
  logger.info('[Widget]', 'Hardware profile detected', hw);

  // 1. Auto-detect engine và lazy load
  // Ưu tiên cấu hình người dùng từ Dashboard nếu có, fallback theo hardware
  engineMode.value = '3D';
  activeModelConfig.value = DEFAULT_WIDGET_MODEL;
  activeEngine.value = VRMEngine;
  engineStatus.value = 'forced-3d-bootstrap';
  logger.info('[Widget]', 'Initial engine forced to 3D for diagnostics');

  // 2. Mặc định xuyên chuột (Ghost Mode) - Rust will handle this dynamically.
  // We trigger the initial update and start a 150ms periodic check to sync coords.
  nextTick(() => {
    updateInteractiveZones();
    void connectWebSocket();
});
  startZonesInterval();

  // Expose global helper for clickable bubble buttons
  (window as LivaWindow).sendLIVAMessage = (text: string) => {
    if (ws.value && ws.value.readyState === WebSocket.OPEN) {
      speaker.stop();
      messages.value = [...messages.value, { id: generateMsgId(), role: 'user', text }];
      triggerRef(messages);
      sendMsg('user_voice_command', { text });
      scrollToBottom();
    }
  };

  // 3. Connect WebSocket
  // Connect directly because the Tauri event might fire before this component mounts.


});

onUnmounted(() => {
  globalThis.removeEventListener('keydown', handleKeydown);
  closeTransport();
  speaker.close();
  stopFrameCapture();
  voice.stopPipeline();
  pauseZonesInterval();
  clearToolPresentationTimer();
  engineRef.value?.setThinking?.(false);
  engineRef.value?.clearInspection?.();
  // [Audit H-3, H-5] Clean zombie timers
  if (sensingTimeout) {
    clearTimeout(sensingTimeout);
    sensingTimeout = null;
  }
});

onActivated(() => {
  // Widget became visible again — restart frame capture if camera was active
  if (isCameraActive.value) {
    startFrameCapture();
  }
});

onDeactivated(() => {
  // Widget hidden by KeepAlive — pause frame capture + zones interval to save CPU
  stopFrameCapture();
  // [Audit C-2] Also pause zonesInterval
  pauseZonesInterval();
});
</script>

<template>
  <div
    class="h-screen w-screen flex flex-col items-end justify-end bg-transparent font-sans relative overflow-hidden"
  >
    <!-- 3D/2D Engine — khung vẽ phủ trọn màn hình để nhân vật đi lại được.
         `pointer-events: none` giữ click xuyên qua; vùng bắt chuột của riêng thân
         nhân vật do updateInteractiveZones() đăng ký với Rust theo hộp bao thật. -->
    <component
      :is="activeEngine"
      ref="engineRef"
      :modelConfig="activeModelConfig"
      :fullScreen="true"
      :screenPos="avatarScreenPos"
      :avatarScale="avatarScale"
      style="pointer-events: none; position: fixed; inset: 0; z-index: 0; width: 100%; height: 100%"
    />
    <!-- U16: đồng hồ chi phí. `pointer-events: none` để không phá Ghost Mode
         (widget phải click xuyên qua được). -->
    <div class="resource-meter-slot">
      <ResourceMeter />
    </div>

    <Transition name="tool-panel-fade">
      <div v-if="toolPanel" ref="toolPanelZoneRef" class="tool-panel-zone">
        <ToolPanel
          :tool="toolPanel.tool"
          :state="toolPanel.state"
          :payload="toolPanel.payload"
          @close="closeToolPanel"
        />
      </div>
    </Transition>

    <!-- Debug info hidden from UI -->
    <div v-if="false" class="hardware-badge">
      {{ hardwareInfo }}
    </div>

    <div v-if="false" class="engine-badge">Engine: {{ engineMode }} · {{ engineStatus }}</div>

    <!-- Chat UI Layer (pointer-events: auto → bắt click) -->
    <div
      ref="chatUIRef"
      :class="[
        'flex flex-col relative z-10 animate-fade-in-up mb-[60px] mr-4',
        isDragging ? '' : 'transition-all duration-300 ease-out',
        !isCollapsed ? 'w-full max-w-[400px]' : 'w-auto',
      ]"
      :style="{
        'pointer-events': 'auto',
        left: dragOffset.x + 'px',
        top: dragOffset.y + 'px',
      }"
    >
      <!-- Floating Mini-Icons -->
      <div
        ref="miniIconsRef"
        class="absolute flex gap-2.5 no-drag-region transition-all duration-300"
        :class="[
          snapPosition === 'left' ? 'left-0 flex-row-reverse' : 'right-2 flex-row',
          verticalSnapPosition === 'top' ? '-top-[44px]' : '-bottom-[44px]',
        ]"
      >
        <button
          class="floating-mini-icon w-8 h-8 flex items-center justify-center transition-all hover:scale-105"
          :title="t('wg_new_chat')"
          @click="startNewChat"
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            fill="none"
            viewBox="0 0 24 24"
            stroke-width="1.5"
            stroke="currentColor"
            class="w-4 h-4"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8"
            />
            <path stroke-linecap="round" stroke-linejoin="round" d="M21 3v5h-5" />
          </svg>
        </button>
        <button
          class="floating-mini-icon w-8 h-8 flex items-center justify-center transition-all hover:scale-105"
          title="Memory Knot"
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            fill="none"
            viewBox="0 0 24 24"
            stroke-width="1.5"
            stroke="currentColor"
            class="w-4 h-4"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              d="M13.19 8.688a4.5 4.5 0 0 1 1.242 7.244l-4.5 4.5a4.5 4.5 0 0 1-6.364-6.364l1.757-1.757m13.35-.622 1.757-1.757a4.5 4.5 0 0 0-6.364-6.364l-4.5 4.5a4.5 4.5 0 0 0 1.242 7.244"
            />
          </svg>
        </button>
        <button
          class="floating-mini-icon w-8 h-8 flex items-center justify-center transition-all hover:scale-105 relative"
          title="Shadow Digest"
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            fill="none"
            viewBox="0 0 24 24"
            stroke-width="1.5"
            stroke="currentColor"
            class="w-4 h-4"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              d="M12 6.042A8.967 8.967 0 0 0 6 3.75c-1.052 0-2.062.18-3 .512v14.25A8.987 8.987 0 0 1 6 18c2.305 0 4.408.867 6 2.292m0-14.25a8.966 8.966 0 0 1 6-2.292c1.052 0 2.062.18 3 .512v14.25A8.987 8.987 0 0 0 18 18a8.967 8.967 0 0 0-6 2.292m0-14.25v14.25"
            />
          </svg>
        </button>
      </div>
      <!-- Messages (scrollable) -->
      <div
        ref="chatContainer"
        v-if="!isCollapsed && (messages.length > 1 || isThinking)"
        :class="[
          'absolute w-full scrollbar-hide max-h-[300px] overflow-y-auto flex flex-col gap-2 px-2 py-1',
          verticalSnapPosition === 'top' ? 'top-full mt-4' : 'bottom-full mb-4',
        ]"
      >
        <template v-for="msg in messages.slice(-15)" :key="msg.id">
          <div
            v-if="msg.text?.trim() || msg.thinking?.trim()"
            :class="[
              'px-4 py-2.5 rounded-[22px] text-sm max-w-[85%] leading-relaxed flex flex-col gap-1 msg-enter',
              msg.role === 'user'
                ? 'self-end bg-gradient-to-r from-purple-600 to-blue-500 text-white rounded-br-[6px] chat-bubble-user'
                : 'self-start chat-bubble-ai rounded-bl-[6px]',
            ]"
          >
            <details
              v-if="msg.thinking"
              :open="!msg.text || msg.text.length === 0"
              class="thinking-details mb-2 select-none opacity-80 w-full"
              style="outline: none"
            >
              <summary
                class="text-xs text-purple-400 hover:text-purple-300 font-semibold focus:outline-none cursor-pointer flex items-center gap-1"
              >
                💭 {{ t('thinking_details') }}
              </summary>
              <div
                class="mt-1 pl-2 border-l border-purple-500/30 text-xs text-gray-400/80 leading-relaxed whitespace-pre-line"
              >
                {{ msg.thinking }}
              </div>
            </details>
            <div
              v-if="msg.text"
              v-html="renderSafeRichText(msg.text)"
              class="w-full"
              @click="handleRichTextClick"
            ></div>
          </div>
        </template>
        <!-- Thinking indicator -->
        <div
          v-if="isThinking"
          class="self-start chat-bubble-ai px-4 py-2.5 rounded-[22px] rounded-bl-[6px] text-sm flex items-center gap-2 msg-enter"
        >
          <span class="thinking-dot text-purple-400" style="animation-delay: 0s">●</span>
          <span class="thinking-dot text-purple-400" style="animation-delay: 0.2s">●</span>
          <span class="thinking-dot text-purple-400" style="animation-delay: 0.4s">●</span>
        </div>
      </div>

      <!-- Developer Diagnostics Panel -->
      <div
        v-if="!isCollapsed && showDiagnostics"
        ref="diagnosticsPanelRef"
        class="diagnostics-box w-full mb-3 p-4 rounded-[22px] text-xs flex flex-col gap-3 glass-diagnostics msg-enter border border-purple-500/20 shadow-2xl relative"
        style="pointer-events: auto; backdrop-filter: blur(20px)"
      >
        <div class="flex justify-between items-center border-b border-white/10 pb-2">
          <strong class="text-purple-400 font-semibold tracking-wide flex items-center gap-1.5">
            <span>🛠️</span> LIVA WAKE DIAGNOSTICS
          </strong>
          <span
            class="px-2 py-0.5 rounded-full text-[10px] uppercase font-bold tracking-wider"
            :class="{
              'bg-slate-800 text-slate-400 border border-slate-700': voice.state.value === 'OFF',
              'bg-blue-500/20 text-blue-400 border border-blue-500/30':
                voice.state.value === 'PASSIVE',
              'bg-green-500/20 text-green-400 border border-green-500/30':
                voice.state.value === 'ACTIVE',
              'bg-purple-500/20 text-purple-400 border border-purple-500/30':
                voice.state.value === 'PROCESSING',
            }"
          >
            {{ voice.state.value }}
          </span>
        </div>

        <!-- Mic không dùng được. "Chưa cấp quyền" / "không có thiết bị" không phải
             sự cố — báo trung tính, và gợi ý đúng việc cần làm cho từng ca thay vì
             một câu chung dẫn người dùng vào Windows Settings cho mọi thứ. -->
        <div
          v-if="pipelineError"
          class="p-2.5 border rounded-xl text-[10px] leading-relaxed"
          :class="
            pipelineErrorKind === 'failure'
              ? 'bg-red-500/10 border-red-500/25 text-red-200'
              : 'bg-amber-500/10 border-amber-500/25 text-amber-200'
          "
        >
          <strong>{{
            pipelineErrorKind === 'failure' ? '⚠️ Lỗi thiết bị âm thanh:' : '🎙️ Chưa nghe được:'
          }}</strong>
          {{ pipelineError }}
          <div class="mt-1 opacity-80 text-[9px]">
            <template v-if="pipelineErrorKind === 'permission'">
              Cấp quyền Micro trong Windows Settings → Privacy → Microphone (và trong trình duyệt),
              rồi bật lại mic. Trình duyệt nhúng của các công cụ dev thường chặn cứng quyền này — mở
              LIVA bằng trình duyệt thường hoặc app desktop để dùng giọng nói.
            </template>
            <template v-else-if="pipelineErrorKind === 'no-device'">
              Cắm micro vào rồi bật lại mic.
            </template>
            <template v-else-if="pipelineErrorKind === 'busy'">
              Đóng ứng dụng đang giữ micro (Zoom, Teams, OBS…) rồi bật lại mic.
            </template>
            <template v-else-if="pipelineErrorKind === 'unsupported'">
              Mở LIVA bằng app desktop hoặc một trình duyệt hỗ trợ thu âm.
            </template>
            <template v-else>
              Kiểm tra quyền Micro trong Windows Settings → Privacy → Microphone, hoặc trong cài đặt
              trình duyệt.
            </template>
            <span v-if="pipelineErrorKind !== 'failure'">
              Mọi thứ khác của LIVA vẫn chạy bình thường.</span
            >
          </div>
        </div>

        <!-- Mic Volume Meter -->
        <div class="flex flex-col gap-1.5">
          <div class="flex justify-between text-slate-400">
            <span>Microphone Level (RMS)</span>
            <span class="font-mono text-[10px] text-blue-400">Live 60 FPS</span>
          </div>
          <div
            class="h-2 w-full bg-black/40 rounded-full overflow-hidden border border-white/5 relative"
          >
            <div
              class="h-full bg-gradient-to-r from-blue-500 to-indigo-500 rounded-full transition-all duration-75"
              style="width: var(--rms-level, 0%)"
            ></div>
          </div>
        </div>

        <div
          v-if="wakeProbeFeedback.outcome !== 'idle'"
          data-testid="wake-probe-feedback"
          class="p-2.5 border rounded-xl text-[10px] leading-relaxed"
          :class="
            wakeProbeFeedback.outcome === 'accepted'
              ? 'bg-green-500/10 border-green-500/25 text-green-200'
              : 'bg-amber-500/10 border-amber-500/25 text-amber-200'
          "
        >
          <strong>
            {{
              wakeProbeFeedback.outcome === 'accepted'
                ? '✅ Đã nhận “Hey Liva”'
                : '⏳ Đã nghe nhưng chưa khớp'
            }}
          </strong>
          <div class="mt-1 opacity-90">
            Điểm:
            <b>{{
              wakeProbeFeedback.score === null
                ? 'không có'
                : `${(wakeProbeFeedback.score * 100).toFixed(1)}%`
            }}</b>
            <template v-if="wakeProbeFeedback.transcript">
              · Core nghe: “{{ wakeProbeFeedback.transcript }}”
            </template>
            <template v-else> · STT không ra chữ; classifier quyết định theo điểm. </template>
          </div>
        </div>

        <!-- Kết quả xác minh cụm đánh thức (core trả về, không phải điểm model) -->
        <div class="flex flex-col gap-1.5">
          <div class="flex justify-between text-slate-400">
            <span>Câu gọi duy nhất: “Hey Liva”</span>
            <span class="font-mono text-[10px] text-purple-400">core STT</span>
          </div>
          <div
            class="h-2 w-full bg-black/40 rounded-full overflow-hidden border border-white/5 relative"
          >
            <div
              class="h-full bg-gradient-to-r from-purple-500 to-pink-500 rounded-full transition-all duration-75"
              style="width: var(--confidence-level, 0%)"
            ></div>
          </div>
        </div>

        <!-- Sàn RMS của bộ cắt câu -->
        <div class="flex flex-col gap-1.5 bg-white/5 p-2.5 rounded-xl border border-white/5">
          <div class="flex justify-between items-center">
            <span class="text-slate-300">Sàn tiếng nói (RMS)</span>
            <span class="font-mono font-bold text-purple-300">{{
              wakeWordThreshold.toFixed(3)
            }}</span>
          </div>
          <input
            type="range"
            min="0.004"
            max="0.08"
            step="0.001"
            :value="wakeWordThreshold"
            @input="setWakeWordThreshold(parseFloat(($event.target as HTMLInputElement).value))"
            class="w-full accent-purple-500 cursor-pointer h-1.5 bg-black/30 rounded-lg appearance-none"
          />
          <p class="text-[10px] text-slate-400 leading-normal mt-0.5">
            Đây chỉ là cổng "có ai đang nói không" để cắt câu; nội dung do core nghe lại và quyết
            định. Hạ xuống nếu mic yếu, nâng lên nếu phòng ồn khiến nó gửi đi quá nhiều.
          </p>
        </div>

        <!-- Manual Actions -->
        <div class="flex gap-2">
          <button
            @click="forceTriggerWakeWord"
            class="flex-1 py-1.5 px-3 rounded-xl bg-purple-600/30 hover:bg-purple-600/50 border border-purple-500/30 text-[11px] font-semibold text-purple-200 transition-all active:scale-95 flex items-center justify-center gap-1.5"
          >
            <span>⚡</span> Force Trigger
          </button>
        </div>

        <!-- Architecture info -->
        <div class="text-[10px] text-slate-500 leading-relaxed border-t border-white/5 pt-2">
          💡 Model chạy local offline trong Rust core; âm thanh chỉ đi qua WebSocket nội bộ, không
          gửi lên cloud.
        </div>
      </div>

      <!-- Thẻ xác nhận gửi tin nhắn.
           Hiện CẢ tên lẫn địa chỉ đích: tên đúng mà số sai vẫn là gửi nhầm
           người, và người dùng là lớp duy nhất bắt được chuyện đó. -->
      <div v-if="!isCollapsed && pendingDraft" class="draft-card w-full mb-2 p-3 rounded-xl">
        <div class="flex items-center gap-2 mb-2">
          <span class="text-[11px] font-semibold opacity-90">{{ t('wg_draft_title') }}</span>
          <span class="draft-platform text-[10px] px-1.5 py-0.5 rounded">{{
            pendingDraft.platform
          }}</span>
        </div>
        <div class="text-[11px] opacity-70 mb-1">
          {{ t('wg_draft_to') }}
          <b>{{ pendingDraft.display_name }}</b>
          <span class="opacity-60">({{ pendingDraft.handle }})</span>
        </div>
        <div class="draft-body text-[12px] p-2 rounded-lg mb-2 break-words">
          {{ pendingDraft.text }}
        </div>
        <div class="flex gap-2">
          <button class="draft-btn draft-btn-send" :disabled="draftBusy" @click="confirmDraft">
            {{ draftBusy ? t('wg_draft_sending') : t('wg_draft_confirm') }}
          </button>
          <button class="draft-btn draft-btn-cancel" :disabled="draftBusy" @click="cancelDraft">
            {{ t('wg_draft_cancel') }}
          </button>
        </div>
      </div>

      <!-- Full Chat Bar State -->
      <div
        v-if="!isCollapsed"
        class="chat-capsule w-full flex items-center p-[6px]"
        :class="snapPosition === 'left' ? 'flex-row-reverse' : ''"
      >
        <!-- Drag Handle (Grip) -->
        <div
          class="w-6 h-8 flex items-center justify-center cursor-move transition-colors"
          :class="
            isLightMode
              ? 'text-slate-400 hover:text-slate-600'
              : 'text-white/30 hover:text-white/60'
          "
          :title="t('wg_drag')"
          @mousedown="onDragStart"
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="14"
            height="14"
            fill="currentColor"
            viewBox="0 0 16 16"
          >
            <path
              d="M7 2a1 1 0 1 1-2 0 1 1 0 0 1 2 0zm3 0a1 1 0 1 1-2 0 1 1 0 0 1 2 0zM7 5a1 1 0 1 1-2 0 1 1 0 0 1 2 0zm3 0a1 1 0 1 1-2 0 1 1 0 0 1 2 0zM7 8a1 1 0 1 1-2 0 1 1 0 0 1 2 0zm3 0a1 1 0 1 1-2 0 1 1 0 0 1 2 0zM7 11a1 1 0 1 1-2 0 1 1 0 0 1 2 0zm3 0a1 1 0 1 1-2 0 1 1 0 0 1 2 0zM7 14a1 1 0 1 1-2 0 1 1 0 0 1 2 0zm3 0a1 1 0 1 1-2 0 1 1 0 0 1 2 0z"
            />
          </svg>
        </div>
        <input
          v-model="inputText"
          @keyup.enter="sendMessage"
          type="text"
          :placeholder="t('wg_placeholder')"
          class="chat-input flex-1 bg-transparent border-none pl-1 pr-2 focus:outline-none w-full"
        />
        <!-- Send Button (visible when input has text) -->
        <button
          v-if="inputText.trim()"
          @click="sendMessage"
          class="send-btn"
          :title="t('wg_send') || 'Send'"
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            fill="none"
            viewBox="0 0 24 24"
            stroke-width="2"
            stroke="currentColor"
            class="w-4 h-4"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              d="M6 12 3.269 3.126A59.769 59.769 0 0 1 21.485 12 59.768 59.768 0 0 1 3.27 20.876L5.999 12Zm0 0h7.5"
            />
          </svg>
        </button>
        <div
          class="flex items-center gap-1.5"
          :class="snapPosition === 'left' ? 'flex-row-reverse pl-1' : 'pr-1'"
        >
          <!-- Toggle Collapse Button -->
          <button @click="toggleCollapse" class="chat-icon-btn bg-transparent border-none outline-none w-8 h-8 rounded-full flex justify-center items-center" :title="t('wg_collapse')">
            <svg
              v-if="snapPosition === 'left'"
              xmlns="http://www.w3.org/2000/svg"
              fill="none"
              viewBox="0 0 24 24"
              stroke-width="1.5"
              stroke="currentColor"
              class="w-4 h-4"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                d="M15.75 19.5L8.25 12l7.5-7.5"
              />
            </svg>
            <svg
              v-else
              xmlns="http://www.w3.org/2000/svg"
              fill="none"
              viewBox="0 0 24 24"
              stroke-width="1.5"
              stroke="currentColor"
              class="w-4 h-4"
            >
              <path stroke-linecap="round" stroke-linejoin="round" d="M8.25 4.5l7.5 7.5-7.5 7.5" />
            </svg>
          </button>

          <!-- Theme toggle button -->
          <button
            @click="toggleTheme"
            class="chat-icon-btn bg-transparent border-none outline-none w-8 h-8 rounded-full flex justify-center items-center"
            :title="t('wg_theme')"
          >
            <svg
              v-if="isLightMode"
              xmlns="http://www.w3.org/2000/svg"
              fill="none"
              viewBox="0 0 24 24"
              stroke-width="2"
              stroke="currentColor"
              class="w-4 h-4 text-yellow-400 drop-shadow-[0_0_8px_rgba(250,204,21,0.6)]"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                d="M12 3v2.25m6.364.386-1.591 1.591M21 12h-2.25m-.386 6.364-1.591-1.591M12 18.75V21m-4.773-2.227-1.591 1.591M5.25 12H3m4.227-4.773L5.636 5.636M15.75 12a3.75 3.75 0 1 1-7.5 0 3.75 3.75 0 0 1 7.5 0Z"
              />
            </svg>
            <svg
              v-else
              xmlns="http://www.w3.org/2000/svg"
              fill="none"
              viewBox="0 0 24 24"
              stroke-width="2"
              stroke="currentColor"
              class="w-4 h-4 text-blue-100 drop-shadow-[0_0_6px_rgba(219,234,254,0.4)]"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                d="M21.752 15.002A9.72 9.72 0 0 1 18 15.75c-5.385 0-9.75-4.365-9.75-9.75 0-1.33.266-2.597.748-3.752A9.753 9.753 0 0 0 3 11.25C3 16.635 7.365 21 12.75 21a9.753 9.753 0 0 0 9.002-5.998Z"
              />
            </svg>
          </button>

          <!-- Diagnostics toggle button -->
          <button
            @click="showDiagnostics = !showDiagnostics"
            class="chat-icon-btn bg-transparent border-none outline-none w-8 h-8 rounded-full flex justify-center items-center transition-all"
            :class="
              showDiagnostics
                ? 'text-purple-400 bg-purple-500/10'
                : 'text-slate-400 hover:text-slate-200'
            "
            title="Diagnostics"
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              fill="none"
              viewBox="0 0 24 24"
              stroke-width="1.5"
              stroke="currentColor"
              class="w-4 h-4"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                d="M9 9V4.5M12 9V3M15 9V5.25M6 9V7.5M3 9v7.5m3-3V21m3-6.75V18m3-6V19.5m3-8.25V18m3-4.5V21"
              />
            </svg>
          </button>

          <!-- Camera indicator -->
          <div
            v-if="isCameraActive"
            class="w-8 h-8 rounded-full bg-green-500/20 text-green-400 flex justify-center items-center text-xs"
            :title="t('wg_cam_on')"
          >
            👁️
          </div>

          <button
            @click="openDashboard"
            class="chat-icon-btn bg-transparent border-none outline-none w-8 h-8 rounded-full flex justify-center items-center"
            :title="t('wg_settings')"
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              fill="none"
              viewBox="0 0 24 24"
              stroke-width="1.5"
              stroke="currentColor"
              class="w-5 h-5"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                d="M9.594 3.94c.09-.542.56-.94 1.11-.94h2.593c.55 0 1.02.398 1.11.94l.213 1.281c.063.374.313.686.645.87.074.04.147.083.22.127.325.196.72.257 1.075.124l1.217-.456a1.125 1.125 0 0 1 1.37.49l1.296 2.247a1.125 1.125 0 0 1-.26 1.431l-1.003.827c-.293.241-.438.613-.43.992a7.723 7.723 0 0 1 0 .255c-.008.378.137.75.43.99l1.005.828c.424.35.534.955.26 1.43l-1.298 2.247a1.125 1.125 0 0 1-1.369.491l-1.217-.456c-.355-.133-.75-.072-1.076.124a6.47 6.47 0 0 1-.22.128c-.331.183-.581.495-.644.869l-.213 1.281c-.09.543-.56.94-1.11.94h-2.594c-.55 0-1.019-.398-1.11-.94l-.213-1.281c-.062-.374-.312-.686-.644-.87a6.52 6.52 0 0 1-.22-.127c-.325-.196-.72-.257-1.076-.124l-1.217.456a1.125 1.125 0 0 1-1.369-.49l-1.297-2.247a1.125 1.125 0 0 1 .26-1.431l1.004-.827c.292-.24.437-.613.43-.991a6.932 6.932 0 0 1 0-.255c.007-.38-.138-.751-.43-.992l-1.004-.827a1.125 1.125 0 0 1-.26-1.43l1.297-2.247a1.125 1.125 0 0 1 1.37-.491l1.216.456c.356.133.751.072 1.076-.124.072-.044.146-.086.22-.128.332-.183.582-.495.644-.869l.214-1.28Z"
              />
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                d="M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z"
              />
            </svg>
          </button>

          <!-- Voice button (mic toggle) - Focal Point -->
          <button
            @click="isThinking ? interruptLIVA() : toggleVoice()"
            :class="[
              'w-9 h-9 rounded-full flex justify-center items-center transition-all relative',
              snapPosition === 'left' ? 'mr-1' : 'ml-1',
              isThinking ? 'animate-pulse' : '',
              isListening
                ? isLightMode
                  ? 'bg-indigo-400/60 text-white shadow-[0_0_15px_rgba(129,140,248,0.5)]'
                  : 'bg-[#43528F]/30 text-[#0f1225] shadow-[inset_0_0_15px_rgba(99,102,241,0.2)]'
                : 'mic-btn',
            ]"
            :title="
              isThinking ? t('wg_interrupt') : isListening ? t('wg_stop_mic') : t('wg_start_mic')
            "
          >
            <!-- Volume ring (when recording) -->
            <svg v-if="isListening" class="voice-ring" viewBox="0 0 36 36">
              <circle
                cx="18"
                cy="18"
                r="16"
                fill="none"
                :stroke="isLightMode ? 'rgba(255,255,255,0.4)' : 'rgba(0,0,0,0.5)'"
                stroke-width="2"
              />
              <circle
                cx="18"
                cy="18"
                r="16"
                fill="none"
                :stroke="isLightMode ? '#ffffff' : '#7C93F5'"
                stroke-width="2.5"
                :stroke-dasharray="`${volumeLevel * 100} ${100 - volumeLevel * 100}`"
                stroke-linecap="round"
                transform="rotate(-90 18 18)"
              />
            </svg>
            <!-- Icon -->
            <svg
              v-if="isThinking"
              xmlns="http://www.w3.org/2000/svg"
              fill="none"
              viewBox="0 0 24 24"
              stroke-width="1.5"
              stroke="currentColor"
              class="w-5 h-5"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                d="M10.05 4.575a1.575 1.575 0 1 0-3.15 0v3m3.15-3v-1.5a1.575 1.575 0 0 1 3.15 0v1.5m-3.15 0 .075 5.925m3.075.75V4.575m0 0a1.575 1.575 0 0 1 3.15 0V15M6.9 7.575a1.575 1.575 0 1 0-3.15 0v8.175a6.75 6.75 0 0 0 6.75 6.75h2.018a5.25 5.25 0 0 0 3.712-1.538l1.732-1.732a5.25 5.25 0 0 0 1.538-3.712l.003-2.024a.668.668 0 0 1 .198-.471 1.575 1.575 0 1 0-2.228-2.228 3.818 3.818 0 0 0-1.12 2.687M6.9 7.575V12m6.27 4.318A4.49 4.49 0 0 1 16.35 15m.002 0h-.002"
              />
            </svg>
            <svg
              v-else-if="isListening"
              xmlns="http://www.w3.org/2000/svg"
              fill="none"
              viewBox="0 0 24 24"
              stroke-width="2"
              stroke="currentColor"
              class="w-4 h-4"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                d="M12 18.75a6 6 0 0 0 6-6v-1.5m-6 7.5a6 6 0 0 1-6-6v-1.5m6 7.5v3.75m-3.75 0h7.5M12 15.75a3 3 0 0 1-3-3V4.5a3 3 0 1 1 6 0v8.25a3 3 0 0 1-3 3Z"
              />
            </svg>
            <svg
              v-else
              xmlns="http://www.w3.org/2000/svg"
              fill="none"
              viewBox="0 0 24 24"
              stroke-width="1.5"
              stroke="currentColor"
              class="w-5 h-5"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                d="M12 18.75a6 6 0 0 0 6-6v-1.5m-6 7.5a6 6 0 0 1-6-6v-1.5m6 7.5v3.75m-3.75 0h7.5M12 15.75a3 3 0 0 1-3-3V4.5a3 3 0 1 1 6 0v8.25a3 3 0 0 1-3 3Z"
              />
            </svg>
          </button>
        </div>
      </div>

      <!-- Compact Collapsed State — LIVA Branded Icon -->
      <div
        v-else
        class="chat-capsule collapsed-capsule w-12 h-12 flex items-center justify-center relative rounded-full shadow-lg ml-auto"
      >
        <!-- Outer Drag Ring -->
        <div class="absolute inset-0 rounded-full border-[2px] border-white/10 hover:border-purple-400/40 cursor-move transition-colors duration-300 z-10" @mousedown.stop="onDragStart" :title="t('wg_drag')"></div>

        <!-- Expand Button — LIVA Sparkle Icon -->
        <button @mousedown.stop @click="toggleCollapse" class="bg-transparent border-none outline-none w-9 h-9 rounded-full flex justify-center items-center z-20 transition-all duration-200 hover:scale-110" :title="t('wg_collapse')">
          <svg
            xmlns="http://www.w3.org/2000/svg"
            fill="none"
            viewBox="0 0 24 24"
            stroke-width="1.5"
            stroke="currentColor"
            class="w-5 h-5"
            :class="isLightMode ? 'text-indigo-500' : 'text-purple-300'"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              d="M9.813 15.904 9 18.75l-.813-2.846a4.5 4.5 0 0 0-3.09-3.09L2.25 12l2.846-.813a4.5 4.5 0 0 0 3.09-3.09L9 5.25l.813 2.846a4.5 4.5 0 0 0 3.09 3.09L15.75 12l-2.846.813a4.5 4.5 0 0 0-3.09 3.09ZM18.259 8.715 18 9.75l-.259-1.035a3.375 3.375 0 0 0-2.455-2.456L14.25 6l1.036-.259a3.375 3.375 0 0 0 2.455-2.456L18 2.25l.259 1.035a3.375 3.375 0 0 0 2.455 2.456L21.75 6l-1.036.259a3.375 3.375 0 0 0-2.455 2.456ZM16.894 20.567 16.5 21.75l-.394-1.183a2.25 2.25 0 0 0-1.423-1.423L13.5 18.75l1.183-.394a2.25 2.25 0 0 0 1.423-1.423l.394-1.183.394 1.183a2.25 2.25 0 0 0 1.423 1.423l1.183.394-1.183.394a2.25 2.25 0 0 0-1.423 1.423Z"
            />
          </svg>
        </button>
      </div>
    </div>
  </div>
</template>

<style>
@keyframes blob {
  0% {
    transform: translate(0px, 0px) scale(1);
  }
  33% {
    transform: translate(30px, -50px) scale(1.1);
  }
  66% {
    transform: translate(-20px, 20px) scale(0.9);
  }
  100% {
    transform: translate(0px, 0px) scale(1);
  }
}
.animate-blob {
  animation: blob 7s infinite;
}
.animation-delay-2000 {
  animation-delay: 2s;
}

@keyframes fadeInUp {
  from {
    opacity: 0;
    transform: translateY(20px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}
.animate-fade-in-up {
  animation: fadeInUp 0.6s ease-out forwards;
}

.scrollbar-hide::-webkit-scrollbar {
  display: none;
}
.scrollbar-hide {
  -ms-overflow-style: none;
  scrollbar-width: none;
}

/* Thẻ xác nhận gửi tin nhắn.
   Viền hổ phách, không phải màu nền của bong bóng chat: đây là thứ DUY NHẤT
   trong widget gây ra hành động không hoàn tác được, nên nó phải nhìn khác mọi
   thứ còn lại. Nút gửi cũng cố ý KHÔNG phải màu chủ đạo — không để người dùng
   bấm nó theo quán tính. */
.draft-card {
  background: rgba(251, 191, 36, 0.08);
  border: 1px solid rgba(251, 191, 36, 0.35);
  animation: fadeInUp 0.25s ease-out forwards;
}
.draft-platform {
  background: rgba(251, 191, 36, 0.18);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}
.draft-body {
  background: rgba(0, 0, 0, 0.18);
  white-space: pre-wrap;
}
.draft-btn {
  flex: 1;
  padding: 6px 10px;
  border-radius: 8px;
  font-size: 12px;
  font-weight: 600;
  transition:
    filter 0.15s,
    opacity 0.15s;
}
.draft-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.draft-btn:not(:disabled):hover {
  filter: brightness(1.15);
}
.draft-btn-send {
  background: rgba(245, 158, 11, 0.9);
  color: #1c1917;
}
.draft-btn-cancel {
  background: rgba(255, 255, 255, 0.08);
  border: 1px solid rgba(255, 255, 255, 0.15);
}

/* Thinking dots animation */
@keyframes thinkingPulse {
  0%,
  80%,
  100% {
    opacity: 0.3;
    transform: scale(0.8);
  }
  40% {
    opacity: 1;
    transform: scale(1.2);
  }
}
.thinking-dot {
  animation: thinkingPulse 1.4s infinite ease-in-out;
  font-size: 8px;
}

/* Voice button */
.voice-btn {
  position: relative;
  z-index: 1;
}
.voice-ring {
  position: absolute;
  inset: -2px;
  width: calc(100% + 4px);
  height: calc(100% + 4px);
  pointer-events: none;
  transition: stroke-dasharray 0.1s ease;
}

/* Recording pulse */
@keyframes recPulse {
  0%,
  100% {
    box-shadow: 0 0 0 0 rgba(239, 68, 68, 0.4);
  }
  50% {
    box-shadow: 0 0 0 6px rgba(239, 68, 68, 0);
  }
}
.voice-btn.bg-red-500\/30 {
  animation: recPulse 1.5s infinite;
}

.hardware-badge {
  position: absolute;
  left: 16px;
  bottom: 16px;
  max-width: 420px;
  padding: 8px 12px;
  border-radius: 12px;
  background: rgba(0, 0, 0, 0.45);
  color: rgba(255, 255, 255, 0.9);
  font-size: 11px;
  line-height: 1.4;
  pointer-events: none;
  backdrop-filter: blur(8px);
  z-index: 20;
}

.engine-badge {
  position: absolute;
  left: 16px;
  bottom: 66px;
  padding: 6px 10px;
  border-radius: 10px;
  background: rgba(21, 128, 61, 0.45);
  color: rgba(240, 253, 244, 0.95);
  font-size: 11px;
  pointer-events: none;
  backdrop-filter: blur(8px);
  z-index: 20;
}

/* Premium HITL Action Buttons */
.hitl-container {
  display: flex;
  gap: 10px;
  margin-top: 12px;
  width: 100%;
}
.hitl-btn {
  flex: 1;
  padding: 8px 16px;
  border-radius: 12px;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.2s ease;
  border: none;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
}
.hitl-channel-container {
  flex-direction: column;
  gap: 6px;
}
.hitl-channel-btn {
  width: 100%;
  margin-top: 6px;
  justify-content: flex-start;
  text-align: left;
}
.hitl-btn-approve {
  background: linear-gradient(135deg, #a855f7 0%, #3b82f6 100%);
  color: white;
}
.hitl-btn-approve:hover {
  transform: translateY(-1px);
  box-shadow: 0 6px 16px rgba(168, 85, 247, 0.4);
}
.hitl-btn-approve:active {
  transform: translateY(1px);
}
.hitl-btn-reject {
  background: rgba(255, 255, 255, 0.08);
  border: 1px solid rgba(239, 68, 68, 0.4);
  color: #ef4444;
}
.hitl-btn-reject:hover {
  background: rgba(239, 68, 68, 0.15);
  transform: translateY(-1px);
  box-shadow: 0 6px 16px rgba(239, 68, 68, 0.25);
}
.hitl-btn-reject:active {
  transform: translateY(1px);
}

.glass-diagnostics {
  background: rgba(15, 17, 26, 0.75);
  border: 1px solid rgba(168, 85, 247, 0.15);
  box-shadow: 0 8px 32px 0 rgba(0, 0, 0, 0.37);
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
}

/* U16 — chỗ đặt đồng hồ chi phí.
   `pointer-events: none` là BẮT BUỘC: widget chạy ở Ghost Mode, người dùng phải
   click xuyên qua nó xuống ứng dụng bên dưới. Một dải chắn chuột ở góc màn hình
   sẽ phá đúng tính năng mà demo này định khoe. */
.resource-meter-slot {
  position: fixed;
  top: 10px;
  right: 12px;
  z-index: 5;
  pointer-events: none;
}

.tool-panel-zone {
  position: fixed;
  left: 24px;
  bottom: 92px;
  z-index: 16;
  pointer-events: none;
}

.tool-panel-fade-enter-active,
.tool-panel-fade-leave-active {
  transition: opacity 180ms ease;
}

.tool-panel-fade-enter-from,
.tool-panel-fade-leave-to {
  opacity: 0;
}

@media (max-width: 520px) {
  .tool-panel-zone {
    left: 12px;
    bottom: 76px;
  }
}

@media (prefers-reduced-motion: reduce) {
  .tool-panel-fade-enter-active,
  .tool-panel-fade-leave-active {
    transition: none;
  }
}
</style>
