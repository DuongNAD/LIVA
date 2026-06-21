import { GeminiAPI } from "../../tools/GeminiAPI";
import { SkillMetadata } from "../SkillMetadata";
import { logger } from "../../utils/logger";
import { z } from "zod";

export const metadata: SkillMetadata = {
  name: "dictionary_lookup",
  category: "core",
  short_desc: "Look up a word in the dictionary.",
  description: "Look up a word's definition, phonetics, parts of speech, synonyms, antonyms, and translation using Gemini API.",
  parameters: {
    type: "object",
    properties: {
      word: {
        type: "string",
        description: "The word to look up (e.g., 'benevolent')."
      }
    },
    required: ["word"]
  }
};

export interface DictionaryResult {
  word: string;
  phonetics: string;
  partsOfSpeech: Array<{
    pos: string;
    definitions: string[];
    examples: string[];
  }>;
  synonyms: string[];
  antonyms: string[];
  translation: string;
}

const dictionarySchema = {
  type: "object",
  properties: {
    word: { type: "string", description: "The word looked up." },
    phonetics: { type: "string", description: "Phonetic transcription (e.g. IPA) of the word." },
    partsOfSpeech: {
      type: "array",
      description: "List of parts of speech, definitions, and examples.",
      items: {
        type: "object",
        properties: {
          pos: { type: "string", description: "Part of speech (e.g., noun, verb, adjective)." },
          definitions: { type: "array", items: { type: "string" }, description: "Definitions of the word for this part of speech." },
          examples: { type: "array", items: { type: "string" }, description: "Example sentences using the word in this context." }
        },
        required: ["pos", "definitions", "examples"]
      }
    },
    synonyms: { type: "array", items: { type: "string" }, description: "Synonyms of the word." },
    antonyms: { type: "array", items: { type: "string" }, description: "Antonyms of the word." },
    translation: { type: "string", description: "Vietnamese translation of the word." }
  },
  required: ["word", "phonetics", "partsOfSpeech", "synonyms", "antonyms", "translation"]
};

const argsSchema = z.object({
  word: z.string({
    message: "No word provided for dictionary lookup.",
  })
  .trim()
  .min(1, "No word provided for dictionary lookup.")
  .max(100, "Word exceeds maximum length of 100 characters.")
});

export const execute = async (rawArgs: unknown): Promise<string> => {
  const parsed = argsSchema.safeParse(rawArgs);
  if (!parsed.success) {
    return `Error: ${parsed.error.issues.map(e => e.message).join(", ")}`;
  }
  const args = parsed.data;

  const word = args.word;
  logger.info(`[Skill: dictionary_lookup] Looking up word: ${word}`);

  try {
    const prompt = `Look up the word "${word}" in the dictionary. Provide its pronunciation/phonetics, grammatical roles/parts of speech with definitions and examples, synonyms, antonyms, and Vietnamese translation.`;

    const result = await GeminiAPI.generateStructured<DictionaryResult>(prompt, dictionarySchema);

    if (!result || !result.word) {
      return `Failed to look up word "${word}" (empty result).`;
    }

    let output = `# Dictionary Lookup: **${result.word}**\n`;
    if (result.phonetics) {
      output += `*Phonetics:* \`${result.phonetics}\`\n`;
    }
    if (result.translation) {
      output += `*Translation:* **${result.translation}**\n\n`;
    }

    if (Array.isArray(result.partsOfSpeech) && result.partsOfSpeech.length > 0) {
      output += `## Definitions\n`;
      result.partsOfSpeech.forEach((item) => {
        if (!item) return;
        const posStr = item.pos || "unknown";
        output += `### *${posStr}*\n`;
        if (Array.isArray(item.definitions)) {
          item.definitions.forEach((def, index) => {
            if (typeof def !== 'string') return;
            output += `${index + 1}. ${def}\n`;
            if (Array.isArray(item.examples) && typeof item.examples[index] === 'string') {
              output += `   *Example:* "${item.examples[index]}"\n`;
            }
          });
        }
        output += `\n`;
      });
    }

    if (Array.isArray(result.synonyms) && result.synonyms.length > 0) {
      output += `**Synonyms:** ${result.synonyms.filter(s => typeof s === 'string').join(", ")}\n`;
    }
    if (Array.isArray(result.antonyms) && result.antonyms.length > 0) {
      output += `**Antonyms:** ${result.antonyms.filter(a => typeof a === 'string').join(", ")}\n`;
    }

    return output.trim();
  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    logger.error(`[Skill: dictionary_lookup] Error: ${errMsg}`);
    return `Error: ${errMsg}`;
  }
};
