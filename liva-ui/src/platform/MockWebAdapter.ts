import type { IPlatformAdapter } from "./IPlatformAdapter";
import { logger } from "../utils/logger";

export class MockWebAdapter implements IPlatformAdapter {
  platformName = 'web' as const;

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

  async readVaultKey(key: string) {
    return localStorage.getItem(`liva_vault_${key}`);
  }

  async writeVaultKey(key: string, value: string) {
    localStorage.setItem(`liva_vault_${key}`, value);
    logger.debug('[MockWebAdapter]', `Wrote to localStorage: vault_${key}`);
  }

  onGatewayReady(callback: (port: number, token: string | null) => void) {
    logger.info('[MockWebAdapter]', 'Emulating GATEWAY_READY handshake on port 8082');
    // Simulate slight delay for boot
    setTimeout(() => {
      callback(8082, null);
    }, 1000);
  }

  async invokeBackend(command: string, args?: Record<string, unknown>) {
    logger.debug('[MockWebAdapter]', `Invoked command: ${command}`, args);
    return null;
  }
}
