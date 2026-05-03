import { describe, it, expect, vi, beforeEach } from "vitest";
import { DeepResearchCron } from "../../src/memory/DeepResearchCron";
import { promises as fsp } from "node:fs";

vi.mock("node:fs", () => ({
    promises: {
        readdir: vi.fn(),
        readFile: vi.fn(),
        mkdir: vi.fn(),
        writeFile: vi.fn()
    }
}));

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        error: vi.fn()
    }
}));

describe("DeepResearchCron", () => {
    let cron: DeepResearchCron;
    let vaultManager: any;

    beforeEach(() => {
        vaultManager = {} as any;
        cron = new DeepResearchCron(vaultManager, "/mock/vault");
        vi.clearAllMocks();
    });

    it("should calculate Node Gravity and return Top Orphan Nodes", async () => {
        // Giả lập 2 files: note1.md và note2.md
        vi.mocked(fsp.readdir).mockImplementation(async (dir) => {
            if (dir === "/mock/vault" || dir.endsWith("vault")) { // Cross-platform path matching
                return [
                    { isDirectory: () => false, isFile: () => true, name: "note1.md" },
                    { isDirectory: () => false, isFile: () => true, name: "note2.md" }
                ] as any;
            }
            return [];
        });

        vi.mocked(fsp.readFile).mockImplementation(async (file) => {
            if (file.toString().includes("note1.md")) {
                return "Đây là [[Vi khuẩn DPAO|Alias]] và [[Nút mồ côi 1]].";
            }
            if (file.toString().includes("note2.md")) {
                return "Cũng nhắc lại [[Nút mồ côi 1]] và thêm [[Nút mồ côi 2]].";
            }
            return "";
        });

        const topOrphans = await cron.findTopOrphanNodes(2);

        // Nút mồ côi 1 có 2 backlink, Nút mồ côi 2 có 1 backlink, Vi khuẩn DPAO có 1
        expect(topOrphans.length).toBe(2);
        expect(topOrphans[0]).toBe("Nút mồ côi 1"); // Node Gravity cao nhất
        expect(topOrphans[1]).toBe("Vi khuẩn DPAO"); // Đứng thứ 2
    });

    it("should save draft research securely", async () => {
        await cron.draftResearch("AI Concept", "Research Data");
        
        expect(fsp.mkdir).toHaveBeenCalled();
        expect(fsp.writeFile).toHaveBeenCalledWith(
            expect.stringContaining("LIVA_Drafts"),
            "Research Data",
            "utf-8"
        );
    });
});
