/**
 * @module AuditLogger
 * [Legal Compliance — Luật BVDL Cá nhân VN 2026 & Luật AI VN 2026]
 * 
 * Structured JSON audit logger for tracking all AI data access events.
 * Every interaction with personal data, tool execution, and system event
 * is recorded as a machine-parseable JSON entry for compliance auditing.
 * 
 * Features:
 * - JSON Lines format (*.jsonl) for easy ingestion by log analysis tools
 * - Async writes (non-blocking) with write buffer
 * - Daily log rotation
 * - Categorized event types for filtering
 */

import * as fs from "fs";
import { promises as fsp } from "fs";
import * as path from "path";
import { logger } from "./logger";

export type AuditEventType = 
  | "DATA_ACCESS"          // AI accessed personal data
  | "DATA_CAPTURE"         // Sensory data captured
  | "DATA_PURGE"           // User invoked Right to be Forgotten
  | "CONSENT_CHANGE"       // User changed consent settings
  | "TOOL_EXECUTION"       // AI executed a skill/tool
  | "AUTH_SUCCESS"         // Successful authentication
  | "AUTH_FAILURE"         // Failed authentication attempt
  | "SKILL_LOADED"         // Dynamic skill loaded/reloaded
  | "SKILL_TAMPER"         // Skill file hash mismatch detected
  | "SYSTEM_EVENT";        // System lifecycle events

export interface AuditEntry {
  timestamp: string;       // ISO 8601
  epochMs: number;         // Unix timestamp for machine processing
  eventType: AuditEventType;
  actor: string;           // "system" | "user" | "agent" | IP address
  action: string;          // Human-readable action description
  target?: string;         // Resource being accessed/modified
  metadata?: Record<string, unknown>; // Additional structured data
  riskLevel?: "LOW" | "MEDIUM" | "HIGH" | "CRITICAL";
}

class AuditLogger {
  private logDir: string;
  private currentLogFile: string;
  private currentDate: string;
  private writeBuffer: string[] = [];
  private flushTimer: NodeJS.Timeout | null = null;
  private readonly FLUSH_INTERVAL_MS = 2000; // Flush every 2 seconds
  private readonly MAX_BUFFER_SIZE = 50;      // Force flush at 50 entries

  constructor() {
    this.logDir = path.resolve(process.cwd(), "logs", "audit");
    if (!fs.existsSync(this.logDir)) {
      fs.mkdirSync(this.logDir, { recursive: true });
    }
    this.currentDate = this.getDateString();
    this.currentLogFile = this.getLogFilePath();

    // Start periodic flush
    this.flushTimer = setInterval(() => this.flush(), this.FLUSH_INTERVAL_MS);
    this.flushTimer.unref(); // Don't prevent process exit
  }

  private getDateString(): string {
    return new Date().toISOString().split("T")[0]; // YYYY-MM-DD
  }

  private getLogFilePath(): string {
    return path.join(this.logDir, `audit_${this.currentDate}.jsonl`);
  }

  /**
   * Daily log rotation: If date has changed, switch to new file.
   */
  private rotateIfNeeded(): void {
    const today = this.getDateString();
    if (today !== this.currentDate) {
      this.flush(); // Flush remaining entries to old file
      this.currentDate = today;
      this.currentLogFile = this.getLogFilePath();
    }
  }

  /**
   * Record an audit event. Non-blocking — writes are buffered and flushed periodically.
   */
  public record(entry: Omit<AuditEntry, "timestamp" | "epochMs">): void {
    this.rotateIfNeeded();

    const fullEntry: AuditEntry = {
      timestamp: new Date().toISOString(),
      epochMs: Date.now(),
      ...entry,
    };

    this.writeBuffer.push(JSON.stringify(fullEntry));

    // Force flush if buffer is full
    if (this.writeBuffer.length >= this.MAX_BUFFER_SIZE) {
      this.flush();
    }
  }

  /**
   * Convenience methods for common event types.
   */
  public recordDataAccess(actor: string, action: string, target: string, metadata?: Record<string, unknown>): void {
    this.record({ eventType: "DATA_ACCESS", actor, action, target, metadata, riskLevel: "MEDIUM" });
  }

  public recordToolExecution(skillName: string, args: Record<string, unknown>, riskLevel: AuditEntry["riskLevel"] = "LOW"): void {
    this.record({
      eventType: "TOOL_EXECUTION",
      actor: "agent",
      action: `Executed skill: ${skillName}`,
      target: skillName,
      metadata: { args },
      riskLevel,
    });
  }

  public recordAuthEvent(success: boolean, actor: string, detail: string): void {
    this.record({
      eventType: success ? "AUTH_SUCCESS" : "AUTH_FAILURE",
      actor,
      action: detail,
      riskLevel: success ? "LOW" : "HIGH",
    });
  }

  public recordConsentChange(categories: string[], granted: boolean): void {
    this.record({
      eventType: "CONSENT_CHANGE",
      actor: "user",
      action: granted ? `Consent granted: ${categories.join(", ")}` : `Consent revoked: ${categories.join(", ")}`,
      metadata: { categories, granted },
      riskLevel: "MEDIUM",
    });
  }

  /**
   * Flush buffered entries to disk asynchronously.
   */
  private flush(): void {
    if (this.writeBuffer.length === 0) return;

    const data = this.writeBuffer.join("\n") + "\n";
    this.writeBuffer = [];

    // Async write — fire and forget to avoid blocking event loop
    fsp.appendFile(this.currentLogFile, data, "utf-8").catch((err) => {
      logger.error(
        { context: "AuditLogger", error: err instanceof Error ? err.message : String(err) },
        `Failed to write audit log`
      );
    });
  }

  /**
   * Cleanup: flush remaining entries and stop timer.
   */
  public dispose(): void {
    this.flush();
    if (this.flushTimer) {
      clearInterval(this.flushTimer);
      this.flushTimer = null;
    }
  }
}

export const auditLogger = new AuditLogger();
