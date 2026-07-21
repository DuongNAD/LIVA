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
const systemStatus = ref<Partial<SystemStatus>>({});
const skillsList = ref<SkillInfo[]>([]);
const tasksList = ref<TaskItem[]>([]);
const avatarModels3D = ref<AvatarModelInfo[]>([]);
const avatarModels2D = ref<AvatarModelInfo[]>([]);
const gpuSetupStatus = ref<string>('');

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

// Task Planning Chat — callback registry for inline AI planning
let _taskPlanReplyCallback: ((payload: TaskPlanReplyPayload) => void) | null = null;

// Skill Check Result — callback registry for self-test results
let _skillCheckResultCallback: ((payload: unknown) => void) | null = null;

// Bulk Skill Check Complete — callback registry
let _allSkillsCheckCompleteCallback: ((payload: unknown) => void) | null = null;

// Env Config Data — callback registry
let _envConfigDataCallback: ((payload: unknown) => void) | null = null;

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
    case 'get_system_status':
      systemStatus.value = (res as Partial<SystemStatus>) || {};
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
    case 'task_plan_chat':
      if (_taskPlanReplyCallback) _taskPlanReplyCallback(res as TaskPlanReplyPayload);
      break;
    case 'test_skill':
      if (_skillCheckResultCallback) _skillCheckResultCallback(res);
      break;
    case 'test_all_skills':
      if (_allSkillsCheckCompleteCallback) _allSkillsCheckCompleteCallback(res);
      break;
    case 'get_env_config':
      if (_envConfigDataCallback) _envConfigDataCallback(res);
      break;
    case 'reset_memory':
      if (_memoryResetResultCallback) _memoryResetResultCallback(res);
      break;
    case 'memory_updated':
      if (_memoryUpdatedCallback) _memoryUpdatedCallback();
      break;
    case 'vision:ask':
      finishVision((res as { text?: string })?.text ?? '');
      break;
  }
};

const isTauri = typeof window !== "undefined" && (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ !== undefined;

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
    sendMsg('get_config');
    sendMsg('get_ai_config');
    sendMsg('get_voice_status');
    sendMsg('get_voice_profiles');
    sendMsg('get_system_status');
    sendMsg('get_skills_list');
    sendMsg('get_user_profile');
    sendMsg('get_tasks');
    sendMsg('get_avatar_models');
    sendMsg('get_memory_data');
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
    sendMsg('get_config');
    sendMsg('get_ai_config');
    sendMsg('get_voice_status');
    sendMsg('get_voice_profiles');
    sendMsg('get_system_status');
    sendMsg('get_skills_list');
    sendMsg('get_user_profile');
    sendMsg('get_tasks');
    sendMsg('get_avatar_models');
    sendMsg('get_memory_data');

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
        case 'avatar_models_list':
          avatarModels3D.value = (data.payload?.models3d as AvatarModelInfo[]) ?? [];
          avatarModels2D.value = (data.payload?.models2d as AvatarModelInfo[]) ?? [];
          break;
        case 'system_status':
          systemStatus.value = data.payload;
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
        case 'env_config_data':
          if (_envConfigDataCallback) _envConfigDataCallback(data.payload);
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

  const onEnvConfigData = (cb: (payload: unknown) => void) => {
    _envConfigDataCallback = cb;
  };

  const offEnvConfigData = () => {
    _envConfigDataCallback = null;
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

  return {
    init,
    destroy,
    isConnected,
    configData,
    aiConfig,
    voiceStatus,
    voiceProfiles,
    systemStatus,
    skillsList,
    tasksList,
    avatarModels3D,
    avatarModels2D,
    gpuSetupStatus,
    userProfile,
    isProfileLoading,
    memoryData,
    visionAnswer,
    visionBusy,
    visionError,
    askVision,
    updateConfig,
    saveUserProfile,
    sendMsg,
    getRawWs,
    onTaskPlanReply,
    onSkillCheckResult,
    offSkillCheckResult,
    onAllSkillsCheckComplete,
    offAllSkillsCheckComplete,
    onEnvConfigData,
    offEnvConfigData,
    onMemoryResetResult,
    offMemoryResetResult,
    onMemoryUpdated,
    offMemoryUpdated
  };
}
