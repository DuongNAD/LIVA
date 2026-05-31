import { z } from "zod";
import { logger } from "@utils/logger";
import { exec } from "node:child_process";
import { promisify } from "node:util";
import { HITLGuard } from "@security/HITLGuard";

const execAsync = promisify(exec);

const ProcessSchema = z.object({
  action: z.enum(["list", "kill", "search"]).describe("Hành động: liệt kê, tìm kiếm, hoặc kết thúc tiến trình"),
  name: z.string().optional().describe("Process name cần tìm hoặc kết thúc (VD: 'llama-server', 'chrome')"),
  pid: z.number().optional().describe("Process ID cần kết thúc (dùng khi biết chính xác PID)"),
  sortBy: z.enum(["cpu", "memory", "name"]).optional().default("memory").describe("Tiêu chí sắp xếp khi liệt kê"),
});

export const metadata = {
  name: "process_manager",
  description: "[ASK_FIRST] Windows process manager. List top N processes by CPU/RAM, search by name, and safely kill processes via HITL Guard.",
  kit: "DEVOPS_KIT",
  search_keywords: ["process", "task manager", "kill", "tiến trình", "ram", "cpu"],
  parameters: {
    type: "object",
    properties: {
      action: { type: "string", enum: ["list", "kill", "search"] },
      name: { type: "string", description: "Process name" },
      pid: { type: "number", description: "Process ID" },
      sortBy: { type: "string", enum: ["cpu", "memory", "name"] }
    },
    required: ["action"],
  },
};

export const execute = async (argsObj: unknown): Promise<string> => {
    try {
        const parsed = ProcessSchema.parse(argsObj);

        if (parsed.action === "list") {
            return await listProcesses(parsed.sortBy ?? "memory");
        }

        if (parsed.action === "search") {
            if (!parsed.name) {
                return "[PROCESS ERROR] Cần cung cấp 'name' để tìm kiếm tiến trình.";
            }
            return await searchProcess(parsed.name);
        }

        if (parsed.action === "kill") {
            if (!parsed.pid && !parsed.name) {
                return "[PROCESS ERROR] Cần cung cấp 'pid' hoặc 'name' để kết thúc tiến trình.";
            }
            return await killProcess(parsed.pid, parsed.name);
        }

        return "[PROCESS ERROR] Hành động không hợp lệ.";
    } catch (error: unknown) {
        const msg = error instanceof z.ZodError
            ? `Sai định dạng: ${error.issues.map(e => e.message).join(", ")}`
            : (error instanceof Error ? error.message : "Unknown error");
        logger.error(`[ProcessManager] Lỗi: ${msg}`);
        return `[PROCESS ERROR] ${msg}`;
    }
};

async function listProcesses(sortBy: string): Promise<string> {
    const sortField = sortBy === "cpu" ? "CPU" : sortBy === "name" ? "ProcessName" : "WorkingSet64";
    
    const psScript = `
        Get-Process | Sort-Object -Property ${sortField} -Descending | 
        Select-Object -First 15 ProcessName, Id, 
            @{N='CPU_Sec';E={[math]::Round($_.CPU, 1)}}, 
            @{N='RAM_MB';E={[math]::Round($_.WorkingSet64 / 1MB, 1)}} |
        ConvertTo-Json
    `;

    const { stdout } = await execAsync(
        `powershell.exe -NoProfile -Command "${psScript}"`,
        { timeout: 10000 }
    );

    const processes = JSON.parse(stdout.trim());
    const list = Array.isArray(processes) ? processes : [processes];

    let output = `[PROCESS LIST] Top 15 tiến trình (sắp xếp: ${sortBy}):\n\n`;
    output += `| # | Tên Tiến Trình | PID | CPU (s) | RAM (MB) |\n`;
    output += `|---|----------------|-----|---------|----------|\n`;
    
    for (let i = 0; i < list.length; i++) {
        const p = list[i];
        output += `| ${i + 1} | ${p.ProcessName} | ${p.Id} | ${p.CPU_Sec ?? 0} | ${p.RAM_MB} |\n`;
    }

    logger.info(`[ProcessManager] Đã liệt kê ${list.length} tiến trình.`);
    return output;
}

async function searchProcess(name: string): Promise<string> {
    // Sanitize input to prevent injection
    const safeName = name.replace(/[^a-zA-Z0-9._\-]/g, "");

    const psScript = `
        Get-Process -Name '*${safeName}*' -ErrorAction SilentlyContinue | 
        Select-Object ProcessName, Id, 
            @{N='CPU_Sec';E={[math]::Round($_.CPU, 1)}}, 
            @{N='RAM_MB';E={[math]::Round($_.WorkingSet64 / 1MB, 1)}},
            @{N='StartTime';E={$_.StartTime.ToString('yyyy-MM-dd HH:mm:ss')}} |
        ConvertTo-Json
    `;

    const { stdout } = await execAsync(
        `powershell.exe -NoProfile -Command "${psScript}"`,
        { timeout: 10000 }
    );

    const trimmed = stdout.trim();
    if (!trimmed || trimmed === "") {
        return `[PROCESS SEARCH] Không tìm thấy tiến trình nào khớp với "${name}".`;
    }

    const processes = JSON.parse(trimmed);
    const list = Array.isArray(processes) ? processes : [processes];

    let output = `[PROCESS SEARCH] Tìm thấy ${list.length} tiến trình khớp "${name}":\n\n`;
    for (const p of list) {
        output += `- ${p.ProcessName} (PID: ${p.Id}) | CPU: ${p.CPU_Sec ?? 0}s | RAM: ${p.RAM_MB} MB | Start: ${p.StartTime || "N/A"}\n`;
    }

    logger.info(`[ProcessManager] Tìm thấy ${list.length} tiến trình cho "${name}".`);
    return output;
}

async function killProcess(pid?: number, name?: string): Promise<string> {
    const target = pid ? `PID ${pid}` : `"${name}"`;

    // 🔒 [P1-3.2] Validate PID is a safe integer, sanitize name to prevent shell injection
    if (pid !== undefined && (!Number.isSafeInteger(pid) || pid <= 0)) {
        return `[PROCESS ERROR] PID không hợp lệ: ${pid}. PID phải là số nguyên dương.`;
    }
    const safeName = name ? name.replace(/[^a-zA-Z0-9._\-]/g, "") : undefined;
    if (name && !safeName) {
        return `[PROCESS ERROR] Process name "${name}" chứa toàn ký tự không hợp lệ.`;
    }

    // HITL Guard: Kill process is destructive
    logger.info(`[ProcessManager] Yêu cầu kill ${target}. Chờ HITL phê duyệt...`);
    try {
        await HITLGuard.requestApproval({
            toolName: "process_manager",
            args: { action: "kill", pid, name },
            reason: `LIVA muốn kết thúc tiến trình ${target} trên máy tính.`
        });
    } catch (error: unknown) {
        const msg = error instanceof Error ? error.message : "Unknown";
        return `[PROCESS BLOCKED] Không được phép kết thúc tiến trình ${target}: ${msg}`;
    }

    const command = pid
        ? `Stop-Process -Id ${pid} -Force -ErrorAction Stop`
        : `Stop-Process -Name '${safeName}' -Force -ErrorAction Stop`;

    await execAsync(`powershell.exe -NoProfile -Command "${command}"`, { timeout: 10000 });

    logger.info(`[ProcessManager] ✅ Đã kết thúc tiến trình ${target}.`);
    return `[PROCESS KILLED] Đã kết thúc thành công tiến trình ${target}.`;
}
