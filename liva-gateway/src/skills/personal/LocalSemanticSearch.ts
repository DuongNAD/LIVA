import { EmbeddingService } from "../../services/EmbeddingService";
import { logger } from "@utils/logger";
import { SkillMetadata } from "../SkillMetadata";

export const metadata: SkillMetadata = {
  name: "local_semantic_search",
  category: "personal",
  short_desc: "Semantic search over local memory.",
  semantic_tags: ["#search", "#rag", "#find", "#memory", "#semantic"],
  search_keywords: ["search", "rag", "find", "memory", "semantic", "tìm kiếm", "tra cứu", "trí nhớ"],
  description: "Perform a combined semantic and keyword hybrid search over all conversation histories, events, and facts stored in LIVA's long-term memory.",
  parameters: {
    type: "object",
    properties: {
      query: {
        type: "string",
        description: "The search query string in natural language (e.g. 'what is the user's wife's name' or 'meeting schedules')."
      },
      limit: {
        type: "number",
        description: "Maximum number of search results to return. Default is 5."
      },
      type_filter: {
        type: "string",
        description: "Filter matches by specific record type (e.g. 'CONVERSATION', 'FACT', 'ANCHOR', 'AXIOM')."
      }
    },
    required: ["query"]
  }
};

export const execute = async (args: {
  query: string;
  limit?: number;
  type_filter?: string;
}): Promise<string> => {
  const query = args.query?.trim();
  const limit = args.limit || 5;
  const typeFilter = args.type_filter?.trim();

  if (!query) {
    return "Error: Please provide a valid 'query'.";
  }

  logger.info(`[Skill: local_semantic_search] Searching for: "${query}" (limit: ${limit}, filter: ${typeFilter || "none"})`);

  const kernel = (globalThis as any).kernelInstance;
  const memory = kernel?.memory;
  const sm = memory?.getStructuredMemoryInstance();
  const dbBridge = sm?.dbBridge;

  if (!sm || !dbBridge) {
    logger.warn("[Skill: local_semantic_search] Memory database not ready. Returning mock results.");
    return `### 🔍 Local Semantic Search Results (Mock Mode)\n\n*Memory database is currently offline or uninitialized. Here is a simulated result:*\n\n| Score | Source Type | Category | Content Preview |\n|---|---|---|---|\n| 0.92 | CONVERSATION | general | "Sếp nói: nhớ nhắc anh gọi điện thoại cho Dương lúc 3h chiều" |\n| 0.85 | FACT | preference | "Sếp thích uống trà nhài và làm việc trong phòng tối" |`;
  }

  try {
    const embedSvc = EmbeddingService.getInstance();
    let queryVector: number[] = [];
    let isSemanticReady = false;

    if (embedSvc.ready) {
      try {
        queryVector = await embedSvc.embedWithTimeout(query, 3000);
        isSemanticReady = true;
      } catch (err) {
        logger.warn(`[Skill: local_semantic_search] Embedding failed: ${(err as Error).message}. Falling back to FTS5 keyword-only search.`);
      }
    } else {
      logger.info("[Skill: local_semantic_search] EmbeddingService not ready. Falling back to FTS5 keyword-only search.");
    }

    let rawResults: any[] = [];
    let searchType = "";

    if (isSemanticReady && queryVector.length > 0) {
      searchType = "Hybrid (Semantic + Keyword)";
      // Call StructuredMemory searchHybridVectors
      rawResults = await sm.searchHybridVectors(
        query,
        queryVector,
        limit,
        typeFilter
      );
    } else {
      searchType = "FTS5 Keyword-Only";
      // Escape double quotes and split search query into Porter tokens for FTS5 prefix match
      const escapedQuery = query.replace(/"/g, '""');
      const cleanQuery = escapedQuery.trim().split(/\s+/).filter(Boolean).map(word => `"${word}"*`).join(" AND ");
      
      let metaConditions = "1=1";
      const metaParams: any[] = [];
      if (typeFilter) {
        metaConditions += " AND m.type = ?";
        metaParams.push(typeFilter);
      }

      try {
        rawResults = await dbBridge.all(`
          SELECT m.vec_id, m.content, m.type, m.domain, m.category, m.trace_keywords
          FROM vectors_fts f
          INNER JOIN vectors_meta m ON m.id = f.rowid
          WHERE f.content MATCH ? AND ${metaConditions}
          LIMIT ?
        `, [cleanQuery, ...metaParams, limit]);
      } catch (err) {
        logger.error(`[Skill: local_semantic_search] FTS5 search failed: ${(err as Error).message}`);
        rawResults = [];
      }
    }

    if (rawResults.length === 0) {
      return `### 🔍 Local Memory Search (Search Mode: ${searchType})\n\nNo matching records found for query: *"${query}"*`;
    }

    // Format results as markdown table
    let markdownTable = `### 🔍 Local Memory Search (Search Mode: ${searchType})\n\n`;
    markdownTable += `| Score | Type | Category (Domain) | Matching Memory Content |\n`;
    markdownTable += `| :---: | :---: | :---: | :--- |\n`;

    for (const r of rawResults) {
      // Score calculation
      const scoreStr = r.score !== undefined ? r.score.toFixed(3) : "FTS Match";
      const typeStr = r.type || "N/A";
      const categoryStr = `${r.category || "Uncategorized"} (${r.domain || "General"})`;
      
      // Escape pipe character in content to avoid breaking markdown table formatting
      const cleanContent = (r.content || "").replace(/\|/g, "\\|").replace(/\n/g, " ");

      markdownTable += `| **${scoreStr}** | \`${typeStr}\` | *${categoryStr}* | ${cleanContent} |\n`;
    }

    return markdownTable;

  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    logger.error(`[Skill: local_semantic_search] Failed: ${errMsg}`);
    return `Error performing memory search: ${errMsg}`;
  }
};
