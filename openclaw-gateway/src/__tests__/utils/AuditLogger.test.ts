/**
 * @module AuditLogger Tests
 * Unit tests for the structured JSONL audit logger.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import * as fs from "fs";
import * as path from "path";

describe("AuditLogger", () => {
  let auditLogger: any;

  beforeEach(async () => {
    // Fresh import for each test
    const mod = await import("../../utils/AuditLogger");
    auditLogger = mod.auditLogger;
  });

  afterEach(() => {
    auditLogger?.dispose();
  });

  it("should record events without throwing", () => {
    expect(() => {
      auditLogger.record({
        eventType: "SYSTEM_EVENT",
        actor: "test",
        action: "Test event",
      });
    }).not.toThrow();
  });

  it("should accept all valid event types", () => {
    const eventTypes = [
      "DATA_ACCESS", "DATA_CAPTURE", "DATA_PURGE",
      "CONSENT_CHANGE", "TOOL_EXECUTION", "AUTH_SUCCESS",
      "AUTH_FAILURE", "SKILL_LOADED", "SKILL_TAMPER", "SYSTEM_EVENT"
    ];

    for (const eventType of eventTypes) {
      expect(() => {
        auditLogger.record({
          eventType,
          actor: "test",
          action: `Testing ${eventType}`,
        });
      }).not.toThrow();
    }
  });

  it("should provide convenience method for data access", () => {
    expect(() => {
      auditLogger.recordDataAccess("user", "Read clipboard", "SensoryManager");
    }).not.toThrow();
  });

  it("should provide convenience method for tool execution", () => {
    expect(() => {
      auditLogger.recordToolExecution("execute_command", { command: "echo test" }, "HIGH");
    }).not.toThrow();
  });

  it("should provide convenience method for auth events", () => {
    expect(() => {
      auditLogger.recordAuthEvent(true, "127.0.0.1", "WebSocket connection successful");
      auditLogger.recordAuthEvent(false, "192.168.1.100", "Invalid token");
    }).not.toThrow();
  });

  it("should provide convenience method for consent changes", () => {
    expect(() => {
      auditLogger.recordConsentChange(["clipboard", "activeWindow"], true);
      auditLogger.recordConsentChange(["clipboard"], false);
    }).not.toThrow();
  });

  it("should create audit log directory", () => {
    const logDir = path.resolve(process.cwd(), "logs", "audit");
    expect(fs.existsSync(logDir)).toBe(true);
  });
});
