import type { IPlatformAdapter } from "./IPlatformAdapter";
import { logger } from "../utils/logger";

export class MockWebAdapter implements IPlatformAdapter {
  platformName = 'web' as const;
  private readonly vaultSecretKeys = new Set<string>();

  constructor() {
    if (typeof document !== 'undefined') {
      document.body.classList.add('web-mock-mode');
    }
  }

  async getWindowSize() {
    return { width: window.innerWidth, height: window.innerHeight };
  }

  async toggleGhostMode(enabled: boolean) {
    logger.debug('[MockWebAdapter]', `Toggle Ghost Mode: ${enabled}`);
  }

  async minimizeToTray() {
    logger.debug('[MockWebAdapter]', 'Minimize to tray requested.');
  }

  async quitApp() {
    logger.debug('[MockWebAdapter]', 'Quit app requested. Closing window.');
    window.close();
  }

  async hasVaultSecret(key: string) {
    return this.vaultSecretKeys.has(key);
  }

  async storeVaultSecret(key: string, value: string) {
    if (!value) throw new Error("vault secret must not be empty");
    this.vaultSecretKeys.add(key);
    logger.debug('[MockWebAdapter]', `Stored mock vault presence: ${key}`);
  }

  async deleteVaultSecret(key: string) {
    this.vaultSecretKeys.delete(key);
  }

  onGatewayReady(callback: (port: number, token: string | null) => void) {
    logger.info('[MockWebAdapter]', 'Emulating in-process Tauri IPC readiness (port 0)');
    // Simulate slight delay for boot
    setTimeout(() => {
      callback(0, null);
    }, 1000);
  }

  async invokeBackend(command: string, args?: Record<string, unknown>) {
    logger.debug('[MockWebAdapter]', `Invoked command: ${command}`, args);
    if (command === 'voice_subscribe') {
      return { success: true };
    }
    if (command === 'native_ipc_call') {
      return { success: true };
    }
    return null;
  }
}
