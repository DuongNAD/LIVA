import { ref } from "vue";
import { logger } from "../utils/logger";

// State lưu trữ kết nối
const isConnected = ref(false);
const ws = ref<WebSocket | null>(null);

// State Dữ liệu toàn cục (Single Source of Truth cho Dashboard)
const configData = ref<Record<string, unknown>>({});
const aiConfig = ref<Record<string, unknown>>({});
const voiceStatus = ref<Record<string, unknown>>({});
const voiceProfiles = ref<Record<string, unknown>[]>([]);
const systemStatus = ref<Record<string, unknown>>({});
const skillsList = ref<Record<string, unknown>[]>([]);
const tasksList = ref<Record<string, unknown>[]>([]);
const avatarModels3D = ref<Record<string, unknown>[]>([]);
const avatarModels2D = ref<Record<string, unknown>[]>([]);
const gpuSetupStatus = ref<string>('');

const applyConfigPayload = (payload: unknown) => {
  if (payload && typeof payload === 'object' && !Array.isArray(payload)) {
    configData.value = payload as Record<string, unknown>;
  }
};

const applyAIConfigPayload = (payload: unknown) => {
  if (payload && typeof payload === 'object' && !Array.isArray(payload)) {
    aiConfig.value = (payload as { ai?: Record<string, unknown> }).ai ?? payload as Record<string, unknown>;
  }
};

const applyVoiceStatusPayload = (payload: unknown) => {
  if (payload && typeof payload === 'object' && !Array.isArray(payload)) {
    voiceStatus.value = (payload as { voice?: Record<string, unknown> }).voice ?? payload as Record<string, unknown>;
  }
};

// Task Planning Chat — callback registry for inline AI planning
let _taskPlanReplyCallback: ((payload: { taskId: string; message: string; done: boolean }) => void) | null = null;

// User Profile & Onboarding State
const userProfile = ref<any>(null);
const isProfileLoading = ref<boolean>(true);

let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let profileTimeout: ReturnType<typeof setTimeout> | null = null;

// Gửi message
  const sendMsg = (event: string, payload: any = {}): boolean => {
    if (ws.value && ws.value.readyState === WebSocket.OPEN) {
      ws.value.send(JSON.stringify({ event, payload }));
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
    // Bỏ qua buffer audio dạng nhị phân nếu có
    if (event.data instanceof Blob || event.data instanceof ArrayBuffer) return;

    try {
      const data = JSON.parse(event.data);
      
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
          avatarModels3D.value = (data.payload?.models3d as Record<string, unknown>[]) ?? [];
          avatarModels2D.value = (data.payload?.models2d as Record<string, unknown>[]) ?? [];
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
        case 'task_plan_reply':
          if (_taskPlanReplyCallback) _taskPlanReplyCallback(data.payload);
          break;
        case 'gpu_setup_progress':
          gpuSetupStatus.value = data.payload.status;
          if (data.payload.status.includes('Hoàn tất') || data.payload.status.includes('thất bại')) {
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

  const updateConfig = (newConfig: any) => {
    sendMsg('update_config', newConfig);
  };

  const saveUserProfile = (profile: any) => {
    isProfileLoading.value = true;
    userProfile.value = { ...(profile ?? {}) };

    const sent = sendMsg('update_user_profile', profile);
    if (!sent) {
      logger.warn('[useGateway]', 'update_user_profile could not be sent, releasing loading state locally');
      setTimeout(() => {
        isProfileLoading.value = false;
      }, 250);
      return;
    }

    setTimeout(() => {
      if (isProfileLoading.value) {
        logger.warn('[useGateway]', 'Profile update timeout reached, releasing loading state');
        isProfileLoading.value = false;
      }
    }, 2500);
  };

  /** [P5] Expose raw WebSocket for one-time event listeners (e.g., memory reset) */
  const getRawWs = (): WebSocket | null => ws.value;

  /** [v25] Register callback for task planning AI replies */
  const onTaskPlanReply = (cb: (payload: { taskId: string; message: string; done: boolean }) => void) => {
    _taskPlanReplyCallback = cb;
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
    updateConfig,
    saveUserProfile,
    sendMsg,
    getRawWs,
    onTaskPlanReply
  };
}
