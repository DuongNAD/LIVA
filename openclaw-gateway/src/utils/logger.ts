import pino from "pino";
import * as fs from "node:fs";
import * as path from "node:path";

// Ensure log directory exists
const logDir = path.resolve(process.cwd(), "logs");
if (!fs.existsSync(logDir)) {
  fs.mkdirSync(logDir, { recursive: true });
}

const logFilePath = path.join(logDir, "ai_debug.log");

/**
 * LIVA Logger — Powered by Pino
 * ==============================
 * - Non-blocking async I/O (worker thread transport)
 * - Structured JSON output → ELK/Grafana-ready
 * - Console: pino-pretty (human-readable) in development
 * - File: JSON lines in production for machine parsing
 *
 * API surface matches previous custom Logger:
 *   logger.info(msg, meta?)
 *   logger.warn(msg, meta?)
 *   logger.error(msg, meta?)
 *   logger.debug(msg, meta?)
 */

const isProduction = process.env.NODE_ENV === "production";

// Build transport targets
const targets: pino.TransportTargetOptions[] = [
  // Target 1: File transport (always active, structured JSON)
  {
    target: "pino/file",
    options: { destination: logFilePath, mkdir: true },
    level: "debug",
  },
];

// Target 2: Console transport (pretty in dev, structured in prod)
// BẮT BUỘC CHUYỂN HƯỚNG RA STDERR (destination: 2) ĐỂ BẢO VỆ STDOUT IPC
if (!isProduction) {
  targets.push({
    target: "pino-pretty",
    options: {
      colorize: true,
      translateTime: "SYS:yyyy-mm-dd HH:MM:ss.l",
      ignore: "pid,hostname",
      destination: 2 // Mọi log dev đều vào stderr
    },
    level: "debug",
  });
} else {
  targets.push({
    target: "pino/file",
    options: { destination: 2 }, // Mọi log prod đều vào stderr
    level: "info",
  });
}

export const logger = pino(
  {
    level: process.env.LOG_LEVEL || "debug",
  },
  pino.transport({ targets })
);

logger.info("=== [SYSTEM START] Logger Initialized (Pino) ===");
