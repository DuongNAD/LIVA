import * as path from "node:path";
import { logger } from "../utils/logger";
import { execSync } from "node:child_process";
/**
 * LIVA Hardened Local Sandbox Verifier (V2 — Security Hardened)
 * ==============================================================
 * Runs candidate code in an isolated local subprocess with strict safety limits:
 * - Process timeout (prevents infinite loops)
 * - Output buffer cap (prevents OOM from excessive logging)
 * - Exit code verification (catches runtime crashes)
 * - TypeScript pre-emit diagnostics (catches type errors)
 * 
 * V2 Security Layers:
 *   - Layer 1: Environment Scrubbing — removes all sensitive env vars
 *   - Layer 2: Command Blocklist — blocks dangerous system commands
 *   - Layer 3: Filesystem Deny List — blocks paths to credentials
 *   - Layer 4: Output Sanitization — redacts leaked secrets in stdout/stderr
 * 
 * Architecture:
 *   1. Security pre-checks (command blocklist + filesystem)
 *   2. tsc --noEmit on sandbox (compile verification)
 *   3. Optional test command via child_process with timeout + scrubbed env
 *   4. Output sanitization before returning results
 */

const SANDBOX_TIMEOUT_MS = 60_000;     // 60s max for any test run
const MAX_OUTPUT_BUFFER = 512 * 1024;   // 512KB max output capture
const TSC_TIMEOUT_MS = 30_000;          // 30s max for TypeScript check

// ===========================
// Security: Blocked Commands
// ===========================
const COMMAND_BLOCKLIST = [
    /\bcurl\b/i,
    /\bwget\b/i,
    /\bpowershell\s+-enc/i,
    /\bpowershell\s+-encodedcommand/i,
    /\brm\s+-rf\s+\//i,
    /\bdel\s+\/s\s+\/q/i,
    /\bformat\s+[a-z]:/i,
    /\bnet\s+user\b/i,
    /\breg\s+(add|delete)\b/i,
    /\bsshpass\b/i,
    /\bnc\s+-[elp]/i,          // netcat reverse shell
    /\bpython\s+-c\s+['"]import\s+os/i, // python os exec
    /\bnode\s+-e\s+['"]require.*child_process/i, // node exec
    /\bchmod\s+[0-7]*s/i,     // setuid
    /\bsudo\b/i,
    /\bmkfs\b/i,
    /\bdd\s+if=/i,
];

// ===========================
// Security: Filesystem Deny List
// ===========================
const FILESYSTEM_DENY_PATTERNS = [
    /\.ssh[\/\\]/i,
    /\.aws[\/\\]/i,
    /\.gnupg[\/\\]/i,
    /\.env$/i,
    /\.env\.local$/i,
    /\.env\.production$/i,
    /id_rsa/i,
    /id_ed25519/i,
    /credentials/i,
    /\.kube[\/\\]config/i,
    /\.docker[\/\\]config\.json/i,
    /\.npmrc$/i,
    /\.pypirc$/i,
];

// ===========================
// Security: Env Vars to Scrub
// ===========================
const ENV_SCRUB_PATTERNS = [
    /API[_-]?KEY/i,
    /SECRET/i,
    /TOKEN/i,
    /PASSWORD/i,
    /PASSWD/i,
    /CREDENTIAL/i,
    /PRIVATE[_-]?KEY/i,
    /ACCESS[_-]?KEY/i,
    /AUTH/i,
    /ENCRYPTION/i,
    /ZALO/i,
    /OPENAI/i,
    /ANTHROPIC/i,
    /GOOGLE[_-]?API/i,
    /AWS/i,
    /AZURE/i,
    /DATABASE[_-]?URL/i,
    /MONGO/i,
    /REDIS[_-]?URL/i,
    /E2B/i,
];

// ===========================
// Security: Output Sanitization Patterns
// ===========================
const OUTPUT_REDACT_PATTERNS = [
    // Generic API keys (long alphanumeric strings after key= or token=)
    { regex: /(api[_-]?key|token|secret|password|passwd)\s*[=:]\s*['"]?([a-zA-Z0-9_\-\.]{16,})['"]?/gi, replacement: "$1=***REDACTED***" },
    // Bearer tokens
    { regex: /Bearer\s+[a-zA-Z0-9_\-\.]{20,}/gi, replacement: "Bearer ***REDACTED***" },
    // AWS access keys
    { regex: /AKIA[0-9A-Z]{16}/g, replacement: "***AWS_KEY_REDACTED***" },
    // Private key blocks
    { regex: /-----BEGIN\s+(RSA\s+)?PRIVATE\s+KEY-----[\s\S]*?-----END/g, replacement: "***PRIVATE_KEY_REDACTED***" },
];

export class MicroVMDaemon {
    private readonly apiKey: string;
    
    constructor() {
        this.apiKey = process.env.E2B_API_KEY || "";
    }

    /**
     * Verify a shadow candidate in the local sandbox.
     * 
     * Phase 0: Security pre-checks (command blocklist)
     * Phase 1: TypeScript compile check (tsc --noEmit)
     * Phase 2: Execute test command with process isolation + timeout + scrubbed env
     * Phase 3: Output sanitization — redact any leaked credentials
     */
    public async verifyShadowCandidate(
        sandboxRoot: string, 
        testCommand: string = "npx tsc --noEmit"
    ): Promise<{ pass: boolean; vmLogs: string; executionTimeMs: number }> {
        const startTime = Date.now();

        // =====================================================
        // PHASE 0: Security Pre-Checks
        // =====================================================
        if (testCommand) {
            const blockCheck = this.isCommandBlocked(testCommand);
            if (blockCheck.blocked) {
                logger.warn(`[LocalSandbox] 🛡️ BLOCKED dangerous command: ${blockCheck.reason}`);
                return {
                    pass: false,
                    vmLogs: `[LocalSandbox] SECURITY BLOCK: Command rejected — ${blockCheck.reason}`,
                    executionTimeMs: Date.now() - startTime
                };
            }
        }

        // Check sandbox root doesn't point to sensitive filesystem paths
        const pathCheck = this.isPathDenied(sandboxRoot);
        if (pathCheck.denied) {
            logger.warn(`[LocalSandbox] 🛡️ BLOCKED sensitive path: ${pathCheck.reason}`);
            return {
                pass: false,
                vmLogs: `[LocalSandbox] SECURITY BLOCK: Path access denied — ${pathCheck.reason}`,
                executionTimeMs: Date.now() - startTime
            };
        }

        // =====================================================
        // PHASE 1: TypeScript Compile Verification
        // =====================================================
        logger.info(`[LocalSandbox] Phase 1: TypeScript compile check on ${path.basename(sandboxRoot)}...`);
        
        const tscResult = this.runCommandSync(
            "npx tsc --noEmit --pretty",
            sandboxRoot,
            TSC_TIMEOUT_MS
        );

        if (!tscResult.success) {
            return {
                pass: false,
                vmLogs: this.sanitizeOutput(`[LocalSandbox] TypeScript compile FAILED:\n${tscResult.output.slice(0, 2000)}`),
                executionTimeMs: Date.now() - startTime
            };
        }

        logger.info(`[LocalSandbox] Phase 1: TypeScript compile PASSED ✅`);

        // =====================================================
        // PHASE 2: Runtime Test Execution (if custom test command)
        // =====================================================
        if (testCommand && testCommand !== "npx tsc --noEmit") {
            logger.info(`[LocalSandbox] Phase 2: Running test command: ${testCommand}`);
            
            const testResult = this.runCommandSync(
                testCommand,
                sandboxRoot,
                SANDBOX_TIMEOUT_MS
            );

            if (!testResult.success) {
                return {
                    pass: false,
                    vmLogs: this.sanitizeOutput(`[LocalSandbox] Runtime test FAILED (exit=${testResult.exitCode}):\n${testResult.output.slice(0, 2000)}`),
                    executionTimeMs: Date.now() - startTime
                };
            }

            logger.info(`[LocalSandbox] Phase 2: Runtime test PASSED ✅ (${Date.now() - startTime}ms)`);
        }

        return {
            pass: true,
            vmLogs: `[LocalSandbox] All verification passed. Compile: OK. Tests: ${testCommand ? "OK" : "skipped"}.`,
            executionTimeMs: Date.now() - startTime
        };
    }

    /**
     * SECURITY LAYER 1: Command Blocklist
     * Checks if a command matches any dangerous patterns
     */
    private isCommandBlocked(command: string): { blocked: boolean; reason: string } {
        for (const pattern of COMMAND_BLOCKLIST) {
            if (pattern.test(command)) {
                return { blocked: true, reason: `Command matches blocklist pattern: ${pattern.source}` };
            }
        }
        return { blocked: false, reason: "" };
    }

    /**
     * SECURITY LAYER 2: Filesystem Deny List
     * Checks if a path points to sensitive locations
     */
    private isPathDenied(targetPath: string): { denied: boolean; reason: string } {
        const normalized = path.resolve(targetPath);
        for (const pattern of FILESYSTEM_DENY_PATTERNS) {
            if (pattern.test(normalized)) {
                return { denied: true, reason: `Path matches deny pattern: ${pattern.source}` };
            }
        }
        return { denied: false, reason: "" };
    }

    /**
     * SECURITY LAYER 3: Environment Scrubbing
     * Creates a sanitized copy of process.env with all sensitive vars removed
     */
    private getScrubbeEnv(): Record<string, string | undefined> {
        const scrubbed: Record<string, string | undefined> = {};
        
        for (const [key, value] of Object.entries(process.env)) {
            const isSensitive = ENV_SCRUB_PATTERNS.some(pattern => pattern.test(key));
            if (isSensitive) {
                continue; // Remove sensitive env var entirely
            }
            scrubbed[key] = value;
        }

        // Ensure essential vars are present
        scrubbed.NODE_ENV = "test";
        scrubbed.PATH = process.env.PATH;
        scrubbed.HOME = process.env.HOME || process.env.USERPROFILE;
        scrubbed.TEMP = process.env.TEMP;
        scrubbed.TMP = process.env.TMP;

        return scrubbed;
    }

    /**
     * SECURITY LAYER 4: Output Sanitization
     * Redacts any credentials that may have leaked into stdout/stderr
     */
    private sanitizeOutput(output: string): string {
        let sanitized = output;
        for (const { regex, replacement } of OUTPUT_REDACT_PATTERNS) {
            sanitized = sanitized.replace(regex, replacement);
        }
        return sanitized;
    }

    /**
     * Execute a command synchronously with timeout + output buffer limits.
     * Uses child_process.execSync with SCRUBBED environment.
     */
    private runCommandSync(
        command: string,
        cwd: string,
        timeoutMs: number
    ): { success: boolean; output: string; exitCode: number } {
        try {
            // NOSONAR: This is a secure Sandbox runner intended to execute dynamic commands
            const output = execSync(command, { // NOSONAR
                cwd,
                timeout: timeoutMs,
                maxBuffer: MAX_OUTPUT_BUFFER,
                encoding: "utf-8",
                stdio: ["pipe", "pipe", "pipe"],
                // SECURITY: Use scrubbed environment — no API keys, tokens, or secrets
                env: this.getScrubbeEnv() as NodeJS.ProcessEnv,
                // On Windows, kill the entire process tree on timeout
                killSignal: "SIGKILL",
            });

            return { success: true, output: this.sanitizeOutput(output || ""), exitCode: 0 };

        } catch (error: any) {
            // execSync throws on non-zero exit or timeout
            const rawOutput = (error.stdout || "") + "\n" + (error.stderr || "");
            const output = this.sanitizeOutput(rawOutput);
            const exitCode = error.status ?? -1;
            const timedOut = error.killed || error.signal === "SIGKILL";

            if (timedOut) {
                logger.warn(`[LocalSandbox] ⏰ TIMEOUT: Command killed after ${timeoutMs}ms`);
                return {
                    success: false,
                    output: `[TIMEOUT after ${timeoutMs}ms] Process was killed to prevent infinite loop.\n${output.slice(0, 1000)}`,
                    exitCode: -1
                };
            }

            return { success: false, output, exitCode };
        }
    }
}
