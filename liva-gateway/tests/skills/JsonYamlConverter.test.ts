import { describe, it, expect, vi, afterEach } from "vitest";
import * as fs from "node:fs";
import path from "node:path";
import os from "node:os";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

import * as JsonYamlConverter from "../../src/skills/data/JsonYamlConverter";

describe("JsonYamlConverter Skill", () => {
    const testDir = path.join(os.tmpdir(), "json_yaml_test");

    afterEach(() => {
        try { fs.rmSync(testDir, { recursive: true, force: true }); } catch {}
    });

    it("should have correct metadata", () => {
        expect(JsonYamlConverter.metadata.name).toBe("json_yaml_converter");
        expect(JsonYamlConverter.metadata.parameters.required).toContain("from");
        expect(JsonYamlConverter.metadata.parameters.required).toContain("to");
    });

    describe("JSON → YAML", () => {
        it("should convert simple JSON to YAML", async () => {
            const result = await JsonYamlConverter.execute({
                content: JSON.stringify({ name: "LIVA", version: 2, active: true }),
                from: "json",
                to: "yaml"
            });
            expect(result).toContain("CONVERT SUCCESS");
            expect(result).toContain("name: LIVA");
            expect(result).toContain("version: 2");
            expect(result).toContain("active: true");
        });

        it("should convert nested JSON to YAML", async () => {
            const result = await JsonYamlConverter.execute({
                content: JSON.stringify({ server: { host: "localhost", port: 8080 } }),
                from: "json",
                to: "yaml"
            });
            expect(result).toContain("server:");
            expect(result).toContain("host: localhost");
            expect(result).toContain("port: 8080");
        });

        it("should handle arrays", async () => {
            const result = await JsonYamlConverter.execute({
                content: JSON.stringify({ items: ["a", "b", "c"] }),
                from: "json",
                to: "yaml"
            });
            expect(result).toContain("items:");
            expect(result).toContain("- a");
            expect(result).toContain("- b");
        });
    });

    describe("YAML → JSON", () => {
        it("should convert simple YAML to JSON", async () => {
            const yaml = `name: LIVA\nversion: 2\nactive: true`;
            const result = await JsonYamlConverter.execute({
                content: yaml,
                from: "yaml",
                to: "json"
            });
            expect(result).toContain("CONVERT SUCCESS");
            const jsonPart = result.match(/```json\n([\s\S]+?)\n```/)?.[1];
            expect(jsonPart).toBeTruthy();
            const parsed = JSON.parse(jsonPart!);
            expect(parsed.name).toBe("LIVA");
            expect(parsed.version).toBe(2);
            expect(parsed.active).toBe(true);
        });

        it("should handle null and special values", async () => {
            const yaml = `key1: null\nkey2: true\nkey3: false`;
            const result = await JsonYamlConverter.execute({
                content: yaml,
                from: "yaml",
                to: "json"
            });
            const jsonPart = result.match(/```json\n([\s\S]+?)\n```/)?.[1];
            const parsed = JSON.parse(jsonPart!);
            expect(parsed.key1).toBeNull();
            expect(parsed.key2).toBe(true);
            expect(parsed.key3).toBe(false);
        });
    });

    describe("File I/O", () => {
        it("should read from file and convert", async () => {
            fs.mkdirSync(testDir, { recursive: true });
            const inputFile = path.join(testDir, "input.json");
            fs.writeFileSync(inputFile, JSON.stringify({ hello: "world" }));

            const result = await JsonYamlConverter.execute({
                filePath: inputFile,
                from: "json",
                to: "yaml"
            });
            expect(result).toContain("hello: world");
        });

        it("should write output to file with atomic pattern", async () => {
            fs.mkdirSync(testDir, { recursive: true });
            const outputFile = path.join(testDir, "output.yaml");

            await JsonYamlConverter.execute({
                content: JSON.stringify({ name: "LIVA" }),
                from: "json",
                to: "yaml",
                outputPath: outputFile
            });

            expect(fs.existsSync(outputFile)).toBe(true);
            const content = fs.readFileSync(outputFile, "utf-8");
            expect(content).toContain("name: LIVA");
        });
    });

    describe("Edge cases", () => {
        it("should skip conversion when from === to", async () => {
            const result = await JsonYamlConverter.execute({
                content: '{"a": 1}',
                from: "json",
                to: "json"
            });
            expect(result).toContain("INFO");
            expect(result).toContain("giống nhau");
        });

        it("should require filePath or content", async () => {
            const result = await JsonYamlConverter.execute({
                from: "json",
                to: "yaml"
            });
            expect(result).toContain("ERROR");
        });

        it("should handle invalid JSON gracefully", async () => {
            const result = await JsonYamlConverter.execute({
                content: "not valid json {{{",
                from: "json",
                to: "yaml"
            });
            expect(result).toContain("ERROR");
        });
    });
});
