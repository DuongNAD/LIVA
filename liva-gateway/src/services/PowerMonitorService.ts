import { EventEmitter } from "node:events";
import { exec } from "child_process";
import { promisify } from "util";
import { logger } from "../utils/logger";
import { UIController } from "../core/UIController";

const execAsync = promisify(exec);

export class PowerMonitorService extends EventEmitter {
  private intervalId: NodeJS.Timeout | null = null;
  private isEcoMode: boolean = false;
  private isDischargingState: boolean = false;
  private ui: UIController;

  constructor(ui: UIController) {
    super();
    this.ui = ui;
  }

  public isEcoModeActive(): boolean {
    return this.isEcoMode;
  }

  public isBatteryModeActive(): boolean {
    return this.isDischargingState;
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
      // Run PowerShell query for battery
      const { stdout } = await execAsync(
        `powershell -Command "Get-CimInstance -ClassName Win32_Battery | Select-Object -Property EstimatedChargeRemaining, BatteryStatus | ConvertTo-Json"`
      );
      
      const trimmed = stdout.trim();
      if (!trimmed) {
        // No battery detected (likely a desktop PC), assume plugged in
        this.updateEcoMode(false);
        return;
      }

      const data = JSON.parse(trimmed);
      let percent = 100;
      let isDischarging = false;

      // JSON output from PowerShell can be an array of batteries or a single object
      const batteries = Array.isArray(data) ? data : [data];
      if (batteries.length === 0 || !batteries[0]) {
        this.updateEcoMode(false);
        return;
      }

      // Check the first battery
      const battery = batteries[0];
      if (battery.EstimatedChargeRemaining !== undefined) {
        percent = Number(battery.EstimatedChargeRemaining);
      }
      if (battery.BatteryStatus !== undefined) {
        // Win32_Battery BatteryStatus:
        // 1 = Discharging (running on battery), 2 = AC Power (charging/plugged in)
        isDischarging = battery.BatteryStatus === 1;
      }

      logger.debug(`[PowerMonitor] Battery status: ${percent}%, discharging: ${isDischarging}`);

      // Eco Mode criteria: running on battery (discharging) or low battery (< 30%)
      const shouldEco = isDischarging || percent < 30;

      this.updateEcoMode(shouldEco, percent, isDischarging);
    } catch (error) {
      // Fallback: assume desktop / plugged in
      this.updateEcoMode(false);
    }
  }

  private updateEcoMode(enable: boolean, percent?: number, isDischarging: boolean = false) {
    if (this.isEcoMode !== enable) {
      this.isEcoMode = enable;
      logger.info(`[PowerMonitor] Eco Mode state changed: ${enable ? "ENABLED" : "DISABLED"}`);
      this.ui.broadcastUIEvent("eco_mode_changed", { enabled: enable, fps: enable ? 5 : 60 });
      this.emit("eco_mode_changed", { enabled: enable, fps: enable ? 5 : 60 });
    }

    if (this.isDischargingState !== isDischarging) {
      this.isDischargingState = isDischarging;
      logger.info(`[PowerMonitor] Battery Mode state changed: ${isDischarging ? "ACTIVE" : "INACTIVE"}`);
      this.emit("battery_mode_changed", { active: isDischarging });
    }
  }
}
