import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { promises as fsp } from "node:fs";
import * as path from "node:path";
import * as os from "node:os";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

import { metadata, execute } from "../../src/skills/personal/ExpenseTracker";

const EXPENSE_FILE = path.join(os.homedir(), ".liva", "expenses.json");

describe("ExpenseTracker", () => {
    let originalData: string | null = null;

    beforeEach(async () => {
        try { originalData = await fsp.readFile(EXPENSE_FILE, "utf-8"); } catch { originalData = null; }
        // Write empty array for clean test
        await fsp.mkdir(path.dirname(EXPENSE_FILE), { recursive: true });
        await fsp.writeFile(EXPENSE_FILE, "[]", "utf-8");
    });

    afterEach(async () => {
        // Restore original data
        if (originalData !== null) await fsp.writeFile(EXPENSE_FILE, originalData, "utf-8");
        else try { await fsp.unlink(EXPENSE_FILE); } catch {}
    });

    it("should have correct metadata", () => {
        expect(metadata.name).toBe("expense_tracker");
        expect(metadata.parameters.required).toContain("action");
    });

    it("should add an expense", async () => {
        const result = await execute({ action: "add", amount: 35000, description: "Cà phê", category: "food" });
        expect(result).toContain("✅");
        expect(result).toContain("35");
        expect(result).toContain("Cà phê");
    });

    it("should reject add without amount", async () => {
        const result = await execute({ action: "add", description: "test" });
        expect(result).toContain("Error");
    });

    it("should reject add without description", async () => {
        const result = await execute({ action: "add", amount: 100 });
        expect(result).toContain("Error");
    });

    it("should list expenses", async () => {
        await execute({ action: "add", amount: 50000, description: "Lunch", category: "food" });
        await execute({ action: "add", amount: 20000, description: "Grab", category: "transport" });
        const result = await execute({ action: "list", period: "today" });
        expect(result).toContain("Lunch");
        expect(result).toContain("Grab");
        expect(result).toContain("Total");
    });

    it("should show summary by category", async () => {
        await execute({ action: "add", amount: 100000, description: "Dinner", category: "food" });
        await execute({ action: "add", amount: 30000, description: "Bus", category: "transport" });
        const result = await execute({ action: "summary", period: "today" });
        expect(result).toContain("food");
        expect(result).toContain("transport");
        expect(result).toContain("Grand Total");
    });

    it("should return empty message when no expenses", async () => {
        const result = await execute({ action: "list", period: "today" });
        expect(result).toContain("No expenses");
    });
});
