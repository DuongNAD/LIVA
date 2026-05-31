import { logger } from "../utils/logger";

/**
 * MeetingCopilot — LIVA Background Daemon
 * =========================================
 * Detects active video meetings (Zoom, Google Meet, Teams, Discord)
 * by monitoring the active window title via the `active-win` package.
 *
 * On meeting start:
 *   - Reduces media volume (DND mode)
 *   - Switches auto-responder context to "meeting"
 *   - Monitors window title for watch keywords
 *
 * On meeting end:
 *   - Restores media volume
 *   - Resets auto-responder context
 *   - Reports meeting duration
 *
 * Check interval: 10 seconds (lightweight — just checks active window).
 * Timer uses .unref() to prevent zombie.
 *
 * @module MeetingCopilot
 */

// ============================================================
// Deps Interface
// ============================================================

export interface MeetingCopilotDeps {
    /** Push notification to UI */
    pushNotification: (title: string, body: string) => void;
    /** Switch auto-responder template (e.g., "meeting", "default") */
    setAutoResponderContext: (context: string) => void;
    /** Reduce system media volume for meeting DND */
    reduceMediaVolume: () => Promise<void>;
    /** Restore system media volume after meeting */
    restoreMediaVolume: () => Promise<void>;
}

// ============================================================
// Constants
// ============================================================

/** Check interval: every 10 seconds */
const CHECK_INTERVAL_MS = 10_000;

/** Meeting is considered "ended" after this many consecutive non-meeting checks */
const END_DETECTION_COUNT = 2;

/** Rate limit: max 1 notification per keyword per 5 minutes */
const KEYWORD_COOLDOWN_MS = 5 * 60 * 1000;

// ============================================================
// Meeting Detection Patterns
// ============================================================

interface MeetingPattern {
    name: string;
    /** Check if the active window matches this meeting app */
    matches: (title: string, ownerName: string) => boolean;
}

const MEETING_PATTERNS: MeetingPattern[] = [
    {
        name: "Zoom",
        matches: (title: string) =>
            title.includes("Zoom Meeting") || title.includes("Zoom Webinar"),
    },
    {
        name: "Google Meet",
        matches: (title: string) =>
            title.includes("Meet -") || title.includes("Google Meet"),
    },
    {
        name: "Microsoft Teams",
        matches: (title: string, ownerName: string) =>
            ownerName.includes("Teams") &&
            (title.includes("Meeting") || title.includes("Call")),
    },
    {
        name: "Discord",
        matches: (title: string, ownerName: string) =>
            ownerName.includes("Discord") &&
            (title.includes("Voice Connected") ||
             title.includes("Screen Share") ||
             /\|.*voice/i.test(title)),
    },
];

// ============================================================
// MeetingCopilot Daemon
// ============================================================

export class MeetingCopilot {
    #deps: MeetingCopilotDeps;
    #intervalRef: ReturnType<typeof setInterval> | null = null;
    #isInMeeting = false;
    #meetingStartTime: number | null = null;
    #meetingAppName: string | null = null;
    #notMeetingCount = 0;
    #watchKeywords: string[];
    #keywordCooldowns: Map<string, number> = new Map();

    constructor(deps: MeetingCopilotDeps, options?: { watchKeywords?: string[] }) {
        this.#deps = deps;
        this.#watchKeywords = options?.watchKeywords ?? [
            "Dương", "deadline", "deploy", "review", "urgent",
        ];
    }

    // ---- Lifecycle ----

    public start(): void {
        if (this.#intervalRef) return;

        this.#intervalRef = setInterval(() => {
            this.#tick().catch(err => {
                const msg = err instanceof Error ? err.message : String(err);
                logger.warn(`[MeetingCopilot] Tick error: ${msg}`);
            });
        }, CHECK_INTERVAL_MS);
        this.#intervalRef.unref(); // Prevent zombie timer

        logger.info("[MeetingCopilot] 📹 Started — monitoring for video meetings (check: 10s).");
    }

    public dispose(): void {
        if (this.#intervalRef) {
            clearInterval(this.#intervalRef);
            this.#intervalRef = null;
        }
        // If in meeting, restore state
        if (this.#isInMeeting) {
            this.#deps.restoreMediaVolume().catch(() => {});
            this.#deps.setAutoResponderContext("default");
        }
        this.#isInMeeting = false;
        this.#meetingStartTime = null;
        this.#meetingAppName = null;
        this.#keywordCooldowns.clear();
        logger.info("[MeetingCopilot] 🛑 Disposed.");
    }

    /**
     * Check if currently in a detected meeting.
     */
    public isInMeeting(): boolean {
        return this.#isInMeeting;
    }

    /**
     * Get current meeting info (for external services).
     */
    public getMeetingInfo(): { isInMeeting: boolean; appName: string | null; durationMinutes: number } {
        return {
            isInMeeting: this.#isInMeeting,
            appName: this.#meetingAppName,
            durationMinutes: this.#meetingStartTime
                ? Math.round((Date.now() - this.#meetingStartTime) / 60_000)
                : 0,
        };
    }

    // ---- Internal Tick ----

    async #tick(): Promise<void> {
        const win = await this.#getActiveWindow();
        if (!win) return; // Could not get active window (non-critical)

        const title = win.title;
        const ownerName = win.ownerName;

        // Check if current window matches any meeting pattern
        const matchedPattern = MEETING_PATTERNS.find(p => p.matches(title, ownerName));

        if (matchedPattern) {
            this.#notMeetingCount = 0; // Reset exit counter

            if (!this.#isInMeeting) {
                // === Meeting started ===
                await this.#onMeetingStart(matchedPattern.name, title);
            } else {
                // === Already in meeting — monitor keywords ===
                this.#monitorKeywords(title);
            }
        } else {
            if (this.#isInMeeting) {
                this.#notMeetingCount++;

                if (this.#notMeetingCount >= END_DETECTION_COUNT) {
                    // === Meeting ended ===
                    await this.#onMeetingEnd();
                }
            }
        }
    }

    // ---- Meeting State Transitions ----

    async #onMeetingStart(appName: string, windowTitle: string): Promise<void> {
        this.#isInMeeting = true;
        this.#meetingStartTime = Date.now();
        this.#meetingAppName = appName;
        this.#notMeetingCount = 0;
        this.#keywordCooldowns.clear();

        logger.info(`[MeetingCopilot] 📹 Phát hiện cuộc họp: ${windowTitle} (${appName})`);

        // 1. Reduce media volume
        try {
            await this.#deps.reduceMediaVolume();
        } catch (err: unknown) {
            const errMsg = err instanceof Error ? err.message : String(err);
            logger.warn(`[MeetingCopilot] Volume reduce failed: ${errMsg}`);
        }

        // 2. Switch auto-responder context
        try {
            this.#deps.setAutoResponderContext("meeting");
        } catch (err: unknown) {
            const errMsg = err instanceof Error ? err.message : String(err);
            logger.warn(`[MeetingCopilot] Auto-responder switch failed: ${errMsg}`);
        }

        // 3. Push notification
        try {
            this.#deps.pushNotification(
                "📹 Cuộc họp đã phát hiện",
                `LIVA đã phát hiện cuộc họp ${appName}, đã giảm nhạc và chuyển chế độ DND.`
            );
        } catch {
            // Non-critical
        }
    }

    async #onMeetingEnd(): Promise<void> {
        const durationMs = this.#meetingStartTime ? Date.now() - this.#meetingStartTime : 0;
        const durationMinutes = Math.round(durationMs / 60_000);
        const appName = this.#meetingAppName ?? "Unknown";

        this.#isInMeeting = false;
        this.#meetingStartTime = null;
        this.#meetingAppName = null;
        this.#notMeetingCount = 0;
        this.#keywordCooldowns.clear();

        logger.info(`[MeetingCopilot] 📹 Cuộc họp kết thúc (${appName}, ${durationMinutes} phút).`);

        // 1. Restore media volume
        try {
            await this.#deps.restoreMediaVolume();
        } catch (err: unknown) {
            const errMsg = err instanceof Error ? err.message : String(err);
            logger.warn(`[MeetingCopilot] Volume restore failed: ${errMsg}`);
        }

        // 2. Reset auto-responder context
        try {
            this.#deps.setAutoResponderContext("default");
        } catch (err: unknown) {
            const errMsg = err instanceof Error ? err.message : String(err);
            logger.warn(`[MeetingCopilot] Auto-responder reset failed: ${errMsg}`);
        }

        // 3. Push summary notification
        try {
            this.#deps.pushNotification(
                "📹 Cuộc họp kết thúc",
                `Cuộc họp ${appName} kết thúc sau ${durationMinutes} phút.`
            );
        } catch {
            // Non-critical
        }
    }

    // ---- Keyword Monitoring ----

    #monitorKeywords(windowTitle: string): void {
        const now = Date.now();
        const titleLower = windowTitle.toLowerCase();

        for (const keyword of this.#watchKeywords) {
            const keywordLower = keyword.toLowerCase();

            if (!titleLower.includes(keywordLower)) continue;

            // Rate limit check
            const lastNotified = this.#keywordCooldowns.get(keywordLower) ?? 0;
            if (now - lastNotified < KEYWORD_COOLDOWN_MS) continue;

            // Send subtle notification
            this.#keywordCooldowns.set(keywordLower, now);
            logger.info(`[MeetingCopilot] 🔔 Keyword detected in meeting: "${keyword}"`);

            try {
                this.#deps.pushNotification(
                    "🔔 Keyword trong cuộc họp",
                    `Phát hiện keyword "${keyword}" trong cuộc họp!`
                );
            } catch {
                // Non-critical
            }
        }
    }

    // ---- Active Window Detection ----

    /**
     * Get the currently active window title and owner name.
     * Uses dynamic import of `active-win` (ESM compatible).
     */
    async #getActiveWindow(): Promise<{ title: string; ownerName: string } | null> {
        try {
            const activeWinModule = await import("active-win");
            const activeWindow = activeWinModule.default;
            const win = await activeWindow();

            if (!win) return null;

            return {
                title: win.title ?? "",
                ownerName: win.owner?.name ?? "",
            };
        } catch (err: unknown) {
            const errMsg = err instanceof Error ? err.message : String(err);
            logger.debug(`[MeetingCopilot] Active window detection failed: ${errMsg}`);
            return null;
        }
    }
}
