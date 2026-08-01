export interface IPlatformAdapter {
  readonly platformName: 'tauri' | 'web';
  
  getWindowSize(): Promise<{ width: number; height: number }>;
  toggleGhostMode(enabled: boolean): Promise<void>;
  minimizeToTray(): Promise<void>;
  quitApp(): Promise<void>;
  
  hasVaultSecret(key: string): Promise<boolean>;
  storeVaultSecret(key: string, value: string): Promise<void>;
  deleteVaultSecret(key: string): Promise<void>;
  
  // IPC methods
  onGatewayReady(callback: (port: number, token: string | null) => void): void;
  invokeBackend(command: string, args?: Record<string, unknown>): Promise<unknown>;
}
