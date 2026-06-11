import { exec } from "child_process";
import { promisify } from "util";
import { logger } from "../utils/logger";
import { UIController } from "../core/UIController";

const execAsync = promisify(exec);

export class PowerMonitorService {
  private intervalId: NodeJS.Timeout | null = null;
  private isEcoMode: boolean = false;
  private ui: UIController;

  constructor(ui: UIController) {
    this.ui = ui;
  }

  public start(intervalMs: number = 10000) {
    if (this.intervalId) return;
    logger.info("[PowerMonitor] Power monitor service started.");
    this.checkPowerStatus();
    this.intervalId = setInterval(() => this.checkPowerStatus(), intervalMs);
  }

  public stop() {
    if (this.intervalId) {
      clearInterval(this.intervalId);
      this.intervalId = null;
    }
  }

  private async checkPowerStatus() {
    try {
      const isDarwin = process.platform === "darwin";
      const cmd = isDarwin
        ? "pmset -g batt"
        : `powershell -Command "Get-CimInstance -ClassName Win32_Battery | Select-Object -Property EstimatedChargeRemaining, BatteryStatus | ConvertTo-Json"`;

      const { stdout } = await execAsync(cmd);
      const trimmed = stdout.trim();
      if (!trimmed) {
        // No battery detected, assume plugged in
        this.updateEcoMode(false);
        return;
      }

      let percent = 100;
      let isDischarging = false;

      // Check if output is JSON (Windows PowerShell or unit test mock)
      if (trimmed.startsWith("{") || trimmed.startsWith("[")) {
        const data = JSON.parse(trimmed);
        const batteries = Array.isArray(data) ? data : [data];
        if (batteries.length === 0 || !batteries[0]) {
          this.updateEcoMode(false);
          return;
        }

        const battery = batteries[0];
        if (battery.EstimatedChargeRemaining !== undefined) {
          percent = Number(battery.EstimatedChargeRemaining);
        }
        if (battery.BatteryStatus !== undefined) {
          isDischarging = battery.BatteryStatus === 1;
        }
      } else {
        // macOS pmset output parsing
        const match = trimmed.match(/(\d+)%/);
        percent = match ? parseInt(match[1], 10) : 100;
        isDischarging = trimmed.includes("discharging") || trimmed.includes("drawing from 'Battery Power'");
      }

      logger.debug(`[PowerMonitor] Battery status: ${percent}%, discharging: ${isDischarging}`);

      // Eco Mode criteria: running on battery (discharging) OR battery < 30%
      const shouldEco = isDischarging || percent < 30;

      this.updateEcoMode(shouldEco);
    } catch (error) {
      // Fallback: assume desktop / plugged in
      this.updateEcoMode(false);
    }
  }

  private updateEcoMode(enable: boolean) {
    if (this.isEcoMode === enable) return;
    this.isEcoMode = enable;
    
    logger.info(`[PowerMonitor] Eco Mode state changed: ${enable ? "ENABLED" : "DISABLED"}`);
    
    // Broadcast event to UI
    this.ui.broadcastUIEvent("eco_mode_changed", { enabled: enable, fps: enable ? 5 : 60 });
  }
}
