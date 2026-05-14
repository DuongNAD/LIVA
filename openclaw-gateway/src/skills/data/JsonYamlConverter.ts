import { z } from "zod";
import { logger } from "@utils/logger";
import * as fs from "node:fs/promises";
import path from "node:path";

const ConverterSchema = z.object({
  filePath: z.string().optional().describe("Đường dẫn file gốc (JSON hoặc YAML)"),
  content: z.string().optional().describe("Nội dung text gốc (nếu không truyền filePath)"),
  from: z.enum(["json", "yaml"]).describe("Định dạng đầu vào"),
  to: z.enum(["json", "yaml"]).describe("Định dạng đầu ra"),
  outputPath: z.string().optional().describe("Đường dẫn file đầu ra (nếu muốn ghi ra file)"),
});

export const metadata = {
  name: "json_yaml_converter",
  description: "[AUTO_RUN] Convert data between JSON and YAML. Supports reading from file or direct text. Zero-dependency (custom YAML parser, no external libraries).",
  kit: "DATA_KIT",
  search_keywords: ["json", "yaml", "convert", "chuyển đổi", "config", "cấu hình"],
  parameters: {
    type: "object",
    properties: {
      filePath: { type: "string" },
      content: { type: "string" },
      from: { type: "string", enum: ["json", "yaml"] },
      to: { type: "string", enum: ["json", "yaml"] },
      outputPath: { type: "string" },
    },
    required: ["from", "to"],
  },
};

/**
 * Lightweight YAML serializer (supports flat + nested objects, arrays, strings, numbers, booleans, null).
 * Zero-dependency — no `js-yaml` needed.
 */
function jsonToYaml(obj: unknown, indent: number = 0): string {
    const pad = "  ".repeat(indent);

    if (obj === null || obj === undefined) return `${pad}null`;
    if (typeof obj === "boolean") return `${pad}${obj}`;
    if (typeof obj === "number") return `${pad}${obj}`;
    if (typeof obj === "string") {
        // Quote strings that contain special YAML characters
        if (obj.includes(":") || obj.includes("#") || obj.includes("\n") || obj.startsWith("{") || obj.startsWith("[") || obj.trim() === "") {
            return `${pad}"${obj.replace(/"/g, '\\"')}"`;
        }
        return `${pad}${obj}`;
    }

    if (Array.isArray(obj)) {
        if (obj.length === 0) return `${pad}[]`;
        return obj.map(item => {
            if (typeof item === "object" && item !== null) {
                const inner = jsonToYaml(item, indent + 1).trimStart();
                return `${pad}- ${inner}`;
            }
            return `${pad}- ${typeof item === "string" && (item.includes(":") || item.includes("#")) ? `"${item}"` : item}`;
        }).join("\n");
    }

    if (typeof obj === "object") {
        const entries = Object.entries(obj as Record<string, unknown>);
        if (entries.length === 0) return `${pad}{}`;
        return entries.map(([key, val]) => {
            if (val === null || val === undefined) {
                return `${pad}${key}: null`;
            }
            if (typeof val === "object") {
                const nested = jsonToYaml(val, indent + 1);
                return `${pad}${key}:\n${nested}`;
            }
            const scalarVal = jsonToYaml(val, 0).trim();
            return `${pad}${key}: ${scalarVal}`;
        }).join("\n");
    }

    return `${pad}${String(obj)}`;
}

/**
 * Lightweight YAML parser (flat + 1-level nesting).
 * Handles key: value pairs, arrays with - prefix, quoted strings.
 */
function yamlToJson(yamlStr: string): unknown {
    const result: Record<string, unknown> = {};
    const lines = yamlStr.split("\n");
    let currentKey: string | null = null;
    let currentArray: unknown[] | null = null;

    for (const rawLine of lines) {
        const line = rawLine.replace(/\r$/, "");

        // Skip comments and empty lines
        if (line.trim() === "" || line.trim().startsWith("#")) continue;

        const indentLevel = line.search(/\S/);
        const trimmed = line.trim();

        // Array item
        if (trimmed.startsWith("- ")) {
            const val = trimmed.substring(2).trim();
            if (currentKey && currentArray) {
                currentArray.push(parseYamlValue(val));
            }
            continue;
        }

        // Key: Value pair
        const colonIdx = trimmed.indexOf(":");
        if (colonIdx > 0) {
            // Save previous array if pending
            if (currentKey && currentArray) {
                result[currentKey] = currentArray;
                currentArray = null;
            }

            const key = trimmed.substring(0, colonIdx).trim();
            const rawVal = trimmed.substring(colonIdx + 1).trim();

            if (rawVal === "" || rawVal === undefined) {
                // This key has a nested object or array — for simplicity, treat next lines as array
                currentKey = key;
                currentArray = [];
            } else {
                currentKey = null;
                currentArray = null;
                result[key] = parseYamlValue(rawVal);
            }
        }
    }

    // Flush any remaining array
    if (currentKey && currentArray) {
        result[currentKey] = currentArray;
    }

    return result;
}

function parseYamlValue(val: string): unknown {
    if (val === "null" || val === "~") return null;
    if (val === "true") return true;
    if (val === "false") return false;
    if (/^-?\d+$/.test(val)) return parseInt(val, 10);
    if (/^-?\d+\.\d+$/.test(val)) return parseFloat(val);
    // Remove quotes
    if ((val.startsWith('"') && val.endsWith('"')) || (val.startsWith("'") && val.endsWith("'"))) {
        return val.slice(1, -1);
    }
    return val;
}

export const execute = async (argsObj: unknown): Promise<string> => {
    try {
        const parsed = ConverterSchema.parse(argsObj);
        let sourceContent: string;

        // Get source content
        if (parsed.filePath) {
            const absPath = path.resolve(process.cwd(), parsed.filePath);
            sourceContent = await fs.readFile(absPath, "utf-8");
            logger.info(`[JsonYamlConverter] Đọc file: ${absPath}`);
        } else if (parsed.content) {
            sourceContent = parsed.content;
        } else {
            return "[CONVERT ERROR] Cần cung cấp 'filePath' hoặc 'content' làm đầu vào.";
        }

        if (parsed.from === parsed.to) {
            return `[CONVERT INFO] Định dạng đầu vào và đầu ra giống nhau (${parsed.from}). Không cần chuyển đổi.\n\n${sourceContent}`;
        }

        let result: string;

        if (parsed.from === "json" && parsed.to === "yaml") {
            const jsonData = JSON.parse(sourceContent);
            result = jsonToYaml(jsonData);
        } else if (parsed.from === "yaml" && parsed.to === "json") {
            const yamlData = yamlToJson(sourceContent);
            result = JSON.stringify(yamlData, null, 2);
        } else {
            return `[CONVERT ERROR] Không hỗ trợ chuyển đổi từ ${parsed.from} sang ${parsed.to}.`;
        }

        // Write to file if requested — Atomic Write pattern
        if (parsed.outputPath) {
            const absOut = path.resolve(process.cwd(), parsed.outputPath);
            const tmpPath = `${absOut}.tmp`;
            await fs.writeFile(tmpPath, result, "utf-8");
            await fs.rename(tmpPath, absOut);
            logger.info(`[JsonYamlConverter] Đã ghi kết quả ra: ${absOut}`);
            return `[CONVERT SUCCESS] Đã chuyển đổi ${parsed.from.toUpperCase()} → ${parsed.to.toUpperCase()} và ghi ra file: ${absOut}`;
        }

        logger.info(`[JsonYamlConverter] Hoàn tất chuyển đổi ${parsed.from} → ${parsed.to}.`);
        return `[CONVERT SUCCESS] ${parsed.from.toUpperCase()} → ${parsed.to.toUpperCase()}:\n\n\`\`\`${parsed.to}\n${result}\n\`\`\``;

    } catch (error: unknown) {
        const msg = error instanceof z.ZodError
            ? `Sai định dạng: ${error.issues.map(e => e.message).join(", ")}`
            : (error instanceof Error ? error.message : "Unknown error");
        logger.error(`[JsonYamlConverter] Lỗi: ${msg}`);
        return `[CONVERT ERROR] ${msg}`;
    }
};
