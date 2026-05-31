import pino from "pino";
import * as fsp from "node:fs/promises";
import * as path from "node:path";
import { TraceContext } from "./TraceContext";

// Ensure log directory exists asynchronously (fire and forget to not block)
const logDir = path.resolve(process.cwd(), "logs");
try {
    const p = fsp.mkdir(logDir, { recursive: true });
    if (p && typeof p.catch === 'function') {
        p.catch(() => {});
    }
} catch {
    // ignore
}


const logFilePath = path.join(logDir, "ai_debug.log");

/**
 * LIVA Logger — Powered by Pino
 * ==============================
 * - Non-blocking async I/O (worker thread transport)
 * - Structured JSON output → ELK/Grafana-ready
 * - Console: pino-pretty (human-readable) in development
 * - File: JSON lines in production for machine parsing
 * - [Phase 4] TraceContext mixin: auto-injects traceId into every log line
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
    mixin: TraceContext.pinoMixin,
  },
  pino.transport({ targets })
);

logger.info("=== [SYSTEM START] Logger Initialized (Pino) ===");
