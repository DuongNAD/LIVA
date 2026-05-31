/**
 * logger.ts — Browser-compatible structured logger for LIVA UI
 * ============================================================
 * Wraps console methods with structured prefixes and log levels.
 * Uses a namespace (channel) for filtering in browser DevTools.
 *
 * Usage: import { logger } from "../utils/logger";
 *        logger.info("[Widget]", "Wake word detected");
 *        logger.error("[WakeWord]", "Worker error:", error);
 */
type LogLevel = "debug" | "info" | "warn" | "error";

function format(level: LogLevel, channel: string, ...args: unknown[]): void {
  const prefix = `[LIVA][${level.toUpperCase()}]${channel ? `[${channel}]` : ""}`;
  const fn = console[level] ?? console.log;
  fn(prefix, ...args);
}

export const logger = {
  debug: (channel: string, ...args: unknown[]) => format("debug", channel, ...args),
  info:  (channel: string, ...args: unknown[]) => format("info",  channel, ...args),
  warn:  (channel: string, ...args: unknown[]) => format("warn",  channel, ...args),
  error: (channel: string, ...args: unknown[]) => format("error", channel, ...args),
};
