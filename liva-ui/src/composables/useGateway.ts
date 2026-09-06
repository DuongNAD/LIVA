import { ref, shallowRef } from "vue";
import { logger } from "../utils/logger";
import { unpack } from "msgpackr";
import type {
  LivaConfig,
  AIConfig,
  VoiceConfig,
  SystemStatus,
  SkillInfo,
  TaskItem,
  AvatarModelInfo,
  VoiceProfile,
  TaskPlanReplyPayload,
  WSClientEvent,
  ChannelItem,
  PairedNodeInfo,
  PendingPairingChallenge,
  BrowserStatus,
  BrowserActionRecord,
  SkillManifestInfo,
  SkillLogEntry,
} from "liva-common";

// State lưu trữ kết nối
const isConnected = ref(false);
const ws = shallowRef<WebSocket | null>(null); // [Audit C-3] shallowRef — no deep proxy on native object

// State Dữ liệu toàn cục (Single Source of Truth cho Dashboard)
// Typed from liva-common — compile-time safety across UI ↔ Gateway boundary
const configData = ref<Partial<LivaConfig>>({});
const aiConfig = ref<Partial<AIConfig>>({});
const voiceStatus = ref<Partial<VoiceConfig>>({});
const voiceProfiles = ref<VoiceProfile[]>([]);

/**
 * Một giọng preset VieNeu — khớp `VoiceInfo` trong `tts/vieneu/mod.rs`.
 * Tên trường giữ nguyên `snake_case` của Rust để khỏi phải ánh xạ hai chiều.
 */
export interface VieNeuVoiceInfo {
  name: string;
  description: string;
  gender: string;
  region: string;
  style: string;
  is_default: boolean;
}

const vieneuVoices = ref<VieNeuVoiceInfo[]>([]);
/** Giọng đang nạp; `null` nghĩa là VieNeu đang tắt, không phải "chưa biết". */
const vieneuCurrent = ref<string | null>(null);
const vieneuEnabled = ref(false);
/** Câu lõi trả về mô tả nó đã làm gì thật (đổi ngay / đã nạp / chỉ ghi cấu hình). */
const vieneuNotice = ref('');

/**
 * Cổng đồng ý quan sát thụ động (U20). `granted` mặc định `false` để khớp
 * fail-closed của lõi: cho tới khi có đáp ứng thật, giao diện coi như CHƯA bật.
 */
const observationConsent = ref<{ granted: boolean; active: boolean; updatedAt: number | null }>({
  granted: false,
  active: false,
  updatedAt: null,
});
const applyConsentPayload = (payload: unknown) => {
  if (!payload || typeof payload !== 'object' || Array.isArray(payload)) return;
  const p = payload as { granted?: boolean; active?: boolean; updatedAt?: number | null };
  observationConsent.value = {
    granted: Boolean(p.granted),
    active: Boolean(p.active),
    updatedAt: typeof p.updatedAt === 'number' ? p.updatedAt : null,
  };
};

const systemStatus = ref<Partial<SystemStatus>>({});
export interface PreflightItem {
  name: string;
  available: boolean | null;
  status: string;
  consequence: string;
}
export interface PreflightReport {
  items: PreflightItem[];
}
const preflightReport = ref<PreflightReport | null>(null);
const applyPreflightPayload = (payload: unknown) => {
  if (!payload || typeof payload !== 'object' || Array.isArray(payload)) return;
  const rawItems = (payload as { items?: unknown }).items;
  if (!Array.isArray(rawItems)) return;
  const items: PreflightItem[] = [];
  for (const raw of rawItems) {
    if (!raw || typeof raw !== 'object' || Array.isArray(raw)) continue;
    const item = raw as Record<string, unknown>;
    if (
      typeof item.name !== 'string' ||
      (typeof item.available !== 'boolean' && item.available !== null) ||
      typeof item.status !== 'string' ||
      typeof item.consequence !== 'string'
    ) continue;
    items.push({
      name: item.name,
      available: item.available,
      status: item.status,
      consequence: item.consequence,
    });
  }
  preflightReport.value = { items };
};
const skillsList = ref<SkillInfo[]>([]);
const tasksList = ref<TaskItem[]>([]);
const avatarModels3D = ref<AvatarModelInfo[]>([]);
const avatarModels2D = ref<AvatarModelInfo[]>([]);
const gpuSetupStatus = ref<string>('');

// Milestone 2 State Singletons
const channelsList = ref<ChannelItem[]>([]);
const pairedNodesList = ref<PairedNodeInfo[]>([]);
const pendingPairingList = ref<PendingPairingChallenge[]>([]);
const browserStatus = ref<BrowserStatus>({
  isRunning: true,
  isPaused: false,
  currentUrl: 'https://liva.ai/dashboard',
  pageTitle: 'LIVA Cognitive Dashboard',
  httpStatus: 200,
  viewportWidth: 1280,
  viewportHeight: 800,
  sandboxActive: true,
  ssrfGuard: true,
});
const browserScreenshot = ref<string>('');
const browserActionLogs = ref<BrowserActionRecord[]>([]);

// Vision (Qwen3-VL "nhìn màn hình") — answer + busy/error state for the UI.
const visionAnswer = ref<string>('');
const visionBusy = ref<boolean>(false);
const visionError = ref<string>('');
let visionTimeout: ReturnType<typeof setTimeout> | null = null;

const finishVision = (text: string, err = '') => {
  if (visionTimeout) { clearTimeout(visionTimeout); visionTimeout = null; }
  visionAnswer.value = text;
  visionError.value = err;
  visionBusy.value = false;
};

// ── Canh chừng màn hình (vision:add_region + get_changed_regions) ──────────
// Đưa `diff_region` — thuật toán được test kỹ nhất core — lên đường chạy thật.
// Trình tự bắt buộc: `vision:capture` TRƯỚC để (1) lấy kích thước khung vật lý
// — `diff_region` TỪ CHỐI vùng vượt biên chứ không tự kẹp, mà CSS px của UI
// lệch DPI so với px vật lý — và (2) mồi luôn baseline so sánh.
const WATCH_REGION_ID = 'ui-watch';
const WATCH_POLL_MS = 3000;
/** Ngưỡng "có thay đổi": 2% điểm ảnh khác — đủ nhạy với cửa sổ bật lên,
 *  đủ lì với nhiễu con trỏ. */
const WATCH_THRESHOLD = 0.02;

export interface WatchEvent {
  time: string;       // HH:MM:SS cho danh sách trong UI
  difference: number; // tỉ lệ điểm ảnh đổi [0..1]
}

const watchActive = ref<boolean>(false);
const watchStarting = ref<boolean>(false);
const watchError = ref<string>('');
const watchLastDiff = ref<number>(0);
const watchEvents = ref<WatchEvent[]>([]);
let watchTimer: ReturnType<typeof setInterval> | null = null;

const stopWatchTimer = () => {
  if (watchTimer) { clearInterval(watchTimer); watchTimer = null; }
};

export interface MemoryL0Item {
  id?: string;
  role?: string;
  content?: string;
  timestamp?: number;
  userMsg?: string;
  aiReply?: string;
}
export interface MemoryFactItem {
  key: string;
  value: string;
  source?: string;
  category?: string;
  memoryStrength: number;
  importance?: number;
  createdAt?: string;
  // Không giải mã được bằng khoá hiện tại (sai LIVA_ENCRYPTION_KEY). Khi true,
  // `value` luôn rỗng (backend không rò ciphertext). Bản gốc vẫn còn trên đĩa.
  locked?: boolean;
}
export interface MemoryEventItem {
  eventId: string;
  timestamp: number;
  rawUserMsg?: string;
  rawAiReply?: string;
  phi?: { facts?: string[]; entities?: string[] };
  psi?: { sentiment?: string; intent?: string; relational?: string };
  consolidationStatus?: string;
  domain?: string;
  category?: string;
  traceKeywords?: string[] | string;
}
export interface MemoryVectorItem {
  id: string;
  vecId?: string;
  text?: string;
  type?: string;
  domain?: string;
  category?: string;
  distance?: number;
  content?: string;
  traceKeywords?: string[] | string;
  createdAt?: number;
  sourceEventIds?: string[] | string;
}

const memoryData = ref<{
  l0: MemoryL0Item[];
  l0_5: string;
  facts: MemoryFactItem[];
  lockedFactsCount?: number;
  events: MemoryEventItem[];
  vectors: MemoryVectorItem[];
}>({ l0: [], l0_5: "", facts: [], events: [], vectors: [] });

const applyConfigPayload = (payload: unknown) => {
  if (payload && typeof payload === 'object' && !Array.isArray(payload)) {
    configData.value = payload as Partial<LivaConfig>;
  }
};

const applyAIConfigPayload = (payload: unknown) => {
  if (payload && typeof payload === 'object' && !Array.isArray(payload)) {
    aiConfig.value = (payload as { ai?: Partial<AIConfig> }).ai ?? payload as Partial<AIConfig>;
  }
};

const applyVoiceStatusPayload = (payload: unknown) => {
  if (payload && typeof payload === 'object' && !Array.isArray(payload)) {
    voiceStatus.value = (payload as { voice?: Partial<VoiceConfig> }).voice ?? payload as Partial<VoiceConfig>;
  }
};

/**
 * Nhận đáp ứng của `voice:list_vieneu_voices` và `voice:set_vieneu_voice`.
 *
 * Hai lệnh trả cùng hình dạng nên dùng chung bộ nhận. `voices` chỉ có ở lệnh
 * liệt kê, nên chỉ ghi đè danh sách khi mảng thật sự có mặt — nếu không, một
 * lần đổi giọng sẽ xoá sạch danh sách đang hiển thị.
 */
const applyVieneuPayload = (payload: unknown) => {
  if (!payload || typeof payload !== 'object' || Array.isArray(payload)) return;
  const data = payload as {
    voices?: VieNeuVoiceInfo[];
    current?: string | null;
    enabled?: boolean;
    applied?: string;
  };
  if (Array.isArray(data.voices)) vieneuVoices.value = data.voices;
  vieneuCurrent.value = data.current ?? null;
  vieneuEnabled.value = Boolean(data.enabled);
  if (typeof data.applied === 'string') vieneuNotice.value = data.applied;
};

// Task Planning Chat — callback registry for inline AI planning
let _taskPlanReplyCallback: ((payload: TaskPlanReplyPayload) => void) | null = null;

// Skill Check Result — callback registry for self-test results
let _skillCheckResultCallback: ((payload: unknown) => void) | null = null;

// Bulk Skill Check Complete — callback registry
let _allSkillsCheckCompleteCallback: ((payload: unknown) => void) | null = null;

// Env Config Data — callback registry

// Memory Reset Result — callback registry
let _memoryResetResultCallback: ((payload: unknown) => void) | null = null;

// Memory Updated — callback registry
let _memoryUpdatedCallback: (() => void) | null = null;


// User Profile & Onboarding State
const userProfile = ref<Record<string, unknown> | null>(null);
const isProfileLoading = ref<boolean>(true);

let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let profileTimeout: ReturnType<typeof setTimeout> | null = null;

// Gửi message
const mapTauriResponse = (event: string, res: unknown, payload: unknown) => {
  switch (event) {
    case 'get_config':
    case 'update_config':
      applyConfigPayload(res);
      break;
    case 'get_ai_config':
    case 'update_ai_config':
      applyAIConfigPayload(res);
      break;
    case 'get_voice_status':
      applyVoiceStatusPayload(res);
      break;
    case 'get_voice_profiles':
      voiceProfiles.value = ((res as { profiles?: VoiceProfile[] })?.profiles || (res as VoiceProfile[]) || []) as VoiceProfile[];
      break;
    case 'voice:list_vieneu_voices':
    case 'voice:set_vieneu_voice':
      applyVieneuPayload(res);
      break;
    case 'consent:get':
    case 'consent:grant':
    case 'consent:revoke':
      applyConsentPayload(res);
      break;
    case 'get_system_status':
      systemStatus.value = (res as Partial<SystemStatus>) || {};
      break;
    case 'get_preflight_status':
      applyPreflightPayload(res);
      break;
    case 'get_skills_list':
      skillsList.value = ((res as { skills?: SkillInfo[] })?.skills || (res as SkillInfo[]) || []) as SkillInfo[];
      break;
    case 'get_user_profile':
    case 'update_user_profile':
      userProfile.value = (res as Record<string, unknown>) ?? {};
      isProfileLoading.value = false;
      if (profileTimeout) { clearTimeout(profileTimeout); profileTimeout = null; }
      break;
    case 'get_tasks':
      tasksList.value = ((res as { tasks?: TaskItem[] })?.tasks || (res as TaskItem[]) || []) as TaskItem[];
      break;
    case 'get_avatar_models':
      avatarModels3D.value = (res as { models3d?: AvatarModelInfo[] })?.models3d ?? [];
      avatarModels2D.value = (res as { models2d?: AvatarModelInfo[] })?.models2d ?? [];
      break;
    case 'get_memory_data':
      memoryData.value = (res as typeof memoryData.value) || { l0: [], l0_5: "", facts: [], events: [], vectors: [] };
      break;
    case 'delete_memory_fact':
      if ((res as { success?: boolean })?.success && (payload as { key?: string })?.key) {
        memoryData.value.facts = memoryData.value.facts.filter((f) => f.key !== (payload as { key: string }).key);
      }
      break;
    case 'consolidate_memory':
      if (_memoryUpdatedCallback) _memoryUpdatedCallback();
      break;
    case 'task_plan_chat':
      if (_taskPlanReplyCallback) _taskPlanReplyCallback(res as TaskPlanReplyPayload);
      break;
    case 'test_skill':
      if (_skillCheckResultCallback) _skillCheckResultCallback(res);
      break;
    case 'test_all_skills':
      if (_allSkillsCheckCompleteCallback) _allSkillsCheckCompleteCallback(res);
      break;
    case 'reset_memory':
    case 'memory:delete_subject':
      if (_memoryResetResultCallback) _memoryResetResultCallback(res);
      break;
    case 'memory_updated':
      if (_memoryUpdatedCallback) _memoryUpdatedCallback();
      break;
    case 'vision:ask':
      finishVision((res as { text?: string })?.text ?? '');
      break;
    case 'channels:list':
      channelsList.value = ((res as { channels?: ChannelItem[] })?.channels || (res as ChannelItem[]) || []) as ChannelItem[];
      break;
    case 'pairing:list':
    case 'pairing:list_nodes':
      pairedNodesList.value = ((res as { nodes?: PairedNodeInfo[] })?.nodes || (res as PairedNodeInfo[]) || []) as PairedNodeInfo[];
      break;
    case 'pairing:list_pending':
      pendingPairingList.value = ((res as { challenges?: PendingPairingChallenge[] })?.challenges || (res as PendingPairingChallenge[]) || []) as PendingPairingChallenge[];
      break;
    case 'browser:status':
      browserStatus.value = (res as BrowserStatus) || browserStatus.value;
      break;
    case 'browser:screenshot':
      if ((res as { base64Png?: string })?.base64Png) {
        browserScreenshot.value = (res as { base64Png: string }).base64Png;
      }
      break;
    case 'browser:action_log':
      browserActionLogs.value = ((res as { actions?: BrowserActionRecord[] })?.actions || (res as BrowserActionRecord[]) || []) as BrowserActionRecord[];
      break;
  }
};

const isTauri = typeof window !== "undefined" && (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ !== undefined;

export type GatewayPrincipal = 'widget' | 'dashboard' | 'remote';

const WIDGET_BOOTSTRAP_COMMANDS = [
  'get_config',
  'get_ai_config',
  'get_voice_status',
  'get_voice_profiles',
  'get_system_status',
  'get_user_profile',
  'get_avatar_models',
] as const;

const DASHBOARD_BOOTSTRAP_COMMANDS = [
  ...WIDGET_BOOTSTRAP_COMMANDS,
  'get_skills_list',
  'get_tasks',
  'get_memory_data',
] as const;

export const gatewayPrincipalForPath = (pathname: string): GatewayPrincipal => {
  const normalized = pathname.replace(/\/+$/, '');
  const entry = normalized.split('/').pop() ?? '';
  if (entry === '' || entry === 'widget.html') return 'widget';
  if (entry === 'dashboard.html') return 'dashboard';
  return 'remote';
};

export const bootstrapCommandsForPrincipal = (
  principal: GatewayPrincipal,
): readonly string[] => {
  if (principal === 'widget') return WIDGET_BOOTSTRAP_COMMANDS;
  if (principal === 'dashboard') return DASHBOARD_BOOTSTRAP_COMMANDS;
  return [];
};

const gatewayPrincipal: GatewayPrincipal = isTauri
  ? gatewayPrincipalForPath(typeof window === 'undefined' ? '' : window.location.pathname)
  : 'remote';

const requestBootstrapData = () => {
  for (const command of bootstrapCommandsForPrincipal(gatewayPrincipal)) {
    sendMsg(command);
  }
};

// Gửi message
const sendMsg = (event: WSClientEvent | string, payload: unknown = {}): boolean => {
  logger.debug('[useGateway] Sending event:', event);
  if (isTauri) {
    const isStream = payload && typeof payload === 'object' && (payload as Record<string, unknown>).stream === true;
    if (isStream) {
      const req_id = `req_${Math.random().toString(36).substring(2, 9)}`;
      import("@tauri-apps/api/event").then(({ listen }) => {
        listen(`ipc-stream:${req_id}`, (tauriEvent: { payload: unknown }) => {
          const data = tauriEvent.payload as { event?: string; payload?: TaskPlanReplyPayload; token?: string; done?: boolean } | null;
          logger.debug(`[useGateway] Stream chunk for ${req_id}:`, data);
          if (data) {
            if (data.event === 'task_plan_reply' || event === 'task_plan_chat') {
              if (_taskPlanReplyCallback) {
                _taskPlanReplyCallback((data.payload ?? data) as TaskPlanReplyPayload);
              }
            } else if (data.token) {
              if (_taskPlanReplyCallback) {
                _taskPlanReplyCallback({
                  taskId: (payload as Record<string, unknown>).taskId as string || '',
                  message: data.token,
                  done: data.done || false
                });
              }
            }
          }
        });
      });

      import("@tauri-apps/api/core").then(({ invoke }) => {
        invoke("native_ipc_call_stream", { command: event, payload, reqId: req_id })
          .then((res) => {
            logger.info(`[useGateway] Tauri IPC stream success: ${event}`, res);
            mapTauriResponse(event, res, payload);
          })
          .catch((err) => {
            logger.error(`[useGateway] Tauri IPC stream error: ${event}`, err);
          });
      });
    } else {
      import("@tauri-apps/api/core").then(({ invoke }) => {
        invoke("native_ipc_call", { command: event, payload })
          .then((res) => {
            logger.info(`[useGateway] Tauri IPC success: ${event}`, res);
            mapTauriResponse(event, res, payload);
          })
          .catch((err) => {
            logger.error(`[useGateway] Tauri IPC error: ${event}`, err);
            if (event === 'vision:ask') {
              finishVision('', err instanceof Error ? err.message : String(err));
            }
          });
      });
    }
    return true;
  }

    if (ws.value && ws.value.readyState === WebSocket.OPEN) {
      ws.value.send(JSON.stringify({ event, payload }));
      return true;
    }
    logger.warn('[useGateway]', `Cannot send '${event}' — socket not open (state=${ws.value?.readyState ?? 'null'})`);
    return false;
  };

const connect = () => {
  if (isTauri) {
    isConnected.value = true;
    isProfileLoading.value = false;
    requestBootstrapData();
    return;
  }

  if (ws.value) return;

  // Lấy IP host an toàn cho Tauri/Browser/localhost
  const host = window.location.hostname;
  const wsHost = !host || host === 'localhost' || host === '127.0.0.1' ? '127.0.0.1' : host;
  const wsUrl = `ws://${wsHost}:8002/ws`;
  const socket = new WebSocket(wsUrl);
  socket.binaryType = "arraybuffer";

  socket.onopen = () => {
    logger.info('[useGateway]', 'Đã kết nối với LIVA Core Engine');
    isConnected.value = true;
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    if (profileTimeout) {
      clearTimeout(profileTimeout);
      profileTimeout = null;
    }

    // Yêu cầu đẩy dữ liệu khởi tạo
    requestBootstrapData();

    if (profileTimeout) clearTimeout(profileTimeout);
    profileTimeout = setTimeout(() => {
      if (isProfileLoading.value) {
        logger.warn('[useGateway]', 'Profile timeout reached, releasing dashboard shell');
        isProfileLoading.value = false;
        if (!userProfile.value) userProfile.value = {};
      }
    }, 2500);
  };

  socket.onmessage = (event) => {
    let data;
    if (event.data instanceof ArrayBuffer) {
      const arrayBuffer = event.data;
      if (arrayBuffer.byteLength > 0) {
        const view = new DataView(arrayBuffer);
        const type = view.getUint8(0);
        if (type === 0x02) {
          try {
            data = unpack(new Uint8Array(arrayBuffer, 1));
          } catch (unpackErr) {
            logger.error('[useGateway]', 'Lỗi unpack MsgPack:', unpackErr);
            return;
          }
        } else {
          return; // Skip audio or other types
        }
      } else {
        return;
      }
    } else if (typeof event.data === "string") {
      try {
        data = JSON.parse(event.data);
      } catch (e) {
        logger.error('[useGateway]', 'Lỗi phân giải JSON:', e instanceof Error ? e.message : String(e));
        return;
      }
    } else {
      return;
    }

    try {
      logger.debug('[useGateway] Received WS event:', data.event);

      // Lõi trả `<lệnh>_error` khi handle_command thất bại. Bắt ở đây, TRƯỚC
      // switch, để mọi lệnh đều có đường báo lỗi mà không phải thêm case thủ
      // công cho từng cái. Không có nhánh này thì lệnh lỗi im lặng hoàn toàn
      // và người dùng chỉ thấy màn hình chờ tới lúc hết giờ.
      if (typeof data.event === 'string' && data.event.endsWith('_error')) {
        const failed = (data.payload?.command as string) ?? data.event.slice(0, -'_error'.length);
        const reason = (data.payload?.error as string) ?? 'Lỗi không rõ';
        logger.warn('[useGateway] Lệnh thất bại:', failed, reason);
        if (failed === 'vision:ask') finishVision('', reason);
        // Canh chừng màn hình: lỗi ở bất kỳ mắt xích nào (capture / add_region /
        // get_changed_regions) thì DỪNG hẳn thay vì poll tiếp vào lỗi lặp lại.
        if (failed === 'vision:capture' || failed === 'vision:add_region' || failed === 'vision:get_changed_regions') {
          stopWatchTimer();
          watchActive.value = false;
          watchStarting.value = false;
          watchError.value = reason;
        }
        return;
      }

      switch (data.event) {
        case 'user_profile':
          userProfile.value = data.payload ?? {};
          isProfileLoading.value = false;
          if (profileTimeout) { clearTimeout(profileTimeout); profileTimeout = null; }
          break;
        case 'profile_updated_success':
          userProfile.value = data.payload ?? {};
          isProfileLoading.value = false;
          if (profileTimeout) { clearTimeout(profileTimeout); profileTimeout = null; }
          break;
        case 'config_data':
        case 'config_updated':
          applyConfigPayload(data.payload);
          break;
        case 'ai_config':
        case 'ai_config_updated':
          applyAIConfigPayload(data.payload);
          break;
        case 'voice_status':
          applyVoiceStatusPayload(data.payload);
          break;
        case 'voice_profiles':
          voiceProfiles.value = data.payload?.profiles || data.payload || [];
          break;
        // Đường WebSocket trả `{lệnh}_response` qua nhánh mặc định của
        // websocket.rs. Bắt cả hai để màn chọn giọng chạy được ở CẢ hai profile
        // — vỏ Tauri lẫn trình duyệt thuần. Đúng bài học của U8: một tính năng
        // chỉ sống ở một profile là một tính năng người dùng thật không thấy.
        case 'voice:list_vieneu_voices_response':
        case 'voice:set_vieneu_voice_response':
          applyVieneuPayload(data.payload);
          break;
        case 'consent:get_response':
        case 'consent:grant_response':
        case 'consent:revoke_response':
          applyConsentPayload(data.payload);
          break;
        case 'avatar_models_list':
          avatarModels3D.value = (data.payload?.models3d as AvatarModelInfo[]) ?? [];
          avatarModels2D.value = (data.payload?.models2d as AvatarModelInfo[]) ?? [];
          break;
        case 'system_status':
          systemStatus.value = data.payload;
          break;
        case 'get_preflight_status_response':
          applyPreflightPayload(data.payload);
          break;
        case 'skills_list':
          skillsList.value = data.payload.skills || data.payload;
          break;
        case 'tasks_list':
          tasksList.value = data.payload.tasks || data.payload;
          break;
        case 'memory_data':
          memoryData.value = data.payload || { l0: [], l0_5: "", facts: [], events: [], vectors: [] };
          break;
        case 'consolidate_memory_response':
          if (_memoryUpdatedCallback) _memoryUpdatedCallback();
          break;
        case 'fact_deleted':
          if (data.payload?.success) {
            memoryData.value.facts = memoryData.value.facts.filter((f) => f.key !== data.payload.key);
          }
          break;
        case 'task_plan_reply':
          if (_taskPlanReplyCallback) _taskPlanReplyCallback(data.payload);
          break;
        case 'skill_check_result':
          if (_skillCheckResultCallback) _skillCheckResultCallback(data.payload);
          break;
        case 'all_skills_check_complete':
          if (_allSkillsCheckCompleteCallback) _allSkillsCheckCompleteCallback(data.payload);
          break;
        case 'memory_reset_result':
          if (_memoryResetResultCallback) _memoryResetResultCallback(data.payload);
          break;
        case 'memory_updated':
          if (_memoryUpdatedCallback) _memoryUpdatedCallback();
          break;
        case 'vision:ask_response':
          finishVision(data.payload?.text ?? '');
          break;
        case 'vision:capture_response': {
          // Chỉ dùng cho khởi động canh chừng: lấy kích thước khung VẬT LÝ rồi
          // đăng ký vùng toàn màn hình. Payload có cả ảnh PNG (~1 MB) nhưng
          // đây là một lần duy nhất lúc bật.
          if (!watchStarting.value) break;
          const w = data.payload?.width as number | undefined;
          const h = data.payload?.height as number | undefined;
          if (!Number.isInteger(w) || !Number.isInteger(h)) {
            watchStarting.value = false;
            watchError.value = 'vision:capture không trả kích thước khung';
            break;
          }
          sendMsg('vision:add_region', {
            id: WATCH_REGION_ID,
            name: 'Toàn màn hình',
            x: 0, y: 0, width: w, height: h,
            threshold: WATCH_THRESHOLD,
          });
          break;
        }
        case 'vision:add_region_response':
          if (!watchStarting.value) break;
          watchStarting.value = false;
          watchActive.value = true;
          stopWatchTimer();
          watchTimer = setInterval(() => { sendMsg('vision:get_changed_regions', {}); }, WATCH_POLL_MS);
          break;
        case 'vision:get_changed_regions_response': {
          if (!watchActive.value) break;
          const ket = Array.isArray(data.payload)
            ? (data.payload as Array<{ region_id: string; difference: number; is_changed: boolean }>)
                .find((r) => r.region_id === WATCH_REGION_ID)
            : undefined;
          if (!ket) break;
          watchLastDiff.value = ket.difference;
          if (ket.is_changed) {
            watchEvents.value.unshift({
              time: new Date().toLocaleTimeString('vi-VN', { hour12: false }),
              difference: ket.difference,
            });
            // Giữ 20 sự kiện gần nhất — danh sách này để liếc, không phải nhật ký.
            if (watchEvents.value.length > 20) watchEvents.value.length = 20;
          }
          break;
        }
        case 'gpu_setup_progress':
          gpuSetupStatus.value = data.payload.status;
          if (data.payload.status.includes('Hoàn tất') || data.payload.status.includes('thất bại') || 
              data.payload.status.includes('Complete') || data.payload.status.includes('Failed')) {
             setTimeout(() => { gpuSetupStatus.value = ''; }, 4000);
          }
          break;
      }
    } catch (e) {
      logger.error('[useGateway]', 'Lỗi phân giải JSON:', e instanceof Error ? e.message : String(e));
    }
  };

  socket.onclose = () => {
    isConnected.value = false;
    ws.value = null;
    logger.warn('[useGateway]', 'Mất kết nối. Đang thử lại sau 3s...');

    // Guard: clear any existing timer before scheduling a new one
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    reconnectTimer = setTimeout(() => {
      reconnectTimer = null;
      connect();
    }, 3000);
  };

  socket.onerror = (e) => {
    logger.error('[useGateway]', 'Lỗi mạng:', e instanceof Error ? e.message : String(e));
    socket.close();
  };

  ws.value = socket;
};

export function useGateway() {
  const init = () => {
    connect();
  };

  const destroy = () => {
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    if (profileTimeout) {
      clearTimeout(profileTimeout);
      profileTimeout = null;
    }
    if (ws.value) ws.value.close();
  };

  const updateConfig = (newConfig: Partial<LivaConfig>) => {
    sendMsg('update_config', newConfig);
  };

  const saveUserProfile = (profile: Record<string, unknown>) => {
    userProfile.value = { ...(profile ?? {}) };

    const sent = sendMsg('update_user_profile', profile);
    if (!sent) {
      logger.warn('[useGateway]', 'update_user_profile could not be sent');
    }
  };

  /** [P5] Expose raw WebSocket for one-time event listeners (e.g., memory reset) */
  const getRawWs = (): WebSocket | null => ws.value;

  /**
   * Ask the unified VL core (Qwen3-VL) about the screen via `vision:ask`.
   * With no `question`, the backend captures the screen (context-aware:
   * mouse-guided crop while gaming) and describes it. The answer lands in
   * `visionAnswer`; `visionBusy`/`visionError` track progress. Requires a
   * release build of the core (vision is disabled in debug).
   */
  const askVision = (question?: string) => {
    visionBusy.value = true;
    visionAnswer.value = '';
    visionError.value = '';
    if (visionTimeout) clearTimeout(visionTimeout);
    visionTimeout = setTimeout(() => finishVision('', 'timeout'), 120000);
    const payload: Record<string, unknown> = {};
    if (question && question.trim()) payload.question = question.trim();
    if (!sendMsg('vision:ask', payload)) finishVision('', 'not_connected');
  };

  /**
   * Bật canh chừng màn hình: chụp một khung để lấy kích thước vật lý + mồi
   * baseline, đăng ký vùng toàn màn hình, rồi poll thay đổi mỗi 3 giây.
   * Sự kiện dồn vào `watchEvents`, lỗi vào `watchError` (tự dừng khi lỗi).
   */
  const startScreenWatch = () => {
    if (watchActive.value || watchStarting.value) return;
    watchError.value = '';
    watchEvents.value = [];
    watchLastDiff.value = 0;
    watchStarting.value = true;
    if (!sendMsg('vision:capture', {})) {
      watchStarting.value = false;
      watchError.value = 'Chưa kết nối tới LIVA Core.';
    }
  };

  /** Tắt canh chừng: dừng poll và gỡ vùng đã đăng ký khỏi core. */
  const stopScreenWatch = () => {
    stopWatchTimer();
    if (watchActive.value) sendMsg('vision:remove_region', { id: WATCH_REGION_ID });
    watchActive.value = false;
    watchStarting.value = false;
  };

  /** [v25] Register callback for task planning AI replies */
  const onTaskPlanReply = (cb: (payload: TaskPlanReplyPayload) => void) => {
    _taskPlanReplyCallback = cb;
  };

  /** [v26] Register callback for skill self-test results */
  const onSkillCheckResult = (cb: (payload: unknown) => void) => {
    _skillCheckResultCallback = cb;
  };

  const offSkillCheckResult = () => {
    _skillCheckResultCallback = null;
  };

  const onAllSkillsCheckComplete = (cb: (payload: unknown) => void) => {
    _allSkillsCheckCompleteCallback = cb;
  };

  const offAllSkillsCheckComplete = () => {
    _allSkillsCheckCompleteCallback = null;
  };

  const onMemoryResetResult = (cb: (payload: unknown) => void) => {
    _memoryResetResultCallback = cb;
  };

  const offMemoryResetResult = () => {
    _memoryResetResultCallback = null;
  };

  const onMemoryUpdated = (cb: () => void) => {
    _memoryUpdatedCallback = cb;
  };

  const offMemoryUpdated = () => {
    _memoryUpdatedCallback = null;
  };

  const invokeCommand = async <T = unknown>(command: string, payload: unknown = {}): Promise<T> => {
    if (isTauri) {
      const { invoke } = await import("@tauri-apps/api/core");
      const res = await invoke<T>("native_ipc_call", { command, payload });
      mapTauriResponse(command, res, payload);
      return res;
    }
    return new Promise<T>((resolve) => {
      sendMsg(command, payload);
      resolve({} as T);
    });
  };

  // ─── Multi-Channel Management Helpers ───
  const fetchChannels = async () => {
    try {
      const res = await invokeCommand<{ count: number; channels: ChannelItem[] }>('channels:list');
      if (res?.channels) channelsList.value = res.channels;
      return channelsList.value;
    } catch (e) {
      logger.error('[useGateway] fetchChannels error:', e);
      return [];
    }
  };

  const configureChannel = async (channelId: string, config: unknown) => {
    const res = await invokeCommand<{ success: boolean; channel: ChannelItem }>('channels:configure', { channelId, config });
    await fetchChannels();
    return res;
  };

  const getWhatsAppQr = async () => {
    return await invokeCommand<{ qrData: string; expiresAtUnix: number; ttlSeconds: number; pairingState: string }>('channels:whatsapp_qr');
  };

  const startChannel = async (channelId: string) => {
    const res = await invokeCommand<{ success: boolean; channel: ChannelItem }>('channels:start', { channelId });
    await fetchChannels();
    return res;
  };

  const stopChannel = async (channelId: string) => {
    const res = await invokeCommand<{ success: boolean; channel: ChannelItem }>('channels:stop', { channelId });
    await fetchChannels();
    return res;
  };

  const testChannel = async (channelId: string) => {
    return await invokeCommand<{ channelId: string; success: boolean; latencyMs: number; status: unknown; message: string }>('channels:test', { channelId });
  };

  // ─── Node Pairing Monitor Helpers ───
  const fetchPairedNodes = async () => {
    try {
      const res = await invokeCommand<{ count: number; nodes: PairedNodeInfo[] }>('pairing:list_nodes');
      if (res?.nodes) pairedNodesList.value = res.nodes;
      return pairedNodesList.value;
    } catch (e) {
      logger.error('[useGateway] fetchPairedNodes error:', e);
      return [];
    }
  };

  const fetchPendingPairing = async () => {
    try {
      const res = await invokeCommand<{ count: number; challenges: PendingPairingChallenge[] }>('pairing:list_pending');
      if (res?.challenges) pendingPairingList.value = res.challenges;
      return pendingPairingList.value;
    } catch (e) {
      logger.error('[useGateway] fetchPendingPairing error:', e);
      return [];
    }
  };

  const approvePairing = async (payload: { shortCode?: string; challengeId?: string }) => {
    const res = await invokeCommand<{ success: boolean; paired: boolean; authToken?: string; serverPublicKey?: string; expiresAtUnix?: number }>('pairing:approve', payload);
    await fetchPairedNodes();
    await fetchPendingPairing();
    return res;
  };

  const rejectPairing = async (challengeId: string, reason?: string) => {
    const res = await invokeCommand<{ success: boolean; challengeId: string; reason?: string }>('pairing:reject', { challengeId, reason });
    await fetchPendingPairing();
    return res;
  };

  const revokePairing = async (nodeId: string) => {
    const res = await invokeCommand<{ success: boolean; nodeId: string; revoked: boolean }>('pairing:revoke', { nodeId });
    await fetchPairedNodes();
    return res;
  };

  const createPairingChallenge = async (nodeName = 'Companion Device', role = 'mobile_companion', publicKey = 'ed25519_client_key') => {
    const res = await invokeCommand<{ challengeId: string; shortCode: string; nodeId: string; nodeName: string; expiresAtUnix: number; qrPayload: string }>('pairing:create_challenge', { nodeName, role, publicKey });
    await fetchPendingPairing();
    return res;
  };

  // ─── Browser Automation Helpers ───
  const fetchBrowserStatus = async () => {
    try {
      const res = await invokeCommand<BrowserStatus>('browser:status');
      if (res) browserStatus.value = res;
      return browserStatus.value;
    } catch (e) {
      logger.error('[useGateway] fetchBrowserStatus error:', e);
      return browserStatus.value;
    }
  };

  const fetchBrowserScreenshot = async () => {
    try {
      const res = await invokeCommand<{ base64Png: string; width: number; height: number; timestampUnix: number }>('browser:screenshot');
      if (res?.base64Png) browserScreenshot.value = res.base64Png;
      return browserScreenshot.value;
    } catch (e) {
      logger.error('[useGateway] fetchBrowserScreenshot error:', e);
      return '';
    }
  };

  const fetchBrowserActionLogs = async () => {
    try {
      const res = await invokeCommand<{ count: number; actions: BrowserActionRecord[] }>('browser:action_log');
      if (res?.actions) browserActionLogs.value = res.actions;
      return browserActionLogs.value;
    } catch (e) {
      logger.error('[useGateway] fetchBrowserActionLogs error:', e);
      return [];
    }
  };

  const navigateBrowser = async (url: string) => {
    const res = await invokeCommand<{ success: boolean; url: string; title: string; httpStatus: number }>('browser:navigate', { url });
    await fetchBrowserStatus();
    await fetchBrowserScreenshot();
    await fetchBrowserActionLogs();
    return res;
  };

  const extractBrowserDom = async (mode = 'semantic') => {
    const res = await invokeCommand<{ mode: string; content: string; length: number }>('browser:extract', { mode });
    await fetchBrowserActionLogs();
    return res;
  };

  const controlBrowser = async (action: 'pause' | 'resume' | 'stop' | 'clear_logs') => {
    const res = await invokeCommand<{ success: boolean; state?: string; cleared?: boolean }>('browser:control', { action });
    await fetchBrowserStatus();
    await fetchBrowserActionLogs();
    return res;
  };

  // ─── Extended Skill Manifest & ClawHub Helpers ───
  const getSkillManifest = async (skillId: string) => {
    return await invokeCommand<SkillManifestInfo>('skills:get_manifest', { skillId });
  };

  const getSkillConfig = async (skillId: string) => {
    return await invokeCommand<{ skillId: string; params: Record<string, unknown>; schema: Record<string, unknown> }>('skills:get_config', { skillId });
  };

  const saveSkillConfig = async (skillId: string, params: unknown) => {
    return await invokeCommand<{ success: boolean; skillId: string; params: unknown }>('skills:save_config', { skillId, params });
  };

  const fetchSkillLogs = async (skillId?: string, limit = 20) => {
    return await invokeCommand<{ skillId: string; count: number; logs: SkillLogEntry[] }>('skills:logs', { skillId, limit });
  };

  const installSkillFromHub = async (name: string, repoUrl?: string) => {
    const res = await invokeCommand<{ success: boolean; skillId: string; name: string; installedPath: string }>('skills:install_from_hub', { name, repoUrl });
    sendMsg('get_skills_list');
    return res;
  };

  return {
    init,
    destroy,
    isConnected,
    configData,
    aiConfig,
    voiceStatus,
    voiceProfiles,
    vieneuVoices,
    vieneuCurrent,
    vieneuEnabled,
    vieneuNotice,
    observationConsent,
    systemStatus,
    preflightReport,
    skillsList,
    tasksList,
    avatarModels3D,
    avatarModels2D,
    gpuSetupStatus,
    userProfile,
    isProfileLoading,
    memoryData,
    channelsList,
    pairedNodesList,
    pendingPairingList,
    browserStatus,
    browserScreenshot,
    browserActionLogs,
    visionAnswer,
    visionBusy,
    watchActive,
    watchStarting,
    watchError,
    watchLastDiff,
    watchEvents,
    startScreenWatch,
    stopScreenWatch,
    visionError,
    askVision,
    updateConfig,
    saveUserProfile,
    sendMsg,
    invokeCommand,
    fetchChannels,
    configureChannel,
    getWhatsAppQr,
    startChannel,
    stopChannel,
    testChannel,
    fetchPairedNodes,
    fetchPendingPairing,
    approvePairing,
    rejectPairing,
    revokePairing,
    createPairingChallenge,
    fetchBrowserStatus,
    fetchBrowserScreenshot,
    fetchBrowserActionLogs,
    navigateBrowser,
    extractBrowserDom,
    controlBrowser,
    getSkillManifest,
    getSkillConfig,
    saveSkillConfig,
    fetchSkillLogs,
    installSkillFromHub,
    getRawWs,
    onTaskPlanReply,
    onSkillCheckResult,
    offSkillCheckResult,
    onAllSkillsCheckComplete,
    offAllSkillsCheckComplete,
    onMemoryResetResult,
    offMemoryResetResult,
    onMemoryUpdated,
    offMemoryUpdated
  };
}
