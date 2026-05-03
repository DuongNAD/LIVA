import { describe, it, expect } from "vitest";
import { parseAxTree, formatAxSnapshot, getInteractiveElements, findElement, getAxTreeStats } from "../../src/utils/AxTreeParser";

describe("AxTreeParser", () => {
    it("should parse raw AX nodes and filter out noise", () => {
        const rawNodes: any[] = [
            {
                nodeId: "1",
                role: { value: "button" },
                name: { value: "Submit" },
                properties: [{ name: "focused", value: { value: true } }]
            },
            {
                nodeId: "2",
                role: { value: "presentation" }, // Noise role
                name: { value: "" }
            },
            {
                nodeId: "3",
                role: { value: "textbox" },
                name: { value: "Search" },
                value: { value: "query text" }
            }
        ];

        const elements = parseAxTree(rawNodes);

        // Should ignore presentation role
        expect(elements).toHaveLength(2);

        expect(elements[0].role).toBe("button");
        expect(elements[0].name).toBe("Submit");
        expect(elements[0].state).toContain("focused");

        expect(elements[1].role).toBe("textbox");
        expect(elements[1].name).toBe("Search");
        expect(elements[1].value).toBe("query text");
    });

    it("should assign sequential IDs", () => {
        const rawNodes: any[] = [
            { nodeId: "a", role: { value: "link" }, name: { value: "A" } },
            { nodeId: "b", role: { value: "link" }, name: { value: "B" } }
        ];

        const elements = parseAxTree(rawNodes);
        expect(elements[0].id).toBe(1);
        expect(elements[1].id).toBe(2);
    });

    it("should calculate nesting depth using BFS", () => {
        const rawNodes: any[] = [
            { nodeId: "root", role: { value: "group" }, name: { value: "Root" }, childIds: ["child1", "child2"] },
            { nodeId: "child1", parentId: "root", role: { value: "button" }, name: { value: "C1" } },
            { nodeId: "child2", parentId: "root", role: { value: "button" }, name: { value: "C2" }, childIds: ["grandchild"] },
            { nodeId: "grandchild", parentId: "child2", role: { value: "link" }, name: { value: "GC" } }
        ];

        const elements = parseAxTree(rawNodes);
        // Include semantic nodes is true by default
        const elRoot = elements.find(e => e.name === "Root");
        const elC1 = elements.find(e => e.name === "C1");
        const elGC = elements.find(e => e.name === "GC");
        
        expect(elRoot?.depth).toBe(0);
        expect(elC1?.depth).toBe(1);
        expect(elGC?.depth).toBe(2);
    });

    it("should format elements into LLM context correctly", () => {
        const rawNodes: any[] = [
            { nodeId: "1", role: { value: "button" }, name: { value: "Login" } },
            { nodeId: "2", role: { value: "textbox" }, name: { value: "Username" }, value: { value: "admin" } }
        ];

        const elements = parseAxTree(rawNodes);
        const snapshot = formatAxSnapshot(elements);

        expect(snapshot).toContain("[AxTree Snapshot — 2 elements]");
        expect(snapshot).toContain('id=1 | button | "Login"');
        expect(snapshot).toContain('id=2 | textbox | "Username" [value="admin"]');
    });

    it("should enforce token budget by truncating string", () => {
        const rawNodes: any[] = Array.from({ length: 100 }, (_, i) => ({
            nodeId: String(i),
            role: { value: "button" },
            name: { value: `Button ${i}` }
        }));

        const elements = parseAxTree(rawNodes);
        
        // Very small token budget (10 tokens = ~40 chars)
        const snapshot = formatAxSnapshot(elements, 10);
        
        expect(snapshot).toContain("cắt ngắn");
    });

    it("should return empty message when formatting empty elements", () => {
        const snapshot = formatAxSnapshot([]);
        expect(snapshot).toContain("[AxTree] Trang trống hoặc không có phần tử tương tác.");
    });
});

describe("getInteractiveElements", () => {
    it("should filter only interactive roles", () => {
        const elements = [
            { id: 1, role: "button", name: "Btn", depth: 0 },
            { id: 2, role: "heading", name: "Header", depth: 0 }, // non-interactive
            { id: 3, role: "textbox", name: "Input", depth: 0 }
        ];

        const interactive = getInteractiveElements(elements);
        expect(interactive).toHaveLength(2);
        expect(interactive[0].role).toBe("button");
        expect(interactive[1].role).toBe("textbox");
    });
});

describe("findElement", () => {
    const elements = [
        { id: 1, role: "button", name: "Submit Form", depth: 0 },
        { id: 2, role: "link", name: "Cancel", depth: 0 },
        { id: 3, role: "button", name: "Logout", depth: 0 }
    ];

    it("should find element by role and string pattern (case-insensitive)", () => {
        const el = findElement(elements, "button", "submit");
        expect(el).toBeDefined();
        expect(el?.name).toBe("Submit Form");
    });

    it("should find element by role and RegExp pattern", () => {
        const el = findElement(elements, "button", /^log/i);
        expect(el).toBeDefined();
        expect(el?.name).toBe("Logout");
    });

    it("should return undefined if not found", () => {
        const el = findElement(elements, "textbox", "Submit");
        expect(el).toBeUndefined();
    });
});

describe("getAxTreeStats", () => {
    it("should return accurate statistics", () => {
        const elements = [
            { id: 1, role: "button", name: "Btn", depth: 0 }, // interactive
            { id: 2, role: "heading", name: "H1", depth: 0 }, // semantic
            { id: 3, role: "link", name: "A", depth: 0 }, // interactive
            { id: 4, role: "img", name: "Logo", depth: 0 } // semantic
        ];

        const stats = getAxTreeStats(elements);
        expect(stats.total).toBe(4);
        expect(stats.interactive).toBe(2);
        expect(stats.semantic).toBe(2);
        expect(stats.estimatedTokens).toBeGreaterThan(0);
    });
});

