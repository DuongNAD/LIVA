import { z } from "zod";
import { logger } from "@utils/logger";
import { HITLGuard } from "@security/HITLGuard";
import { exec } from "node:child_process";
import { promisify } from "node:util";

const execAsync = promisify(exec);

// ── Zod Schema ──────────────────────────────────────────────────────────────
const FocusWardenSchema = z.object({
    action: z.enum(["start", "stop", "status"]).describe("Hành động: bắt đầu, dừng, hoặc xem trạng thái Deep Work"),
    durationMinutes: z.number().min(10).max(480).optional().default(120)
        .describe("Thời gian Deep Work (phút), mặc định 120"),
    blockSites: z.array(z.string()).optional().default([
        "facebook.com", "youtube.com", "tiktok.com",
        "twitter.com", "reddit.com", "instagram.com",
    ]).describe("Danh sách tên miền cần chặn"),
    killGames: z.boolean().optional().default(true)
        .describe("Tự động kill tiến trình game"),
    playLofi: z.boolean().optional().default(true)
        .describe("Bật nhạc lofi khi bắt đầu"),
});

// ── Metadata ────────────────────────────────────────────────────────────────
export const metadata = {
    name: "focus_warden",
    description: "[ASK_FIRST] Deep Work mode. Blocks distracting websites (Facebook, YouTube, TikTok), kills game processes, enables DND auto-reply, and plays lofi music. Requires HITL approval for hosts file modification.",
    kit: "PERSONAL_KIT",
    requires_hitl: true,
    search_keywords: ["focus", "deep work", "block", "distraction", "pomodoro", "dnd", "tập trung", "chặn web"],
    parameters: {
        type: "object",
        properties: {
            action: { type: "string", enum: ["start", "stop", "status"] },
            durationMinutes: { type: "number", description: "Duration in minutes (10-480, default 120)" },
            blockSites: {
                type: "array",
                items: { type: "string" },
                description: "List of domains to block",
            },
            killGames: { type: "boolean", description: "Auto-kill game processes (default true)" },
            playLofi: { type: "boolean", description: "Play lofi music when starting (default true)" },
        },
        required: ["action"],
    },
};

// ── Hosts file path ─────────────────────────────────────────────────────────
const HOSTS_PATH = "C:\\Windows\\System32\\drivers\\etc\\hosts";
const FOCUS_MARKER_BEGIN = "# === LIVA FOCUS WARDEN BEGIN ===";
const FOCUS_MARKER_END = "# === LIVA FOCUS WARDEN END ===";

// ── Game detection patterns ─────────────────────────────────────────────────
const GAME_PROCESS_PATTERN = "\\\\Games\\\\|\\\\Steam\\\\steamapps|\\\\Riot Games\\\\|valorant|leagueoflegends|minecraft|epicgameslauncher|fortniteclient|genshinimpact|PUBG|Overwatch|csgo|cs2|dota2";

// ── FocusSession Singleton ──────────────────────────────────────────────────
class FocusSession {
    #isActive = false;
    #endTime: number | null = null;
    #blockedSites: string[] = [];
    #hostsBackup: string = "";
    #timerRef: NodeJS.Timeout | null = null;
    #gameKillerRef: NodeJS.Timeout | null = null;
    #startTime: number | null = null;

    get isActive(): boolean {
        return this.#isActive;
    }

    get remainingMs(): number {
        if (!this.#endTime) return 0;
        return Math.max(0, this.#endTime - Date.now());
    }

    get blockedSites(): string[] {
        return [...this.#blockedSites];
    }

    get startTime(): number | null {
        return this.#startTime;
    }

    /** Bắt đầu phiên Deep Work */
    async start(durationMinutes: number, blockSites: string[], killGames: boolean, playLofi: boolean): Promise<string> {
        if (this.#isActive) {
            const remaining = Math.ceil(this.remainingMs / 60000);
            return `[FOCUS INFO] Deep Work đang hoạt động. Còn lại ${remaining} phút. Dùng action "stop" để dừng sớm.`;
        }

        // ── HITL Approval ───────────────────────────────────────────────
        try {
            await HITLGuard.requestApproval({
                toolName: "focus_warden",
                args: { action: "start", durationMinutes, blockSites, killGames },
                reason: `LIVA muốn bật chế độ Deep Work ${durationMinutes} phút: chặn ${blockSites.length} website, ${killGames ? "kill game" : "không kill game"}, bật DND.`,
            });
        } catch (err: unknown) {
            const msg = err instanceof Error ? err.message : String(err);
            return `[FOCUS BLOCKED] Người dùng từ chối bật Deep Work: ${msg}`;
        }

        // ── Backup hosts file ───────────────────────────────────────────
        try {
            const { stdout } = await execAsync(
                `powershell.exe -NoProfile -Command "Get-Content '${HOSTS_PATH}' -Raw -ErrorAction SilentlyContinue"`,
                { timeout: 10000 },
            );
            this.#hostsBackup = stdout;
        } catch {
            this.#hostsBackup = "";
            logger.warn("[FocusWarden] Không đọc được hosts file — tiếp tục với backup rỗng.");
        }

        // ── Expand sites (add www. variants) ────────────────────────────
        const expandedSites: string[] = [];
        for (const site of blockSites) {
            expandedSites.push(site);
            if (!site.startsWith("www.")) {
                expandedSites.push(`www.${site}`);
            }
        }
        this.#blockedSites = blockSites;

        // ── Add block entries to hosts file (elevated) ──────────────────
        try {
            const entries = expandedSites.map((s) => `127.0.0.1 ${s}`).join("`n");
            const blockContent = `${FOCUS_MARKER_BEGIN}\`n${entries}\`n${FOCUS_MARKER_END}`;
            const innerCmd = `Add-Content -Path '${HOSTS_PATH}' -Value \\"${blockContent}\\" -Encoding ASCII`;
            const elevatedCmd = `powershell.exe -NoProfile -Command "Start-Process powershell -ArgumentList '-NoProfile -Command ${innerCmd}' -Verb RunAs -Wait"`;
            await execAsync(elevatedCmd, { timeout: 30000 });
            logger.info(`[FocusWarden] Đã chặn ${expandedSites.length} domain trong hosts file.`);
        } catch (err: unknown) {
            const msg = err instanceof Error ? err.message : String(err);
            logger.error(`[FocusWarden] Lỗi ghi hosts file: ${msg}`);
            return `[FOCUS ERROR] Không thể chặn website (cần quyền Admin): ${msg}`;
        }

        // ── Flush DNS cache ─────────────────────────────────────────────
        try {
            await execAsync("powershell.exe -NoProfile -Command \"ipconfig /flushdns\"", { timeout: 10000 });
        } catch {
            logger.warn("[FocusWarden] Không flush được DNS cache — bỏ qua.");
        }

        // ── Game Killer interval (30s) ──────────────────────────────────
        if (killGames) {
            this.#gameKillerRef = setInterval(async () => {
                try {
                    const psCmd = `Get-Process | Where-Object { $_.Path -match '${GAME_PROCESS_PATTERN}' } | Stop-Process -Force -ErrorAction SilentlyContinue`;
                    await execAsync(`powershell.exe -NoProfile -Command "${psCmd}"`, { timeout: 10000 });
                } catch {
                    // Không có game nào đang chạy — bỏ qua
                }
            }, 30_000);
            this.#gameKillerRef.unref();

            // Kill game ngay lập tức lần đầu
            try {
                const psCmd = `Get-Process | Where-Object { $_.Path -match '${GAME_PROCESS_PATTERN}' } | Stop-Process -Force -ErrorAction SilentlyContinue`;
                await execAsync(`powershell.exe -NoProfile -Command "${psCmd}"`, { timeout: 10000 });
            } catch {
                // Không có game
            }
        }

        // ── DND mode via globalThis ─────────────────────────────────────
        try {
            const kernel = (globalThis as any).kernelInstance;
            if (kernel?.autoResponder) {
                kernel.autoResponder.enableDND?.();
                logger.info("[FocusWarden] Đã bật chế độ DND auto-reply.");
            }
        } catch {
            logger.warn("[FocusWarden] Không bật được DND auto-reply — bỏ qua.");
        }

        // ── Play lofi music ─────────────────────────────────────────────
        if (playLofi) {
            try {
                // Mô phỏng phím Play/Pause để bật nhạc (nếu player đang sẵn sàng)
                await execAsync(
                    `powershell.exe -NoProfile -Command "(New-Object -ComObject WScript.Shell).SendKeys([char]0xB3)"`,
                    { timeout: 5000 },
                );
                logger.info("[FocusWarden] Đã gửi lệnh Play/Pause media.");
            } catch {
                logger.warn("[FocusWarden] Không gửi được lệnh media — bỏ qua.");
            }
        }

        // ── Push notification to UI ─────────────────────────────────────
        const toastPayload = JSON.stringify({
            event: "SHOW_TOAST",
            payload: {
                title: "🎯 Deep Work ON",
                message: `Tập trung ${durationMinutes} phút. Đã chặn ${blockSites.length} website.`,
                type: "success",
                duration: 5000,
            },
        });
        process.stdout.write(toastPayload + "\n");

        // ── Set state ───────────────────────────────────────────────────
        this.#isActive = true;
        this.#startTime = Date.now();
        this.#endTime = Date.now() + durationMinutes * 60 * 1000;

        // ── Auto-stop timer ─────────────────────────────────────────────
        this.#timerRef = setTimeout(async () => {
            logger.info("[FocusWarden] ⏰ Deep Work session hết hạn. Tự động dừng...");
            await this.stop();
            const endToast = JSON.stringify({
                event: "SHOW_TOAST",
                payload: {
                    title: "✅ Deep Work kết thúc!",
                    message: `Phiên Deep Work ${durationMinutes} phút đã hoàn tất. Nghỉ ngơi nhé!`,
                    type: "info",
                    duration: 8000,
                },
            });
            process.stdout.write(endToast + "\n");
        }, durationMinutes * 60 * 1000);
        this.#timerRef.unref();

        logger.info(`[FocusWarden] ✅ Deep Work bắt đầu: ${durationMinutes} phút, chặn ${blockSites.length} site.`);
        return `[FOCUS SUCCESS] 🎯 Deep Work đã bật!\n- Thời gian: ${durationMinutes} phút\n- Website bị chặn: ${blockSites.join(", ")}\n- Kill game: ${killGames ? "Có" : "Không"}\n- Lofi music: ${playLofi ? "Đã bật" : "Không"}\n- Kết thúc lúc: ${new Date(this.#endTime).toLocaleTimeString("vi-VN")}`;
    }

    /** Dừng phiên Deep Work */
    async stop(): Promise<string> {
        if (!this.#isActive) {
            return "[FOCUS INFO] Không có phiên Deep Work nào đang hoạt động.";
        }

        // ── Restore hosts file ──────────────────────────────────────────
        try {
            // Xóa block entries bằng cách loại bỏ phần giữa các marker
            const psRemove = `
                $hostsPath = '${HOSTS_PATH}'
                $content = Get-Content $hostsPath -Raw -ErrorAction SilentlyContinue
                if ($content -match '${FOCUS_MARKER_BEGIN}') {
                    $cleaned = $content -replace '(?s)${FOCUS_MARKER_BEGIN.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}.*?${FOCUS_MARKER_END.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\r?\\n?', ''
                    Set-Content -Path $hostsPath -Value $cleaned.TrimEnd() -Encoding ASCII
                }
            `.trim().replace(/\n\s*/g, " ");
            const elevatedCmd = `powershell.exe -NoProfile -Command "Start-Process powershell -ArgumentList '-NoProfile -Command ${psRemove}' -Verb RunAs -Wait"`;
            await execAsync(elevatedCmd, { timeout: 30000 });
            logger.info("[FocusWarden] Đã phục hồi hosts file.");
        } catch (err: unknown) {
            const msg = err instanceof Error ? err.message : String(err);
            logger.error(`[FocusWarden] Lỗi phục hồi hosts file: ${msg}`);
        }

        // ── Flush DNS ───────────────────────────────────────────────────
        try {
            await execAsync("powershell.exe -NoProfile -Command \"ipconfig /flushdns\"", { timeout: 10000 });
        } catch {
            // Bỏ qua
        }

        // ── Clear game killer ───────────────────────────────────────────
        if (this.#gameKillerRef) {
            clearInterval(this.#gameKillerRef);
            this.#gameKillerRef = null;
        }

        // ── Clear auto-stop timer ───────────────────────────────────────
        if (this.#timerRef) {
            clearTimeout(this.#timerRef);
            this.#timerRef = null;
        }

        // ── Restore DND ─────────────────────────────────────────────────
        try {
            const kernel = (globalThis as any).kernelInstance;
            if (kernel?.autoResponder) {
                kernel.autoResponder.disableDND?.();
                logger.info("[FocusWarden] Đã tắt chế độ DND.");
            }
        } catch {
            logger.warn("[FocusWarden] Không tắt được DND — bỏ qua.");
        }

        // ── Calculate session duration ──────────────────────────────────
        const sessionDuration = this.#startTime
            ? Math.round((Date.now() - this.#startTime) / 60000)
            : 0;

        // ── Reset state ─────────────────────────────────────────────────
        this.#isActive = false;
        this.#endTime = null;
        this.#blockedSites = [];
        this.#hostsBackup = "";
        this.#startTime = null;

        logger.info(`[FocusWarden] ✅ Deep Work đã dừng sau ${sessionDuration} phút.`);
        return `[FOCUS SUCCESS] Deep Work đã tắt.\n- Thời gian tập trung: ${sessionDuration} phút\n- Website đã được bỏ chặn\n- Game killer đã tắt\n- DND đã tắt`;
    }

    /** Trạng thái hiện tại */
    status(): string {
        if (!this.#isActive) {
            return "[FOCUS STATUS] Không có phiên Deep Work nào đang hoạt động.";
        }

        const remainingMin = Math.ceil(this.remainingMs / 60000);
        const elapsedMin = this.#startTime
            ? Math.round((Date.now() - this.#startTime) / 60000)
            : 0;

        return `[FOCUS STATUS] 🎯 Deep Work đang hoạt động\n- Đã tập trung: ${elapsedMin} phút\n- Còn lại: ${remainingMin} phút\n- Website bị chặn: ${this.#blockedSites.join(", ")}\n- Game killer: ${this.#gameKillerRef ? "Đang chạy" : "Tắt"}\n- Kết thúc lúc: ${this.#endTime ? new Date(this.#endTime).toLocaleTimeString("vi-VN") : "N/A"}`;
    }
}

// ── Singleton instance ──────────────────────────────────────────────────────
const focusSession = new FocusSession();

// ── Execute function ────────────────────────────────────────────────────────
export const execute = async (argsObj: unknown): Promise<string> => {
    try {
        const parsed = FocusWardenSchema.parse(argsObj);

        switch (parsed.action) {
            case "start":
                return await focusSession.start(
                    parsed.durationMinutes,
                    parsed.blockSites,
                    parsed.killGames,
                    parsed.playLofi,
                );
            case "stop":
                return await focusSession.stop();
            case "status":
                return focusSession.status();
            default:
                return "[FOCUS ERROR] Hành động không hợp lệ.";
        }
    } catch (error: unknown) {
        const errMsg = error instanceof Error ? error.message : String(error);
        logger.error(`[FocusWarden] Lỗi: ${errMsg}`);
        if (error instanceof z.ZodError) {
            return `[FOCUS ERROR] Sai định dạng: ${error.issues.map((e) => e.message).join(", ")}`;
        }
        return `[FOCUS ERROR] Lỗi hệ thống: ${errMsg}`;
    }
};
