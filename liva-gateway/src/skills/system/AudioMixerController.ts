import { z } from "zod";
import { logger } from "@utils/logger";
import { exec } from "node:child_process";
import { promisify } from "node:util";

const execAsync = promisify(exec);

// ─── Zod Schema ────────────────────────────────────────────────────────────────
const AudioMixerSchema = z.object({
    action: z.enum(["duck", "restore", "set_volume", "get_volumes"])
        .describe("Hành động điều khiển âm lượng ứng dụng"),
    appName: z.string().optional()
        .describe("Tên ứng dụng (VD: Spotify, chrome, firefox)"),
    volumePercent: z.number().min(0).max(100).optional()
        .describe("Mức âm lượng mong muốn (0-100)"),
});

// ─── Metadata ──────────────────────────────────────────────────────────────────
export const metadata = {
    name: "audio_mixer_controller",
    description: "[AUTO_RUN] Audio Ducking controller. Reduce/restore specific app volumes (Spotify, Chrome, etc.) for TTS playback without audio collision.",
    kit: "PERSONAL_KIT",
    search_keywords: ["audio", "volume", "duck", "mixer", "spotify", "sound", "âm lượng", "giảm âm"],
    parameters: {
        type: "object",
        properties: {
            action: { type: "string", enum: ["duck", "restore", "set_volume", "get_volumes"] },
            appName: { type: "string", description: "App name e.g. Spotify, chrome" },
            volumePercent: { type: "number", description: "Volume level 0-100" },
        },
        required: ["action"],
    },
};

// ─── Module-level State ────────────────────────────────────────────────────────
/** Lưu âm lượng gốc trước khi duck để restore */
const savedVolumes: Map<string, number> = new Map();

/** Danh sách ứng dụng media mặc định cần duck */
const DEFAULT_MEDIA_APPS = ["Spotify", "chrome", "firefox", "msedge", "vlc", "wmplayer", "foobar2000", "Music"];

// Khoảng duck mặc định: giảm về 20%
const DUCK_VOLUME_PERCENT = 20;

// ─── PowerShell Helpers ────────────────────────────────────────────────────────

/**
 * Lấy danh sách audio sessions đang active qua PowerShell COM.
 * Trả về mảng { ProcessName, Volume } với Volume 0.0-1.0.
 */
const PS_GET_AUDIO_SESSIONS = `
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

[Guid("87CE5498-68D6-44E5-9215-6DA47EF883D8"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface ISimpleAudioVolume {
    int SetMasterVolume(float fLevel, ref Guid EventContext);
    int GetMasterVolume(out float pfLevel);
    int SetMute(bool bMute, ref Guid EventContext);
    int GetMute(out bool pbMute);
}

[Guid("24918ACC-64B3-37C1-8CA9-74A66E9957A8"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IAudioSessionEnumerator {
    int GetCount(out int SessionCount);
    int GetSession(int SessionCount, out IAudioSessionControl2 Session);
}

[Guid("bfb7ff88-7239-4fc9-8fa2-07c950be9c6d"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IAudioSessionControl2 {
    // IAudioSessionControl
    int _cycl0(); int _cycl1(); int _cycl2(); int _cycl3(); int _cycl4(); int _cycl5(); int _cycl6(); int _cycl7();
    // IAudioSessionControl2
    int GetSessionIdentifier([MarshalAs(UnmanagedType.LPWStr)] out string pRetVal);
    int GetSessionInstanceIdentifier([MarshalAs(UnmanagedType.LPWStr)] out string pRetVal);
    int GetProcessId(out uint pRetVal);
    int IsSystemSoundsSession();
    int SetDuckingPreference(bool optOut);
}

[Guid("77AA99A0-1BD6-484F-8BC7-2C654C9A9B6F"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IAudioSessionManager2 {
    int _cycl0(); int _cycl1();
    int GetSessionEnumerator(out IAudioSessionEnumerator SessionEnum);
    // ...
}

[Guid("BCDE0395-E52F-467C-8E3D-C4579291692E")] class MMDeviceEnumeratorCls { }

[Guid("A95664D2-9614-4F35-A746-DE8DB63617E6"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IMMDeviceEnumerator {
    int _cycl0();
    int GetDefaultAudioEndpoint(int dataFlow, int role, out IMMDevice ppDevice);
}

[Guid("D666063F-1587-4E43-81F1-B948E807363F"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IMMDevice {
    int Activate(ref Guid iid, int dwClsCtx, IntPtr pActivationParams, [MarshalAs(UnmanagedType.IUnknown)] out object ppInterface);
}

public class AudioMixer {
    public static string GetSessions() {
        var result = new System.Text.StringBuilder();
        try {
            var enumerator = (IMMDeviceEnumerator)(new MMDeviceEnumeratorCls());
            IMMDevice device;
            enumerator.GetDefaultAudioEndpoint(0, 1, out device);
            Guid iidSM = typeof(IAudioSessionManager2).GUID;
            object o;
            device.Activate(ref iidSM, 1, IntPtr.Zero, out o);
            var mgr = (IAudioSessionManager2)o;
            IAudioSessionEnumerator sessionEnum;
            mgr.GetSessionEnumerator(out sessionEnum);
            int count;
            sessionEnum.GetCount(out count);
            for (int i = 0; i < count; i++) {
                IAudioSessionControl2 ctrl;
                sessionEnum.GetSession(i, out ctrl);
                uint pid;
                ctrl.GetProcessId(out pid);
                if (pid == 0) continue;
                var vol = (ISimpleAudioVolume)ctrl;
                float level;
                vol.GetMasterVolume(out level);
                try {
                    var proc = System.Diagnostics.Process.GetProcessById((int)pid);
                    result.AppendLine(proc.ProcessName + "|" + level.ToString("F2"));
                } catch { }
            }
        } catch (Exception ex) {
            result.AppendLine("ERROR|" + ex.Message);
        }
        return result.ToString();
    }

    public static string SetVolume(string processName, float level) {
        try {
            var enumerator = (IMMDeviceEnumerator)(new MMDeviceEnumeratorCls());
            IMMDevice device;
            enumerator.GetDefaultAudioEndpoint(0, 1, out device);
            Guid iidSM = typeof(IAudioSessionManager2).GUID;
            object o;
            device.Activate(ref iidSM, 1, IntPtr.Zero, out o);
            var mgr = (IAudioSessionManager2)o;
            IAudioSessionEnumerator sessionEnum;
            mgr.GetSessionEnumerator(out sessionEnum);
            int count;
            sessionEnum.GetCount(out count);
            bool found = false;
            var ctx = Guid.Empty;
            for (int i = 0; i < count; i++) {
                IAudioSessionControl2 ctrl;
                sessionEnum.GetSession(i, out ctrl);
                uint pid;
                ctrl.GetProcessId(out pid);
                if (pid == 0) continue;
                try {
                    var proc = System.Diagnostics.Process.GetProcessById((int)pid);
                    if (proc.ProcessName.IndexOf(processName, StringComparison.OrdinalIgnoreCase) >= 0) {
                        var vol = (ISimpleAudioVolume)ctrl;
                        vol.SetMasterVolume(level, ref ctx);
                        found = true;
                    }
                } catch { }
            }
            return found ? "OK" : "NOT_FOUND";
        } catch (Exception ex) {
            return "ERROR|" + ex.Message;
        }
    }
}
'@ -ErrorAction SilentlyContinue
`;

/**
 * Lấy tất cả audio sessions kèm tên process và volume (0.0 - 1.0).
 */
async function getAudioSessions(): Promise<Array<{ name: string; volume: number }>> {
    const script = `${PS_GET_AUDIO_SESSIONS}\n[AudioMixer]::GetSessions()`;
    try {
        const { stdout } = await execAsync(`powershell.exe -NoProfile -Command "${script.replace(/"/g, '\\"').replace(/\n/g, '; ')}"`, {
            timeout: 10_000,
        });
        const lines = stdout.trim().split(/\r?\n/).filter(Boolean);
        const sessions: Array<{ name: string; volume: number }> = [];
        for (const line of lines) {
            const [name, volStr] = line.split("|");
            if (name === "ERROR") {
                logger.warn(`[AudioMixer] COM error: ${volStr}`);
                continue;
            }
            if (name && volStr) {
                sessions.push({ name: name.trim(), volume: parseFloat(volStr) });
            }
        }
        return sessions;
    } catch (err: unknown) {
        const errMsg = err instanceof Error ? err.message : String(err);
        logger.warn(`[AudioMixer] Không thể lấy audio sessions qua COM: ${errMsg}`);
        return [];
    }
}

/**
 * Set volume cho 1 app cụ thể qua PowerShell COM.
 * @param processName Tên process (VD: Spotify, chrome)
 * @param volumeFloat Volume 0.0 - 1.0
 */
async function setAppVolume(processName: string, volumeFloat: number): Promise<boolean> {
    const script = `${PS_GET_AUDIO_SESSIONS}\n[AudioMixer]::SetVolume('${processName}', ${volumeFloat.toFixed(2)})`;
    try {
        const { stdout } = await execAsync(`powershell.exe -NoProfile -Command "${script.replace(/"/g, '\\"').replace(/\n/g, '; ')}"`, {
            timeout: 10_000,
        });
        const result = stdout.trim();
        if (result === "OK") return true;
        if (result === "NOT_FOUND") {
            logger.warn(`[AudioMixer] Process '${processName}' không có audio session`);
            return false;
        }
        logger.warn(`[AudioMixer] SetVolume error: ${result}`);
        return false;
    } catch (err: unknown) {
        const errMsg = err instanceof Error ? err.message : String(err);
        logger.warn(`[AudioMixer] Lỗi set volume cho ${processName}: ${errMsg}`);
        return false;
    }
}

/**
 * Fallback: set master system volume qua WScript.Shell SendKeys (giống HardwareController).
 */
async function setMasterVolumeFallback(percent: number): Promise<void> {
    const upSteps = Math.round(percent / 2);
    const psScript = `
        $obj = new-object -com wscript.shell
        for ($i = 0; $i -lt 50; $i++) { $obj.SendKeys([char]174) }
        for ($i = 0; $i -lt ${upSteps}; $i++) { $obj.SendKeys([char]175) }
    `.replace(/\n/g, ";");
    await execAsync(`powershell.exe -NoProfile -Command "${psScript}"`);
}

// ─── Execute ───────────────────────────────────────────────────────────────────
export const execute = async (argsObj: any): Promise<string> => {
    try {
        const parsed = AudioMixerSchema.parse(argsObj);

        switch (parsed.action) {
            // ── DUCK: lưu volume hiện tại của media apps rồi giảm về 20% ──
            case "duck": {
                const sessions = await getAudioSessions();
                if (sessions.length === 0) {
                    // Fallback: duck master volume
                    logger.info("[AudioMixer] Không tìm thấy audio sessions, fallback duck master volume");
                    savedVolumes.set("__master__", 100);
                    await setMasterVolumeFallback(DUCK_VOLUME_PERCENT);
                    return `[AUDIOMIXER SUCCESS] Đã duck master volume về ${DUCK_VOLUME_PERCENT}%.`;
                }

                let duckedCount = 0;
                for (const session of sessions) {
                    const isMediaApp = DEFAULT_MEDIA_APPS.some(
                        app => session.name.toLowerCase().includes(app.toLowerCase())
                    );
                    if (!isMediaApp) continue;

                    // Lưu volume gốc (đã là float 0.0-1.0, chuyển sang %)
                    const originalPercent = Math.round(session.volume * 100);
                    savedVolumes.set(session.name.toLowerCase(), originalPercent);

                    // Giảm về DUCK_VOLUME_PERCENT
                    const duckFloat = DUCK_VOLUME_PERCENT / 100;
                    const ok = await setAppVolume(session.name, duckFloat);
                    if (ok) duckedCount++;
                }

                if (duckedCount === 0) {
                    logger.info("[AudioMixer] Không có media app nào đang phát, duck master volume");
                    savedVolumes.set("__master__", 100);
                    await setMasterVolumeFallback(DUCK_VOLUME_PERCENT);
                    return `[AUDIOMIXER SUCCESS] Không có media app nào active, đã duck master volume về ${DUCK_VOLUME_PERCENT}%.`;
                }

                logger.info(`[AudioMixer] Đã duck ${duckedCount} ứng dụng media về ${DUCK_VOLUME_PERCENT}%`);
                return `[AUDIOMIXER SUCCESS] Đã giảm âm lượng ${duckedCount} ứng dụng media về ${DUCK_VOLUME_PERCENT}% để phát TTS.`;
            }

            // ── RESTORE: khôi phục volume đã lưu ──
            case "restore": {
                if (savedVolumes.size === 0) {
                    return `[AUDIOMIXER SUCCESS] Không có gì để khôi phục — chưa duck trước đó.`;
                }

                let restoredCount = 0;

                // Xử lý master fallback trước
                if (savedVolumes.has("__master__")) {
                    const original = savedVolumes.get("__master__")!;
                    await setMasterVolumeFallback(original);
                    savedVolumes.delete("__master__");
                    restoredCount++;
                }

                // Restore từng app
                for (const [appNameKey, originalPercent] of savedVolumes) {
                    const volumeFloat = originalPercent / 100;
                    const ok = await setAppVolume(appNameKey, volumeFloat);
                    if (ok) restoredCount++;
                }

                savedVolumes.clear();
                logger.info(`[AudioMixer] Đã restore ${restoredCount} nguồn âm thanh`);
                return `[AUDIOMIXER SUCCESS] Đã khôi phục âm lượng ${restoredCount} nguồn âm thanh về mức gốc.`;
            }

            // ── SET_VOLUME: set volume cho 1 app cụ thể ──
            case "set_volume": {
                if (!parsed.appName) {
                    return `[AUDIOMIXER ERROR] Thiếu tham số 'appName' cho hành động set_volume.`;
                }
                if (parsed.volumePercent === undefined) {
                    return `[AUDIOMIXER ERROR] Thiếu tham số 'volumePercent' cho hành động set_volume.`;
                }

                const volumeFloat = parsed.volumePercent / 100;
                const ok = await setAppVolume(parsed.appName, volumeFloat);

                if (ok) {
                    logger.info(`[AudioMixer] Đã set volume ${parsed.appName} = ${parsed.volumePercent}%`);
                    return `[AUDIOMIXER SUCCESS] Đã đặt âm lượng ${parsed.appName} thành ${parsed.volumePercent}%.`;
                }

                // Fallback: nếu không tìm thấy app, set master
                logger.warn(`[AudioMixer] App '${parsed.appName}' không có audio session, fallback set master volume`);
                await setMasterVolumeFallback(parsed.volumePercent);
                return `[AUDIOMIXER SUCCESS] Không tìm thấy ${parsed.appName}, đã set master volume thành ${parsed.volumePercent}%.`;
            }

            // ── GET_VOLUMES: liệt kê tất cả audio sessions ──
            case "get_volumes": {
                const sessions = await getAudioSessions();
                if (sessions.length === 0) {
                    return `[AUDIOMIXER SUCCESS] Không phát hiện audio session nào đang hoạt động.`;
                }

                const listing = sessions
                    .map(s => `  • ${s.name}: ${Math.round(s.volume * 100)}%`)
                    .join("\n");

                logger.info(`[AudioMixer] Liệt kê ${sessions.length} audio sessions`);
                return `[AUDIOMIXER SUCCESS] Các audio session đang hoạt động:\n${listing}`;
            }

            default:
                return `[AUDIOMIXER ERROR] Hành động không hợp lệ.`;
        }
    } catch (error: unknown) {
        const errMsg = error instanceof Error ? error.message : String(error);
        logger.error(`[AudioMixer] Lỗi: ${errMsg}`);
        if (error instanceof z.ZodError) {
            return `[AUDIOMIXER ERROR] Sai định dạng: ${error.issues.map(e => e.message).join(", ")}`;
        }
        return `[AUDIOMIXER ERROR] Lỗi hệ thống: ${errMsg}`;
    }
};
