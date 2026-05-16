import { ref } from "vue";
import { logger } from "../utils/logger";

// State lưu trữ kết nối
const isConnected = ref(false);
const ws = ref<WebSocket | null>(null);

// State Dữ liệu toàn cục (Single Source of Truth cho Dashboard)
const configData = ref<any>({});
const systemStatus = ref<any>({});
const skillsList = ref<any[]>([]);
const tasksList = ref<any[]>([]);
const gpuSetupStatus = ref<string>('');

// Task Planning Chat — callback registry for inline AI planning
let _taskPlanReplyCallback: ((payload: { taskId: string; message: string; done: boolean }) => void) | null = null;

// User Profile & Onboarding State
const userProfile = ref<any>(null);
const isProfileLoading = ref<boolean>(true);

let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

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

  // Lấy IP tĩnh của máy chủ host (hoạt động cho cả Localhost PC và Mobile LAN)
  const host = window.location.hostname === 'localhost' ? '127.0.0.1' : window.location.hostname;
  const wsUrl = `ws://${host}:8082`;
  const socket = new WebSocket(wsUrl);

  socket.onopen = () => {
    logger.info('[useGateway]', 'Đã kết nối với LIVA Core Engine');
    isConnected.value = true;
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }

    // Yêu cầu đẩy dữ liệu khởi tạo
    sendMsg('get_config');
    sendMsg('get_system_status');
    sendMsg('get_skills_list');
    sendMsg('get_user_profile');
    sendMsg('get_tasks');
  };

  socket.onmessage = (event) => {
    // Bỏ qua buffer audio dạng nhị phân nếu có
    if (event.data instanceof Blob || event.data instanceof ArrayBuffer) return;

    try {
      const data = JSON.parse(event.data);
      
      switch (data.event) {
        case 'user_profile':
          userProfile.value = data.payload;
          isProfileLoading.value = false;
          break;
        case 'profile_updated_success':
          userProfile.value = data.payload;
          isProfileLoading.value = false;
          break;
        case 'config_data':
        case 'config_updated':
          configData.value = data.payload || data; // handle direct config obj
          if (data.payload && data.payload.ai) { // NOSONAR
             configData.value = data.payload;
          } else {
             configData.value = data;
          }
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
    if (ws.value) ws.value.close();
  };

  const updateConfig = (newConfig: any) => {
    sendMsg('update_config', newConfig);
  };

  const saveUserProfile = (profile: any) => {
    isProfileLoading.value = true;
    sendMsg('update_user_profile', profile);
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
    systemStatus,
    skillsList,
    tasksList,
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
