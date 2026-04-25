import { describe, it, expect } from "vitest";
import { parseAxTree, formatAxSnapshot, getInteractiveElements } from "../../src/utils/AxTreeParser";

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
});
