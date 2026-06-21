import { EventEmitter } from "node:events";
import { exec } from "child_process";
import { promisify } from "util";
import { logger } from "../utils/logger";

const execAsync = promisify(exec);

export type PresenceState = "ACTIVE" | "AWAY";

/**
 * PresenceDetector — Monitors user idle activity on Windows.
 * If keyboard/mouse idle time exceeds the threshold, transitions to AWAY.
 * Activates ACTIVE state as soon as user activity is detected.
 */
export class PresenceDetector extends EventEmitter {
  private intervalId: NodeJS.Timeout | null = null;
  private currentPresence: PresenceState = "ACTIVE";
  private readonly idleThresholdMs: number;
  private readonly checkIntervalMs: number;

  constructor(idleThresholdMs: number = 180000, checkIntervalMs: number = 10000) {
    super();
    this.idleThresholdMs = idleThresholdMs;
    this.checkIntervalMs = checkIntervalMs;
  }

  /**
   * Get the current presence state.
   */
  public getPresence(): PresenceState {
    return this.currentPresence;
  }

  /**
   * Start the periodic presence checking.
   */
  public start() {
    if (this.intervalId) return;
    logger.info("[PresenceDetector] Periodic Windows user idle check started.");
    
    // Initial check
    this.checkPresence().catch(err => {
      logger.error(`[PresenceDetector] Initial check error: ${err.message}`);
    });
    
    this.intervalId = setInterval(() => {
      this.checkPresence().catch(err => {
        logger.error(`[PresenceDetector] Periodic check error: ${err.message}`);
      });
    }, this.checkIntervalMs);
  }

  /**
   * Stop the presence checking.
   */
  public stop() {
    if (this.intervalId) {
      clearInterval(this.intervalId);
      this.intervalId = null;
      logger.info("[PresenceDetector] Idle checking stopped.");
    }
  }

  /**
   * Query idle time and update presence accordingly.
   */
  private async checkPresence() {
    try {
      const idleTimeMs = await this.getCurrentIdleTime();
      if (idleTimeMs < 0) {
        // Fallback to ACTIVE on query failure
        this.updatePresence("ACTIVE");
        return;
      }

      logger.debug(`[PresenceDetector] Idle time queried: ${idleTimeMs} ms`);

      if (idleTimeMs >= this.idleThresholdMs) {
        this.updatePresence("AWAY");
      } else {
        this.updatePresence("ACTIVE");
      }
    } catch (error) {
      logger.error(`[PresenceDetector] Unhandled exception in checkPresence: ${error instanceof Error ? error.message : String(error)}`);
      this.updatePresence("ACTIVE");
    }
  }

  /**
   * Safe retrieval of system idle time using user32.dll GetLastInputInfo.
   * Compiles C# on the fly via powershell to execute Win32 API.
   * Correctly handles 32-bit tick count roll-overs by using unsigned integer subtraction.
   */
  private async getCurrentIdleTime(): Promise<number> {
    try {
      const psCommand = `powershell -NoProfile -NonInteractive -Command "$Signature = 'using System; using System.Runtime.InteropServices; public class Win32 { [DllImport(\\"user32.dll\\")] public static extern bool GetLastInputInfo(ref LASTINPUTINFO plii); [StructLayout(LayoutKind.Sequential)] public struct LASTINPUTINFO { public uint cbSize; public uint dwTime; } public static uint GetIdleTime() { LASTINPUTINFO lii = new LASTINPUTINFO(); lii.cbSize = (uint)Marshal.SizeOf(lii); if (GetLastInputInfo(ref lii)) { return (uint)Environment.TickCount - lii.dwTime; } return 0; } }'; Add-Type -TypeDefinition $Signature -ErrorAction SilentlyContinue; [Win32]::GetIdleTime();"`;

      const { stdout } = await execAsync(psCommand, { timeout: 3000 });
      const parsed = parseInt(stdout.trim(), 10);
      return isNaN(parsed) ? -1 : parsed;
    } catch (error) {
      logger.warn(`[PresenceDetector] PowerShell command failed: ${error instanceof Error ? error.message : String(error)}`);
      return -1;
    }
  }

  /**
   * Update internal state and emit change event if transitioned.
   */
  private updatePresence(nextPresence: PresenceState) {
    if (this.currentPresence !== nextPresence) {
      const previous = this.currentPresence;
      this.currentPresence = nextPresence;
      logger.info(`[PresenceDetector] Presence transition: ${previous} -> ${nextPresence}`);
      this.emit("presence_changed", { presence: nextPresence });
    }
  }
}
