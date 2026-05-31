import { describe, it, expect, vi, beforeEach } from "vitest";
import { BookIndex, BookNode } from "../../src/memory/BookIndex";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

describe("BookIndex", () => {
    let index: BookIndex;

    beforeEach(() => {
        index = new BookIndex();
    });

    describe("Node Management", () => {
        it("should add a node successfully", () => {
            const node: BookNode = { id: "1", text: "Leaf 1", level: 0, isSummary: false };
            index.addNode(node);
            
            expect(index.nodeCount).toBe(1);
            expect(index.getNode("1")).toEqual(node);
        });

        it("should replace attributes if node already exists", () => {
            const node: BookNode = { id: "1", text: "Leaf 1", level: 0, isSummary: false };
            index.addNode(node);
            
            const updatedNode: BookNode = { id: "1", text: "Updated Leaf", level: 0, isSummary: false };
            index.addNode(updatedNode);
            
            expect(index.nodeCount).toBe(1);
            expect(index.getNode("1")).toEqual(updatedNode);
        });

        it("should return null for non-existent node", () => {
            expect(index.getNode("invalid")).toBeNull();
        });
    });

    describe("Edge Management", () => {
        it("should add a directed edge between existing nodes", () => {
            const parent: BookNode = { id: "parent1", text: "Summary 1", level: 1, isSummary: true };
            const child: BookNode = { id: "child1", text: "Leaf 1", level: 0, isSummary: false };
            
            index.addNode(parent);
            index.addNode(child);
            index.addEdge("parent1", "child1");
            
            const children = index.getChildren("parent1");
            expect(children).toHaveLength(1);
            expect(children[0]).toEqual(child);
        });

        it("should ignore adding edge if parent or child does not exist", () => {
            const node: BookNode = { id: "1", text: "Node", level: 0, isSummary: false };
            index.addNode(node);
            
            index.addEdge("1", "non-existent");
            expect(index.getChildren("1")).toHaveLength(0);

            index.addEdge("non-existent", "1");
            expect(index.getChildren("non-existent")).toHaveLength(0);
        });
        
        it("should not add duplicate edges", () => {
            const parent: BookNode = { id: "parent1", text: "Summary 1", level: 1, isSummary: true };
            const child: BookNode = { id: "child1", text: "Leaf 1", level: 0, isSummary: false };
            
            index.addNode(parent);
            index.addNode(child);
            index.addEdge("parent1", "child1");
            index.addEdge("parent1", "child1"); // duplicate
            
            const children = index.getChildren("parent1");
            expect(children).toHaveLength(1);
        });
    });

    describe("Tree Operations", () => {
        beforeEach(() => {
            // Build a simple tree
            //          root (Level 2)
            //        /                \
            //  sum1 (Level 1)       sum2 (Level 1)
            //   /     \                |
            // lf1     lf2             lf3 (Level 0)
            
            index.addNode({ id: "root", text: "Root Summary", level: 2, isSummary: true });
            
            index.addNode({ id: "sum1", text: "Summary 1", level: 1, isSummary: true });
            index.addNode({ id: "sum2", text: "Summary 2", level: 1, isSummary: true });
            
            index.addNode({ id: "lf1", text: "Leaf 1", level: 0, isSummary: false });
            index.addNode({ id: "lf2", text: "Leaf 2", level: 0, isSummary: false });
            index.addNode({ id: "lf3", text: "Leaf 3", level: 0, isSummary: false });
            
            index.addEdge("root", "sum1");
            index.addEdge("root", "sum2");
            index.addEdge("sum1", "lf1");
            index.addEdge("sum1", "lf2");
            index.addEdge("sum2", "lf3");
        });

        it("should retrieve all nodes at a specific level", () => {
            const level0 = index.getAllNodesAtLevel(0);
            expect(level0).toHaveLength(3);
            expect(level0.map(n => n.id).sort()).toEqual(["lf1", "lf2", "lf3"]);

            const level1 = index.getAllNodesAtLevel(1);
            expect(level1).toHaveLength(2);
            expect(level1.map(n => n.id).sort()).toEqual(["sum1", "sum2"]);

            const level2 = index.getAllNodesAtLevel(2);
            expect(level2).toHaveLength(1);
            expect(level2[0].id).toBe("root");
        });

        it("should traverse tree using BFS", () => {
            const visited: string[] = [];
            index.traverseBFS("root", (node) => {
                visited.push(node.id);
            });
            
            // root -> sum1, sum2 -> lf1, lf2, lf3
            expect(visited[0]).toBe("root");
            expect(visited.slice(1, 3).sort()).toEqual(["sum1", "sum2"]);
            expect(visited.slice(3).sort()).toEqual(["lf1", "lf2", "lf3"]);
        });
        
        it("should do nothing when traversing from non-existent node", () => {
            const visited: string[] = [];
            index.traverseBFS("invalid", (node) => {
                visited.push(node.id);
            });
            expect(visited).toHaveLength(0);
        });

        it("should clear the index completely", () => {
            expect(index.nodeCount).toBe(6);
            index.clear();
            expect(index.nodeCount).toBe(0);
            expect(index.getAllNodesAtLevel(0)).toHaveLength(0);
        });
    });
});
