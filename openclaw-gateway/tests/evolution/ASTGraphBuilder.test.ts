import { describe, it, expect, vi, beforeEach } from "vitest";
import { ASTGraphBuilder } from "../../src/evolution/ASTGraphBuilder";
import * as fs from "node:fs";
import * as path from "node:path";

vi.mock("node:fs", async (importOriginal) => {
    const actual: any = await importOriginal();
    return {
        ...actual,
        promises: {
            ...actual.promises,
            readdir: vi.fn(),
            readFile: vi.fn()
        }
    };
});
vi.mock("@ast-grep/napi", () => {
    return {
        lang: { TypeScript: "typescript", JavaScript: "javascript" },
        parse: vi.fn((language, sourceCode) => {
            if (sourceCode === "syntax error") {
                throw new Error("Parse error");
            }
            return {
                root: () => {
                    return {
                        findAll: ({ rule }: any) => {
                            if (rule.kind === "function_declaration" && sourceCode.includes("function testFunc")) {
                                return [{
                                    find: ({rule: r}: any) => r.kind === "identifier" ? { text: () => "testFunc" } : null,
                                    range: () => ({ start: { line: 0 }, end: { line: 2 } }),
                                    findAll: ({rule: r}: any) => {
                                        if (r.kind === "call_expression") {
                                            return [{ find: () => ({ text: () => "console.log" }) }];
                                        }
                                        return [];
                                    },
                                    text: () => "function testFunc() { console.log('test'); }"
                                }, {
                                    // Anonymous function branch
                                    find: () => null,
                                    range: () => ({ start: { line: 10 }, end: { line: 12 } }),
                                    findAll: () => [],
                                    text: () => "function () {}"
                                }];
                            }
                            if (rule.kind === "class_declaration" && sourceCode.includes("class TestClass")) {
                                return [{
                                    find: ({rule: r}: any) => r.kind === "identifier" ? { text: () => "TestClass" } : null,
                                    range: () => ({ start: { line: 4 }, end: { line: 10 } }),
                                    findAll: ({ rule: r }: any) => {
                                        if (r.kind === "method_definition") {
                                            return [{
                                                find: ({rule: rr}: any) => rr.kind === "property_identifier" ? { text: () => "testMethod" } : null,
                                                range: () => ({ start: { line: 5 }, end: { line: 8 } }),
                                                findAll: ({rule: rr}: any) => {
                                                    if (rr.kind === "call_expression") {
                                                        return [{ find: () => ({ text: () => "super" }) }];
                                                    }
                                                    return [];
                                                },
                                                text: () => "testMethod() { super(); }"
                                            }, {
                                                // anonymous method
                                                find: () => null,
                                                range: () => ({ start: { line: 6 }, end: { line: 7 } }),
                                                findAll: () => [],
                                                text: () => "() {}"
                                            }];
                                        }
                                        return [];
                                    },
                                    text: () => "class TestClass { testMethod() { super(); } }"
                                }, {
                                    // Anonymous class branch
                                    find: () => null,
                                    range: () => ({ start: { line: 20 }, end: { line: 22 } }),
                                    findAll: () => [],
                                    text: () => "class {}"
                                }];
                            }
                            return [];
                        }
                    };
                }
            };
        })
    };
});

describe("ASTGraphBuilder", () => {
    let builder: ASTGraphBuilder;
    const testDir = "/test/dir";

    beforeEach(() => {
        builder = new ASTGraphBuilder(testDir);
        vi.clearAllMocks();
    });

    it("should build graph correctly", async () => {
        vi.mocked(fs.promises.readdir).mockImplementation((dir: any) => {
            if (dir === testDir) {
                return Promise.resolve([
                    { name: "test.ts", isDirectory: () => false },
                    { name: "node_modules", isDirectory: () => true },
                    { name: "dist", isDirectory: () => true },
                    { name: "sub", isDirectory: () => true },
                    { name: "empty.js", isDirectory: () => false },
                    { name: "error.js", isDirectory: () => false }
                ] as any);
            }
            if (dir === path.resolve(testDir, "sub")) {
                return Promise.resolve([
                    { name: "sub.tsx", isDirectory: () => false }
                ] as any);
            }
            return Promise.resolve([]);
        });

        vi.mocked(fs.promises.readFile).mockImplementation((file: any) => {
            if (file.endsWith("test.ts") || file.endsWith("sub.tsx")) {
                return Promise.resolve("function testFunc() { console.log('test'); }\n\nclass TestClass { testMethod() { super(); } }");
            }
            if (file.endsWith("error.js")) {
                return Promise.resolve("syntax error");
            }
            return Promise.resolve("");
        });

        const graph = await builder.buildGraph();
        
        expect(graph.type).toBe("repository");
        // Expect test.ts and sub.tsx to be parsed. empty.js is skipped, error.js errors out.
        expect(graph.children.length).toBe(2); 

        const fileNode = graph.children.find(c => c.name.includes("test.ts"))!;
        expect(fileNode.type).toBe("file");
        
        const funcNode = fileNode.children.find(c => c.name === "testFunc")!;
        expect(funcNode.type).toBe("function");
        expect(funcNode.calls).toContain("console.log");

        const anonFuncNode = fileNode.children.find(c => c.name === "anonymous_func")!;
        expect(anonFuncNode.type).toBe("function");

        const classNode = fileNode.children.find(c => c.name === "TestClass")!;
        expect(classNode.type).toBe("class");

        const methodNode = classNode.children.find(c => c.name === "testMethod")!;
        expect(methodNode.type).toBe("method");
        expect(methodNode.calls).toContain("super");

        const anonMethodNode = classNode.children.find(c => c.name === "anonymous_method")!;
        expect(anonMethodNode.type).toBe("method");

        const anonClassNode = fileNode.children.find(c => c.name === "anonymous_class")!;
        expect(anonClassNode.type).toBe("class");
    });
});
