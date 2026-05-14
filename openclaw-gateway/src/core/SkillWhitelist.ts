/**
 * SkillWhitelist — Persistent Skill Enablement Registry
 * ======================================================
 * Stores which skills are "verified" / enabled by the user.
 * Persisted to `data/skill_whitelist.json` using Atomic Write.
 *
 * Default behavior: ALL skills are ENABLED unless explicitly disabled.
 * This avoids breaking existing workflows while giving the user
 * granular control via the Dashboard.
 *
 * Architecture Decision:
 *   - File-based persistence (same pattern as liva-config.json)
 *   - Atomic Write (.tmp + rename) to prevent corruption
 *   - In-memory Map for O(1) lookups during SkillRegistry filtering
 *
 * @module SkillWhitelist
 */

import { promises as fsp } from "node:fs";
import * as path from "node:path";
import { logger } from "../utils/logger";

interface WhitelistEntry {
    enabled: boolean;
    /** Timestamp when the user last toggled this skill */
    lastToggled: number;
    /** Optional note from user about why disabled */
    note?: string;
}

export class SkillWhitelist {
    #entries: Map<string, WhitelistEntry> = new Map();
    #filePath: string;
    #saveTimer: NodeJS.Timeout | null = null;

    constructor() {
        this.#filePath = path.join(process.cwd(), "..", "data", "skill_whitelist.json");
    }

    /**
     * Load whitelist from disk. If file doesn't exist, starts with empty map
     * (all skills enabled by default).
     */
    async load(): Promise<void> {
        try {
            const raw = await fsp.readFile(this.#filePath, "utf-8");
            const data = JSON.parse(raw) as Record<string, WhitelistEntry>;
            this.#entries.clear();
            for (const [name, entry] of Object.entries(data)) {
                this.#entries.set(name, entry);
            }
            logger.info(`[SkillWhitelist] Loaded ${this.#entries.size} skill states from disk.`);
        } catch (e: unknown) {
            const isENOENT = e instanceof Error && "code" in e && (e as NodeJS.ErrnoException).code === "ENOENT";
            if (!isENOENT) {
                const errMsg = e instanceof Error ? e.message : String(e);
                logger.warn(`[SkillWhitelist] Failed to load: ${errMsg}. Starting with defaults (all enabled).`);
            }
            // No file = fresh install = all skills enabled
        }
    }

    /**
     * Check if a skill is enabled.
     * Default: true (enabled) if no explicit entry exists.
     */
    isEnabled(skillName: string): boolean {
        const entry = this.#entries.get(skillName);
        if (!entry) return true; // Default: enabled
        return entry.enabled;
    }

    /**
     * Set a skill's enabled state. Debounced save to disk.
     */
    setEnabled(skillName: string, enabled: boolean, note?: string): void {
        this.#entries.set(skillName, {
            enabled,
            lastToggled: Date.now(),
            note,
        });
        this.#debouncedSave();
        logger.info(`[SkillWhitelist] ${skillName}: ${enabled ? "✅ ENABLED" : "❌ DISABLED"}${note ? ` (${note})` : ""}`);
    }

    /**
     * Bulk enable/disable skills.
     */
    bulkSet(skills: Array<{ name: string; enabled: boolean }>): void {
        for (const { name, enabled } of skills) {
            this.#entries.set(name, {
                enabled,
                lastToggled: Date.now(),
            });
        }
        this.#debouncedSave();
    }

    /**
     * Get all entries for dashboard display.
     */
    getAll(): Record<string, WhitelistEntry> {
        const result: Record<string, WhitelistEntry> = {};
        for (const [name, entry] of this.#entries) {
            result[name] = entry;
        }
        return result;
    }

    /**
     * Get names of all explicitly disabled skills.
     */
    getDisabledSkills(): Set<string> {
        const disabled = new Set<string>();
        for (const [name, entry] of this.#entries) {
            if (!entry.enabled) disabled.add(name);
        }
        return disabled;
    }

    /**
     * Debounced save — prevents rapid successive writes when
     * user toggles multiple skills quickly in the dashboard.
     */
    #debouncedSave(): void {
        if (this.#saveTimer) clearTimeout(this.#saveTimer);
        this.#saveTimer = setTimeout(() => {
            this.#saveTimer = null;
            this.#save().catch(e => {
                const errMsg = e instanceof Error ? e.message : String(e);
                logger.error(`[SkillWhitelist] Save failed: ${errMsg}`);
            });
        }, 500);
    }

    /**
     * Atomic Write: .tmp + rename (per AI_CONTEXT.md Rule 4.3)
     */
    async #save(): Promise<void> {
        const data = this.getAll();
        const tmpPath = `${this.#filePath}.tmp`;
        
        // Ensure directory exists
        const dir = path.dirname(this.#filePath);
        await fsp.mkdir(dir, { recursive: true });

        await fsp.writeFile(tmpPath, JSON.stringify(data, null, 2), "utf-8");
        await fsp.rename(tmpPath, this.#filePath);
        logger.debug(`[SkillWhitelist] Saved ${Object.keys(data).length} entries to disk.`);
    }

    /**
     * Flush pending saves and clean up timer.
     */
    async dispose(): Promise<void> {
        if (this.#saveTimer) {
            clearTimeout(this.#saveTimer);
            this.#saveTimer = null;
            await this.#save();
        }
    }
}
