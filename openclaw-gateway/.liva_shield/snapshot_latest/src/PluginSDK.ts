// Base Plugin System lấy cảm hứng từ kiến trúc AIRI (@proj-airi/plugin-sdk)
// Giúp cô lập các Skills / Hooks thành các Plugin độc lập hoàn toàn khỏi hệ thống Core.

export interface LivaPluginContext {
  sendToUI: (componentId: string, data: any) => void;
  readMemory: (query: string) => Promise<any[]>;
  saveMemory: (fact: string) => Promise<void>;
}

export interface LivaPluginManifest {
  id: string;
  name: string;
  version: string;
  permissions: string[]; // VD: ['disk_read', 'network']
}

export abstract class LivaPlugin {
  public abstract readonly manifest: LivaPluginManifest;

  /**
   * Bắt buộc phải implement để cung cấp các Skills.
   */
  public abstract getSkills(): any[];

  /**
   * Được gọi khi Plugin được load vào hệ thống LIVA
   */
  public onInstall(context: LivaPluginContext): void {
    console.log(
      `[Plugin System] Installing Plugin: ${this.manifest.name} (v${this.manifest.version})`,
    );
  }

  /**
   * Được gọi khi LIVA khởi động xong phần Core
   */
  public onReady(): void {
    // Init connections or background jobs
  }

  /**
   * Hủy các vòng lặp hoặc giải phóng RAM khi tắt Plugin
   */
  public onUninstall(): void {
    console.log(`[Plugin System] Uninstalling Plugin: ${this.manifest.name}`);
  }
}

/**
 * Hỗ trợ Builder Pattern để dev có thể định nghĩa Plugin dễ dàng
 */
export function definePlugin(
  manifest: LivaPluginManifest,
  setup: (ctx: LivaPluginContext) => { skills: any[]; onReady?: () => void },
): LivaPlugin {
  return new (class extends LivaPlugin {
    public manifest = manifest;
    private _skills: any[] = [];
    private _onReadyFn?: () => void;

    public getSkills() {
      return this._skills;
    }

    public onInstall(ctx: LivaPluginContext) {
      super.onInstall(ctx);
      const { skills, onReady } = setup(ctx);
      this._skills = skills;
      this._onReadyFn = onReady;
    }

    public onReady() {
      if (this._onReadyFn) this._onReadyFn();
    }
  })();
}
