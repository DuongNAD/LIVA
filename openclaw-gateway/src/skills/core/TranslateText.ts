import { safeFetch } from "@utils/HttpClient";
import { logger } from "@utils/logger";

import { SkillMetadata } from "../SkillMetadata";

export const metadata: SkillMetadata = {
  name: "translate_text",
  category: "core",
  short_desc: "Translate text between languages.",
  semantic_tags: ["#translate", "#language", "#dich", "#ngonngu"],
  search_keywords: ["translate", "dịch", "translation", "ngôn ngữ", "language", "tiếng anh", "tiếng việt"],
  description: "[AUTO_RUN] Translate text between languages using the local AI engine. Supports any language pair.",
  parameters: {
    type: "object",
    properties: {
      text: { type: "string", description: "The text to translate (required)." },
      target_language: { type: "string", description: "Target language name e.g. 'English', 'Vietnamese', 'Japanese', 'Korean', 'Chinese', 'French'." },
      source_language: { type: "string", description: "Source language (optional — auto-detect if omitted)." },
    },
    required: ["text", "target_language"],
  },
};

export const execute = async (args: {
  text: string;
  target_language: string;
  source_language?: string;
}): Promise<string> => {
  if (!args.text?.trim()) return "Error: No text provided for translation.";
  if (!args.target_language?.trim()) return "Error: No target language specified.";

  const text = args.text.trim();
  const target = args.target_language.trim();
  const source = args.source_language?.trim() || "auto-detected";

  // Limit input to prevent token overflow
  const MAX_INPUT = 3000;
  if (text.length > MAX_INPUT) {
    return `Error: Text too long (${text.length} chars). Maximum is ${MAX_INPUT} characters. Please split into smaller parts.`;
  }

  logger.info(`[Skill: translate_text] Translating ${text.length} chars from ${source} → ${target}`);

  try {
    const llmUrl = process.env.LLM_ENDPOINT || "http://localhost:8000/v1/chat/completions";

    const response = await safeFetch(llmUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        model: "local",
        messages: [
          {
            role: "system",
            content: `You are a professional translator. Translate the user's text to ${target}. Output ONLY the translated text, nothing else. No explanations, no notes. Preserve formatting and tone.`,
          },
          {
            role: "user",
            content: text,
          },
        ],
        temperature: 0.3,
        max_tokens: 2048,
      }),
    }, 30000);

    const data = await response.json();
    const translated = data.choices?.[0]?.message?.content?.trim();

    if (!translated) {
      return "Translation failed: No output from AI engine.";
    }

    return `[Translation: ${source} → ${target}]\n\n${translated}`;
  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    logger.error(`[Skill: translate_text] Error: ${errMsg}`);
    return `Translation error: ${errMsg}`;
  }
};
