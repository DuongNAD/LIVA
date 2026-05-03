/**
 * WebSearch — Tavily API Integration
 * ===================================
 * Upgraded from DuckDuckGo HTML scraping (fragile regex) to Tavily REST API.
 * Tavily is purpose-built for LLM/RAG — returns clean Markdown content
 * instead of raw HTML, saving massive token overhead.
 *
 * Free tier: 1000 queries/month (sufficient for personal use).
 * Set TAVILY_API_KEY in .env to activate.
 */
import { safeFetch } from "@utils/HttpClient";
import { logger } from "@utils/logger";

export const metadata = {
  name: "web_search",
  search_keywords: ["web_search","web search","tìm kiếm","tra cứu"],
  description:
    "Tìm kiếm thông tin trên Internet (Web Search). Sử dụng khi cần tra cứu các kiến thức, định nghĩa, hoặc thông tin mà AI chưa biết chắc chắn.",
  parameters: {
    type: "object",
    properties: {
      query: {
        type: "string",
        description: "Từ khóa cần tìm kiếm (Search query).",
      },
    },
    required: ["query"],
  },
};

const TAVILY_API_KEY = process.env.TAVILY_API_KEY || "";
const TAVILY_ENDPOINT = "https://api.tavily.com/search";

// Fallback: DuckDuckGo HTML scraping (when no API key)
async function fallbackDuckDuckGo(query: string): Promise<string> {
  const url = `https://html.duckduckgo.com/html/?q=${encodeURIComponent(query)}`;
  const response = await safeFetch(url, {
    headers: {
      "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64)",
    },
  }, 10000);

  const html = await response.text();
  const results: { title: string; link: string }[] = [];
  const resultRegex = /<a class="result__url" href="([^"]+)">([^<]+)<\/a>/g;

  let match;
  while ((match = resultRegex.exec(html)) !== null && results.length < 10) {
    let link = match[1];
    if (link.startsWith("//duckduckgo.com/l/?uddg=")) {
      try {
        link = decodeURIComponent(link.split("uddg=")[1].split("&")[0]);
      } catch {}
    }
    results.push({ link, title: match[2].trim() });
  }

  if (results.length === 0) {
    return `Không tìm thấy kết quả nào trên web cho "${query}".`;
  }

  let output = `[Web Search — DuckDuckGo Fallback] Top ${results.length} bài viết cho "${query}":\n`;
  results.forEach((r, i) => {
    output += `${i + 1}. ${r.title}\n   Link: ${r.link}\n`;
  });
  return output;
}

// Primary: Tavily API (LLM-optimized, structured Markdown)
async function searchTavily(query: string): Promise<string> {
  const response = await safeFetch(TAVILY_ENDPOINT, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      api_key: TAVILY_API_KEY,
      query,
      search_depth: "basic",
      include_answer: true,
      include_raw_content: false,
      max_results: 5,
    }),
  }, 15000);

  const data = await response.json() as {
    answer?: string;
    results?: Array<{
      title: string;
      url: string;
      content: string;
      score: number;
    }>;
  };

  let output = `[Web Search — Tavily] Kết quả cho "${query}":\n\n`;

  // Tavily's AI-generated answer summary
  if (data.answer) {
    output += `📝 Tóm tắt AI: ${data.answer}\n\n`;
  }

  // Individual results
  if (data.results && data.results.length > 0) {
    output += `📰 Top ${data.results.length} nguồn:\n`;
    for (let i = 0; i < data.results.length; i++) {
      const r = data.results[i];
      output += `${i + 1}. ${r.title}\n`;
      output += `   ${r.content.substring(0, 200)}\n`;
      output += `   🔗 ${r.url}\n\n`;
    }
  }

  return output;
}

export const execute = async (args: { query: string }): Promise<string> => {
  try {
    logger.info(
      `[Skill: web_search] Đang tìm kiếm từ khóa: "${args.query}"`,
    );

    // Use Tavily if API key is configured, otherwise fallback to DuckDuckGo
    if (TAVILY_API_KEY) {
      return await searchTavily(args.query);
    } else {
      logger.warn("[web_search] TAVILY_API_KEY not set, using DuckDuckGo fallback (less reliable).");
      return await fallbackDuckDuckGo(args.query);
    }
  } catch (error: any) {
    // If Tavily fails, try DuckDuckGo as last resort
    if (TAVILY_API_KEY) {
      logger.warn(`[web_search] Tavily failed: ${error.message}. Trying DuckDuckGo fallback...`);
      try {
        return await fallbackDuckDuckGo(args.query);
      } catch (fallbackError: any) {
        return `Lỗi tìm kiếm (tất cả nguồn đều thất bại): Tavily: ${error.message} | DDG: ${fallbackError.message}`;
      }
    }
    return `Lỗi tìm kiếm (Search error): ${error.message}`;
  }
};
