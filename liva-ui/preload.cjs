/**
 * Electron Preload Script (Context Bridge)
 * =========================================
 * Cầu nối bảo mật giữa Vue Frontend và Electron Main Process.
 * contextIsolation: true → Vue không thể truy cập trực tiếp Node.js/Electron API.
 * Tất cả IPC phải đi qua electronAPI được expose ở đây.
 */
const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('electronAPI', {
  // === Window Controls ===
  openDashboard: () => ipcRenderer.send('open-dashboard'),
  closeDashboard: () => ipcRenderer.send('close-dashboard'),
  minimizeWindow: () => ipcRenderer.send('minimize-window'),
  maximizeWindow: () => ipcRenderer.send('maximize-window'),
  moveWindow: (dx, dy) => ipcRenderer.send('move-window', dx, dy),

  // === Mouse Passthrough (Phantom Bounding Box Fix) ===
  setIgnoreMouse: (ignore) => ipcRenderer.send('set-ignore-mouse-events', ignore),

  // === Global Mouse Position (for LookAt) ===
  getMousePosition: () => ipcRenderer.invoke('get-mouse-position'),

  // === Avatar Hot-Swap ===
  onAvatarChanged: (callback) => {
    ipcRenderer.on('avatar-changed', (_event, config) => callback(config));
  },
  changeAvatarConfig: (config) => ipcRenderer.send('avatar-config-changed', config),

  // === Config ===
  onConfigUpdated: (callback) => {
    ipcRenderer.on('config-updated', (_event, config) => callback(config));
  },

  // === Cleanup ===
  removeAllListeners: (channel) => {
    ipcRenderer.removeAllListeners(channel);
  }
});
