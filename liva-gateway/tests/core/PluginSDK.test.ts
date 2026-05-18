/**
 * PluginSDK.test.ts — Test plugin system interfaces and definePlugin factory
 */
import { describe, it, expect, vi } from "vitest";
import { definePlugin, LivaPluginContext, LivaPluginManifest } from "../../src/PluginSDK";

describe("PluginSDK", () => {
    const mockContext: LivaPluginContext = {
        sendToUI: vi.fn(),
        readMemory: vi.fn().mockResolvedValue([]),
        saveMemory: vi.fn().mockResolvedValue(undefined),
    };

    const testManifest: LivaPluginManifest = {
        id: "test-plugin",
        name: "Test Plugin",
        version: "1.0.0",
        permissions: ["disk_read"],
    };

    it("should create a plugin with definePlugin factory", () => {
        const plugin = definePlugin(testManifest, () => ({
            skills: [{ name: "test_skill" }],
        }));

        expect(plugin.manifest).toEqual(testManifest);
    });

    it("should return skills after onInstall", () => {
        const skills = [{ name: "skill_1" }, { name: "skill_2" }];
        const plugin = definePlugin(testManifest, () => ({ skills }));

        // Before install, skills are empty
        expect(plugin.getSkills()).toEqual([]);

        // After install, skills are populated
        plugin.onInstall(mockContext);
        expect(plugin.getSkills()).toEqual(skills);
    });

    it("should call onReady callback if provided", () => {
        const readyFn = vi.fn();
        const plugin = definePlugin(testManifest, () => ({
            skills: [],
            onReady: readyFn,
        }));

        plugin.onInstall(mockContext);
        plugin.onReady();
        expect(readyFn).toHaveBeenCalledTimes(1);
    });

    it("should not crash when onReady is not provided", () => {
        const plugin = definePlugin(testManifest, () => ({
            skills: [],
        }));

        plugin.onInstall(mockContext);
        expect(() => plugin.onReady()).not.toThrow();
    });

    it("should expose manifest properties correctly", () => {
        const plugin = definePlugin(testManifest, () => ({
            skills: [],
        }));

        expect(plugin.manifest.id).toBe("test-plugin");
        expect(plugin.manifest.name).toBe("Test Plugin");
        expect(plugin.manifest.version).toBe("1.0.0");
        expect(plugin.manifest.permissions).toContain("disk_read");
    });
});
