import { safeFetch } from "@utils/HttpClient";
import { logger } from "@utils/logger";

import { SkillMetadata } from "../SkillMetadata";

export const metadata: SkillMetadata = {
  name: "summarize_content",
  category: "web",
  short_desc: "Summarize web URL or text.",
  semantic_tags: ["#summary", "#tomtat", "#article", "#url"],
  search_keywords: ["summarize", "tóm tắt", "summary", "tóm lược", "article", "bài viết", "url"],
  description: "[AUTO_RUN] Summarize text content or a web page URL. Fetches URL content automatically and provides AI-powered summaries in different styles.",
  parameters: {
    type: "object",
    properties: {
      url: { type: "string", description: "URL of a web page to summarize (optional — use 'text' instead for raw content)." },
      text: { type: "string", description: "Raw text to summarize (optional — use 'url' instead for web pages)." },
      style: {
        type: "string",
        enum: ["brief", "detailed", "bullet_points"],
        description: "Summary style: 'brief' (2-3 sentences), 'detailed' (full paragraph), 'bullet_points' (key points list). Default: 'brief'.",
      },
    },
    required: [],
  },
};

async function fetchUrlContent(url: string): Promise<string> {
  const response = await safeFetch(url, {
    headers: { "User-Agent": "LIVA-Bot/1.0 (Content Summarizer)" },
  }, 15000);

  const html = await response.text();

  // Strip HTML tags — lightweight extraction
  const text = html
    .replaceAll(/<script[^>]*>[\s\S]*?<\/script>/gi, "")
    .replaceAll(/<style[^>]*>[\s\S]*?<\/style>/gi, "")
    .replaceAll(/<nav[^>]*>[\s\S]*?<\/nav>/gi, "")
    .replaceAll(/<footer[^>]*>[\s\S]*?<\/footer>/gi, "")
    .replaceAll(/<header[^>]*>[\s\S]*?<\/header>/gi, "")
    .replaceAll(/<[^>]+>/g, " ")
    .replaceAll(/\s+/g, " ")
    .trim();

  return text;
}

export const execute = async (args: {
  url?: string;
  text?: string;
  style?: "brief" | "detailed" | "bullet_points";
}): Promise<string> => {
  const style = args.style || "brief";
  let content = "";

  if (args.url?.trim()) {
    logger.info(`[Skill: summarize_content] Fetching URL: ${args.url}`);
    try {
      content = await fetchUrlContent(args.url.trim());
    } catch (error: unknown) {
      const errMsg = error instanceof Error ? error.message : String(error);
      return `Failed to fetch URL: ${errMsg}`;
    }
  } else if (args.text?.trim()) {
    content = args.text.trim();
  } else {
    return "Error: Please provide either 'url' or 'text' to summarize.";
  }

  if (content.length < 50) {
    return "Error: Content too short to summarize (minimum 50 characters).";
  }

  // Truncate to prevent token overflow
  const MAX_CONTENT = 6000;
  const truncated = content.length > MAX_CONTENT;
  if (truncated) content = content.substring(0, MAX_CONTENT);

  const styleInstructions: Record<string, string> = {
    brief: "Provide a concise summary in 2-3 sentences.",
    detailed: "Provide a comprehensive summary covering all main points in 1-2 paragraphs.",
    bullet_points: "Summarize the key points as a bullet-point list (5-10 points).",
  };

  logger.info(`[Skill: summarize_content] Summarizing ${content.length} chars (style=${style})`);

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
            content: `You are a content summarizer. ${styleInstructions[style]} Output ONLY the summary. Use Vietnamese if the content is in Vietnamese, otherwise match the content language.`,
          },
          { role: "user", content },
        ],
        temperature: 0.3,
        max_tokens: 1024,
      }),
    }, 30000);

    const data = await response.json();
    const summary = data.choices?.[0]?.message?.content?.trim();

    if (!summary) return "Summary failed: No output from AI engine.";

    let result = `[Summary — ${style}]${truncated ? " (content was truncated)" : ""}\n\n${summary}`;
    if (args.url) result += `\n\nSource: ${args.url}`;
    return result;
  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    return `Summary error: ${errMsg}`;
  }
};
