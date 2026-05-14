import { logger } from "@utils/logger";
import { SkillMetadata } from "../SkillMetadata";

export const metadata: SkillMetadata = {
  name: "code_runner",
  category: "devops",
  short_desc: "Execute temporary code snippets safely.",
  semantic_tags: ["#code", "#run", "#script", "#javascript", "#sandbox"],
  search_keywords: ["run", "chạy", "code", "execute", "script", "javascript", "tính"],
  description: "Execute a JavaScript code snippet in a strict sandbox (isolated-vm). Returns stdout output.",
  requires_hitl: true,
  parameters: {
    type: "object",
    properties: {
      code: { type: "string", description: "The JavaScript code to execute (required)." },
      timeout: { type: "number", description: "Max execution time in seconds (default: 10, max: 30)." },
    },
    required: ["code"],
  },
};

export const execute = async (args: {
  code: string;
  timeout?: number;
}): Promise<string> => {
  if (!args.code?.trim()) return "Error: No code provided.";

  const timeoutMs = Math.min(Math.max(args.timeout || 10, 1), 30) * 1000;

  logger.info(`[Skill: code_runner] Running sandboxed JS snippet (timeout=${timeoutMs}ms)`);

  try {
    const ivm = await import("isolated-vm");
    const isolate = new ivm.Isolate({ memoryLimit: 64 });
    const context = await isolate.createContext();

    // Provide console.log capture
    const logs: string[] = [];
    const logCallback = new ivm.Reference((msg: string) => { logs.push(String(msg)); });
    await context.global.set("_log", logCallback);
    await context.eval(`globalThis.console = { log: (...a) => _log.applySync(undefined, [a.join(" ")]), error: (...a) => _log.applySync(undefined, ["[ERR] " + a.join(" ")]), warn: (...a) => _log.applySync(undefined, ["[WARN] " + a.join(" ")]) };`);

    const script = await isolate.compileScript(args.code);
    const result = await script.run(context, { timeout: timeoutMs });

    isolate.dispose();

    const output = logs.length > 0 ? logs.join("\n") : (result !== undefined ? String(result) : "(no output)");
    return `✅ Output:\n${output}`;
  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    return `❌ Runtime Error:\n${errMsg}`;
  }
};
