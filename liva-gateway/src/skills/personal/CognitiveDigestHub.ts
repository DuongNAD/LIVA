import { safeFetch } from "@utils/HttpClient";
import { logger } from "@utils/logger";
import { cosineSimilarity } from "@utils/VectorMath";
import { EmbeddingService } from "../../services/EmbeddingService";
import { SkillMetadata } from "../SkillMetadata";

export const metadata: SkillMetadata = {
  name: "cognitive_digest_hub",
  category: "personal",
  short_desc: "Generate notifications and email digests.",
  semantic_tags: ["#digest", "#focus", "#notification", "#summary", "#unify"],
  search_keywords: ["digest", "notifications", "emails", "focus", "summary", "tóm tắt thông báo", "thư", "hộp thư"],
  description: "Generate an AI-powered consolidated summary digest of recent messages, notifications, or emails during Focus Mode or specific time windows, helping the user stay updated without distraction. Incorporates semantic deduplication.",
  parameters: {
    type: "object",
    properties: {
      action: {
        type: "string",
        enum: ["get_digest", "get_recent_events"],
        description: "Action to perform: 'get_digest' to generate a markdown summary, or 'get_recent_events' to fetch raw recent event logs."
      },
      time_window_hours: {
        type: "number",
        description: "Specify the time window in hours to review. Default is 4 hours."
      },
      urgency_filter: {
        type: "boolean",
        description: "If true, only focus the digest on high-urgency discussions or events. Default is false."
      }
    },
    required: ["action"]
  }
};

async function semanticDeduplicate(items: string[]): Promise<string[]> {
  if (items.length <= 1) return items;

  const embedSvc = EmbeddingService.getInstance();
  if (!embedSvc.ready) {
    logger.debug("[CognitiveDigestHub] EmbeddingService not ready — falling back to text deduplication.");
    return [...new Set(items)];
  }

  try {
    const embeddings = await embedSvc.embedBatch(items);
    const uniqueIndices: number[] = [];

    for (let i = 0; i < items.length; i++) {
      let isDuplicate = false;
      for (const uniqueIdx of uniqueIndices) {
        const sim = cosineSimilarity(embeddings[i], embeddings[uniqueIdx]);
        // Threshold 0.82 for grouping similar context events/sentences
        if (sim > 0.82) {
          isDuplicate = true;
          break;
        }
      }
      if (!isDuplicate) {
        uniqueIndices.push(i);
      }
    }

    logger.debug(`[CognitiveDigestHub] Semantic deduplication reduced ${items.length} items to ${uniqueIndices.length}.`);
    return uniqueIndices.map(idx => items[idx]);
  } catch (err) {
    logger.warn(`[CognitiveDigestHub] Semantic deduplication error: ${(err as Error).message}. Falling back to standard filter.`);
    return [...new Set(items)];
  }
}

export const execute = async (args: {
  action: "get_digest" | "get_recent_events";
  time_window_hours?: number;
  urgency_filter?: boolean;
}): Promise<any> => {
  const action = args.action;
  const timeWindowHours = args.time_window_hours || 4;
  const urgencyOnly = args.urgency_filter || false;

  logger.info(`[Skill: cognitive_digest_hub] Executing action '${action}' for window of ${timeWindowHours} hours.`);

  const kernel = (globalThis as any).kernelInstance;
  const memory = kernel?.memory;
  const sm = memory?.getStructuredMemoryInstance();
  const dbBridge = sm?.dbBridge;

  // Fallback if DB is not ready or executing in test/mock environment
  if (!dbBridge) {
    logger.warn("[Skill: cognitive_digest_hub] Database connection not available. Using mock mode.");
    if (action === "get_recent_events") {
      return [
        {
          eventId: "mock_ev_1",
          timestamp: Date.now() - 30 * 60000,
          rawUserMsg: "Hello Liva, remind me to check the email server later.",
          rawAiReply: "Sure! I have noted that reminder.",
          domain: "General",
          category: "Work"
        }
      ];
    }

    return `### 📬 LIVA Cognitive Digest (Mock Mode)\n\n*System warning: Database is offline. Simulated summary of recent events:*\n\n- **Focus Mode:** Enabled (active apps: VS Code, Terminal).\n- **Missed Items:** 2 Emails, 1 Zalo notification.\n- **Summary:** Sếp đang tập trung lập trình. Không có thông báo nào khẩn cấp phát hiện.`;
  }

  try {
    const now = Date.now();
    const windowMs = timeWindowHours * 60 * 60 * 1000;
    const cutoff = now - windowMs;

    // 1. Fetch conversations from Turn Layer
    const turns = await dbBridge.all(
      "SELECT userMsg, aiReply, temporal_anchor FROM turn_layer_nodes WHERE temporal_anchor >= ? ORDER BY temporal_anchor ASC",
      [cutoff]
    );

    // 2. Fetch processed events
    const events = await dbBridge.all(
      "SELECT phi_facts, psi_intent, domain, category, timestamp FROM events WHERE timestamp >= ? ORDER BY timestamp ASC",
      [cutoff]
    );

    // Filter by urgency if requested
    const filteredTurns = urgencyOnly 
      ? turns.filter((t: any) => 
          /khẩn|gấp|urgent|cháy|chết|ngay/i.test(t.userMsg || "") || 
          /khẩn|gấp|urgent/i.test(t.aiReply || "")
        )
      : turns;

    const filteredEvents = urgencyOnly
      ? events.filter((e: any) =>
          /khẩn|gấp|urgent|high/i.test(e.psi_intent || "") || 
          /General|Uncategorized/i.test(e.domain || "") === false
        )
      : events;

    if (action === "get_recent_events") {
      return {
        time_window_hours: timeWindowHours,
        total_turns: turns.length,
        total_events: events.length,
        filtered_turns: filteredTurns,
        filtered_events: filteredEvents
      };
    }

    // Check if we have any data to summarize
    if (filteredTurns.length === 0 && filteredEvents.length === 0) {
      return `### 📬 LIVA Cognitive Digest\n\nNo conversations or memory events recorded in the last **${timeWindowHours} hours**${urgencyOnly ? " matching the urgency filter" : ""}. LIVA is idling and ready.`;
    }

    // 3. Format strings for semantic deduplication
    const turnStrings = filteredTurns.map((t: any) => `User: ${t.userMsg || ""}\nAI: ${t.aiReply || ""}`);
    
    const eventStrings = filteredEvents.map((ev: any) => {
      let facts = [];
      try {
        facts = ev.phi_facts ? JSON.parse(ev.phi_facts) : [];
      } catch {
        // Non-critical
      }
      return `[${ev.domain || "General"} / ${ev.category || "Uncategorized"}] Intent: ${ev.psi_intent || "N/A"}. Facts: ${Array.isArray(facts) ? facts.join("; ") : String(facts)}`;
    });

    // 4. Perform batch semantic deduplication
    logger.debug("[CognitiveDigestHub] Starting semantic deduplication on turns and events...");
    const [deduplicatedTurns, deduplicatedEvents] = await Promise.all([
      semanticDeduplicate(turnStrings),
      semanticDeduplicate(eventStrings)
    ]);

    logger.info(`[Skill: cognitive_digest_hub] Compiling digest. Deduplicated turns: ${deduplicatedTurns.length}/${filteredTurns.length}, events: ${deduplicatedEvents.length}/${filteredEvents.length}`);

    let promptContext = "";
    if (deduplicatedTurns.length > 0) {
      promptContext += "### Recent Conversational Turns:\n";
      promptContext += deduplicatedTurns.join("\n---\n") + "\n---\n";
    }
    if (deduplicatedEvents.length > 0) {
      promptContext += "\n### Processed Memory Events:\n";
      promptContext += deduplicatedEvents.map(e => `- ${e}`).join("\n") + "\n";
    }

    const llmUrl = process.env.LLM_ENDPOINT || "http://localhost:8000/v1/chat/completions";

    const llmResponse = await safeFetch(llmUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        model: "local",
        messages: [
          {
            role: "system",
            content: `You are LIVA's Focus Assistant. Review the recent logs from the past ${timeWindowHours} hours and compile a clean, high-fidelity markdown digest.
Structure the summary as follows:
1. **Executive Digest**: A 1-2 sentence description of what the user has focused on and LIVA's active support.
2. **Key Topics Discussed**: Short bullet points.
3. **Decisions & Commitments**: Any decisions made or follow-up actions LIVA promised to take.
4. **Captured Preferences**: Any preferences or facts learned about the user during these turns.

Use professional, polite Vietnamese. Keep it extremely brief and direct.`
          },
          {
            role: "user",
            content: promptContext
          }
        ],
        temperature: 0.3,
        max_tokens: 1024
      })
    }, 45000);

    const data = await llmResponse.json();
    const digestText = data.choices?.[0]?.message?.content?.trim();

    if (!digestText) {
      return "Error: Failed to compile cognitive digest. LLM returned no output.";
    }

    return `### 📬 LIVA Cognitive Digest (${timeWindowHours}h window)\n\n${digestText}`;

  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    logger.error(`[Skill: cognitive_digest_hub] Error: ${errMsg}`);
    return `Error compiling cognitive digest: ${errMsg}`;
  }
};
