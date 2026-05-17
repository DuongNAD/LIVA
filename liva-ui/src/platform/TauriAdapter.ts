import type { IPlatformAdapter } from "./IPlatformAdapter";

export class TauriAdapter implements IPlatformAdapter {
  platformName = 'tauri' as const;

  async getWindowSize() {
    return { width: window.innerWidth, height: window.innerHeight };
  }

  async toggleGhostMode(enabled: boolean) {
    // Dynamic import to avoid breaking when Tauri is not present
    try {
      // @ts-ignore
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('toggle_ghost_mode', { enabled });
    } catch (e) {
      console.warn('[TauriAdapter] toggleGhostMode not available', e);
    }
  }

  async minimizeToTray() {
    try {
      // @ts-ignore
      const { Window } = await import('@tauri-apps/api/window');
      const win = Window.getCurrent();
      await win.hide();
    } catch (e) {
      console.warn('[TauriAdapter] minimizeToTray not available', e);
    }
  }

  async quitApp() {
    try {
      // @ts-ignore
      const { exit } = await import('@tauri-apps/plugin-process');
      await exit(0);
    } catch (e) {
      console.warn('[TauriAdapter] quitApp not available', e);
    }
  }

  async readVaultKey(key: string) {
    try {
      // @ts-ignore
      const { invoke } = await import('@tauri-apps/api/core');
      return await invoke('read_vault_key', { key });
    } catch (e) {
      console.warn('[TauriAdapter] readVaultKey not available', e);
      return null;
    }
  }

  async writeVaultKey(key: string, value: string) {
    try {
      // @ts-ignore
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('write_vault_key', { key, value });
    } catch (e) {
      console.warn('[TauriAdapter] writeVaultKey not available', e);
    }
  }

  onGatewayReady(callback: (port: number, token: string | null) => void) {
    // @ts-ignore
    import('@tauri-apps/api/event').then(({ listen }) => {
      listen('gateway-ready', (event: any) => {
        callback(event.payload.port, event.payload.token);
      });
    }).catch(e => {
      console.warn('[TauriAdapter] Failed to listen to gateway-ready', e);
    });
  }

  async invokeBackend(command: string, args?: Record<string, unknown>) {
    try {
      // @ts-ignore
      const { invoke } = await import('@tauri-apps/api/core');
      return await invoke(command, args);
    } catch (e) {
      console.warn(`[TauriAdapter] invokeBackend(${command}) not available`, e);
      return null;
    }
  }
}
