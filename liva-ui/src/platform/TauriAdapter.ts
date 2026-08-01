import type { IPlatformAdapter } from "./IPlatformAdapter";
import { logger } from '../utils/logger';

export class TauriAdapter implements IPlatformAdapter {
  platformName = 'tauri' as const;

  async getWindowSize() {
    return { width: window.innerWidth, height: window.innerHeight };
  }

  async toggleGhostMode(enabled: boolean) {
    // Dynamic import to avoid breaking when Tauri is not present
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('toggle_ghost_mode', { enabled });
    } catch (e) {
      logger.warn('[TauriAdapter] toggleGhostMode not available', e);
    }
  }

  async minimizeToTray() {
    try {
      const { Window } = await import('@tauri-apps/api/window');
      const win = Window.getCurrent();
      await win.hide();
    } catch (e) {
      logger.warn('[TauriAdapter] minimizeToTray not available', e);
    }
  }

  async quitApp() {
    try {
      const { exit } = await import('@tauri-apps/plugin-process');
      await exit(0);
    } catch (e) {
      logger.warn('[TauriAdapter] quitApp not available', e);
    }
  }

  async hasVaultSecret(key: string): Promise<boolean> {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      return await invoke<boolean>('vault_secret_present', { key });
    } catch (e) {
      logger.warn('[TauriAdapter] hasVaultSecret not available', e);
      return false;
    }
  }

  async storeVaultSecret(key: string, value: string) {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('store_vault_secret', { key, value });
  }

  async deleteVaultSecret(key: string) {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('delete_vault_secret', { key });
  }

  onGatewayReady(callback: (port: number, token: string | null) => void) {
    import('@tauri-apps/api/event').then(({ listen }) => {
      listen('gateway-ready', (event: { payload: { port: number; token: string | null } }) => {
        callback(event.payload.port, event.payload.token);
      });
    }).catch(e => {
      logger.warn('[TauriAdapter] Failed to listen to gateway-ready', e);
    });
  }

  async invokeBackend(command: string, args?: Record<string, unknown>) {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      return await invoke(command, args);
    } catch (e) {
      logger.warn(`[TauriAdapter] invokeBackend(${command}) not available`, e);
      return null;
    }
  }
}
