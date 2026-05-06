/**
 * NLCommandTranslator — Natural Language to IDE Command Translator (Phase 2)
 * =========================================================================
 * Translates natural language messages from remote channels (e.g., Telegram)
 * into structured IDE commands (VS Code or Antigravity).
 *
 * Uses a fast LLM extraction pattern to determine the intent and arguments.
 *
 * [v5.0] LIVA Remote Control Hub
 */

import { OpenAI } from "openai";
import { logger } from "../utils/logger";

// ===========================
// Types
// ===========================

export type IDEActionType = 
    | "open_file"
    | "run_terminal"
    | "insert_text"
    | "save_file"
    | "search_project"
    | "unknown";

export interface TranslatedIDECommand {
    action: IDEActionType;
    args: Record<string, any>;
    confidence: number;
    reasoning: string;
}

// ===========================
// NLCommandTranslator
// ===========================

export class NLCommandTranslator {
    readonly #llm: OpenAI;
    readonly #modelName: string;

    constructor() {
        const AI_PROVIDER = process.env.AI_PROVIDER?.toLowerCase() || "local";
        const routerPort = process.env.ROUTER_PORT || 8000;
        
        this.#modelName = process.env.ROUTER_MODEL_NAME || "local-router";

        let defaultBaseUrl = `http://127.0.0.1:${routerPort}/v1`;
        let defaultApiKey = "liva-translator-token";
        /* istanbul ignore if */
        if (AI_PROVIDER === "cloud") {
            defaultBaseUrl = process.env.AI_BASE_URL || "";
            defaultApiKey = process.env.AI_API_KEY || "";
        }

        this.#llm = new OpenAI({
            baseURL: defaultBaseUrl,
            apiKey: defaultApiKey,
            timeout: 10000,
            maxRetries: 1,
        });
    }

    /**
     * Translates a natural language string into a structured IDE command.
     */
    public async translate(nlText: string, context?: string): Promise<TranslatedIDECommand> {
        try {
            const systemPrompt = `You are an expert IDE command translator.
Your job is to translate a user's natural language request into a specific IDE action.
Available Actions:
1. "open_file" (args: { filePath: string }) - User wants to open or view a file.
2. "run_terminal" (args: { command: string }) - User wants to run a shell/terminal command.
3. "insert_text" (args: { text: string }) - User wants to type or insert text into the active file.
4. "save_file" (args: {}) - User wants to save the current file.
5. "search_project" (args: { query: string }) - User wants to search for text across the project.
6. "unknown" (args: {}) - Cannot confidently map to any action.

${context ? `Current Context:\n${context}\n` : ""}
RESPOND ONLY WITH VALID JSON using this schema:
{
  "action": "open_file" | "run_terminal" | "insert_text" | "save_file" | "search_project" | "unknown",
  "args": { ... },
  "confidence": <number between 0 and 1>,
  "reasoning": "<brief explanation>"
}`;

            const response = await this.#llm.chat.completions.create({
                model: this.#modelName,
                messages: [
                    { role: "system", content: systemPrompt },
                    { role: "user", content: nlText }
                ],
                temperature: 0.1,
                response_format: { type: "json_object" }
            });

/* istanbul ignore next */
            const content = response.choices[0]?.message?.content || "{}";
            const parsed = JSON.parse(content) as TranslatedIDECommand;
            
            // Validate basic structure
            if (!parsed.action || typeof parsed.confidence !== "number") {
                return this.#createUnknownCommand("Invalid JSON structure from LLM");
            }

            logger.info(`[NLTranslator] Translated "${nlText}" -> ${parsed.action} (conf: ${parsed.confidence})`);
            return parsed;

        } catch (error: unknown) {
        const errMsg = error instanceof Error ? error.message : String(error);
            logger.error(`[NLTranslator] Translation failed: ${errMsg}`);
            return this.#createUnknownCommand(errMsg);
        }
    }

    #createUnknownCommand(reason: string): TranslatedIDECommand {
        return {
            action: "unknown",
            args: {},
            confidence: 0,
            reasoning: reason
        };
    }
}
