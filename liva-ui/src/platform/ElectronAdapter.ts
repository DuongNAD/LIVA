import type { IPlatformAdapter } from "./IPlatformAdapter";

export class ElectronAdapter implements IPlatformAdapter {
  platformName = 'electron' as const;

  async getWindowSize() {
    return { width: window.innerWidth, height: window.innerHeight };
  }

  async toggleGhostMode(enabled: boolean) {
    if ((window as any).electronAPI) {
      await (window as any).electronAPI.invoke('toggle-ghost-mode', enabled);
    }
  }

  async minimizeToTray() {
    if ((window as any).electronAPI) {
      await (window as any).electronAPI.invoke('minimize-to-tray');
    }
  }

  async quitApp() {
    if ((window as any).electronAPI) {
      await (window as any).electronAPI.invoke('quit-app');
    }
  }

  async readVaultKey(key: string) {
    if ((window as any).electronAPI) {
      return await (window as any).electronAPI.invoke('read-vault-key', key);
    }
    return null;
  }

  async writeVaultKey(key: string, value: string) {
    if ((window as any).electronAPI) {
      await (window as any).electronAPI.invoke('write-vault-key', key, value);
    }
  }

  onGatewayReady(callback: (port: number, token: string | null) => void) {
    if ((window as any).electronAPI) {
      (window as any).electronAPI.on('gateway-ready', (data: { port: number, token: string | null }) => {
        callback(data.port, data.token);
      });
    }
  }

  async invokeBackend(command: string, args?: Record<string, unknown>) {
    if ((window as any).electronAPI) {
      return await (window as any).electronAPI.invoke(command, args);
    }
    return null;
  }
}
