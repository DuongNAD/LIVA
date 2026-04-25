
// State lưu trữ kết nối
const isConnected = ref(false);
const ws = ref<WebSocket | null>(null);

// State Dữ liệu toàn cục (Single Source of Truth cho Dashboard)
const configData = ref<any>({});
const systemStatus = ref<any>({});
const skillsList = ref<any[]>([]);
const gpuSetupStatus = ref<string>('');

let reconnectTimer: any = null;

// Gửi message
const sendMsg = (event: string, payload: any = {}) => {
  if (ws.value && ws.value.readyState === WebSocket.OPEN) { // NOSONAR
    ws.value.send(JSON.stringify({ event, payload }));
  }
};

const connect = () => {
  if (ws.value) return;

  // UIController của Gateway mở cứng cổng 8082
  const socket = new WebSocket('ws://127.0.0.1:8082');

  socket.onopen = () => {
    console.log('[useGateway] Đã kết nối với LIVA Core Engine');
    isConnected.value = true;
    if (reconnectTimer) {
      clearInterval(reconnectTimer);
      reconnectTimer = null;
    }

    // Yêu cầu đẩy dữ liệu khởi tạo
    sendMsg('get_config');
    sendMsg('get_system_status');
    sendMsg('get_skills_list');
  };

  socket.onmessage = (event) => {
    // Bỏ qua buffer audio dạng nhị phân nếu có
    if (event.data instanceof Blob || event.data instanceof ArrayBuffer) return;

    try {
      const data = JSON.parse(event.data);
      
      switch (data.event) {
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
        case 'gpu_setup_progress':
          gpuSetupStatus.value = data.payload.status;
          if (data.payload.status.includes('Hoàn tất') || data.payload.status.includes('thất bại')) {
             setTimeout(() => { gpuSetupStatus.value = ''; }, 4000);
          }
          break;
      }
    } catch (e) {
      console.error('[useGateway] Lỗi phân giải JSON:', e);
    }
  };

  socket.onclose = () => {
    isConnected.value = false;
    ws.value = null;
    console.warn('[useGateway] Mất kết nối. Đang thử lại sau 3s...');
    
    if (!reconnectTimer) {
      reconnectTimer = setInterval(() => {
        connect();
      }, 3000);
    }
  };

  socket.onerror = (e) => {
    console.error('[useGateway] Lỗi mạng:', e);
    socket.close();
  };

  ws.value = socket;
};

// Auto-Refresh dữ liệu
let pingInterval: any = null;

export function useGateway() {
  const init = () => {
    connect();
    // Refresh system status mỗi 2s để hiển thị RAM real-time
    if (!pingInterval) {
      pingInterval = setInterval(() => {
        sendMsg('get_system_status');
      }, 2000);
    }
  };

  const destroy = () => {
    if (pingInterval) clearInterval(pingInterval);
    if (ws.value) ws.value.close();
  };

  const updateConfig = (newConfig: any) => {
    sendMsg('update_config', newConfig);
  };

  return {
    init,
    destroy,
    isConnected,
    configData,
    systemStatus,
    skillsList,
    gpuSetupStatus,
    updateConfig,
    sendMsg
  };
}
