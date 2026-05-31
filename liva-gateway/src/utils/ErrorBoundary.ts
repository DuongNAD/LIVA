import { logger } from "./logger";

/**
 * ErrorBoundary — Global process-level error safety net.
 *
 * Catches unhandled rejections and uncaught exceptions at the process level,
 * logging them structured via pino instead of crashing silently.
 *
 * MUST be called once at Gateway startup (Gateway.ts).
 *
 * Design decisions:
 *   - Only calls process.exit(1) for truly fatal errors (OOM, stack overflow)
 *   - Logs with full context (traceId via mixin, stack trace)
 *   - Prevents the default Node.js "unhandled rejection" warning from
 *     polluting stderr with unstructured text
 */

let initialized = false;

export function installErrorBoundary(): void {
    if (initialized) return;
    initialized = true;

    process.on("unhandledRejection", (reason: unknown) => {
        const errMsg = reason instanceof Error
            ? reason.stack || reason.message
            : String(reason);
        logger.error(
            { err: errMsg, context: "ErrorBoundary" },
            "⚠️ Unhandled Promise Rejection caught by ErrorBoundary"
        );
    });

    process.on("uncaughtException", (error: Error) => {
        logger.error(
            { err: error.stack || error.message, context: "ErrorBoundary" },
            "🚨 Uncaught Exception caught by ErrorBoundary"
        );
        // For truly fatal errors (OOM, stack overflow), let it crash
        // For everything else, log and continue
        if (error.message.includes("out of memory") || error.message.includes("Maximum call stack")) {
            logger.error("Fatal error detected — initiating emergency shutdown");
            process.exit(1);
        }
    });

    logger.info("[ErrorBoundary] Global error safety net installed.");
}
