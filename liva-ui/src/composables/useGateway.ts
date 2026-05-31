import { ref } from "vue";
import { logger } from "../utils/logger";
import { pack, unpack } from "msgpackr";
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
const ws = ref<WebSocket | null>(null);

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
const memoryData = ref<{
  l0: any[];
  l0_5: string;
  facts: any[];
  events: any[];
  vectors: any[];
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
let _skillCheckResultCallback: ((payload: any) => void) | null = null;

// Env Config Data — callback registry
let _envConfigDataCallback: ((payload: any) => void) | null = null;

// Memory Reset Result — callback registry
let _memoryResetResultCallback: ((payload: any) => void) | null = null;

// Memory Updated — callback registry
let _memoryUpdatedCallback: (() => void) | null = null;


// User Profile & Onboarding State
const userProfile = ref<Record<string, unknown> | null>(null);
const isProfileLoading = ref<boolean>(true);

let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let profileTimeout: ReturnType<typeof setTimeout> | null = null;

// Gửi message
  const sendMsg = (event: WSClientEvent | string, payload: unknown = {}): boolean => {
    logger.info('[useGateway] Sending WS event:', event, payload);
    if (ws.value && ws.value.readyState === WebSocket.OPEN) {
      const packed = pack({ event, payload });
      const message = new Uint8Array(1 + packed.byteLength);
      message[0] = 0x02; // MessagePack event
      message.set(new Uint8Array(packed), 1);
      ws.value.send(message);
      return true;
    }
    logger.warn('[useGateway]', `Cannot send '${event}' — socket not open (state=${ws.value?.readyState ?? 'null'})`);
    return false;
  };

const connect = () => {
  if (ws.value) return;

  // Lấy IP host an toàn cho Tauri/Browser/localhost
  const host = window.location.hostname;
  const wsHost = !host || host === 'localhost' || host === '127.0.0.1' ? '127.0.0.1' : host;
  const wsUrl = `ws://${wsHost}:8082`;
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
      logger.info('[useGateway] Received WS event:', data.event, data.payload);
      
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
            memoryData.value.facts = memoryData.value.facts.filter((f: any) => f.key !== data.payload.key);
          }
          break;
        case 'task_plan_reply':
          if (_taskPlanReplyCallback) _taskPlanReplyCallback(data.payload);
          break;
        case 'skill_check_result':
          if (_skillCheckResultCallback) _skillCheckResultCallback(data.payload);
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

  /** [v25] Register callback for task planning AI replies */
  const onTaskPlanReply = (cb: (payload: TaskPlanReplyPayload) => void) => {
    _taskPlanReplyCallback = cb;
  };

  /** [v26] Register callback for skill self-test results */
  const onSkillCheckResult = (cb: (payload: any) => void) => {
    _skillCheckResultCallback = cb;
  };

  const offSkillCheckResult = () => {
    _skillCheckResultCallback = null;
  };

  const onEnvConfigData = (cb: (payload: any) => void) => {
    _envConfigDataCallback = cb;
  };

  const offEnvConfigData = () => {
    _envConfigDataCallback = null;
  };

  const onMemoryResetResult = (cb: (payload: any) => void) => {
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
    updateConfig,
    saveUserProfile,
    sendMsg,
    getRawWs,
    onTaskPlanReply,
    onSkillCheckResult,
    offSkillCheckResult,
    onEnvConfigData,
    offEnvConfigData,
    onMemoryResetResult,
    offMemoryResetResult,
    onMemoryUpdated,
    offMemoryUpdated
  };
}
