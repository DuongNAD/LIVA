/**
 * WebResearchAgent: Internet-Augmented Evolution Intelligence
 * ============================================================
 * Gives the Darwinian evolution loop access to web knowledge:
 * 
 * 1. ERROR RESEARCH: When AST diagnostics fail, search for the error message
 *    to find StackOverflow/GitHub solutions
 * 2. GOAL RESEARCH: Before generating mutations, search for best practices
 *    related to the evolution goal
 * 3. API DOCS: Look up npm package documentation when mutations involve
 *    external libraries
 * 
 * Uses DuckDuckGo HTML search (same as WebSearch skill) — no API key needed.
 */

import { execute as webSearch } from "../skills/WebSearch.js";

const MAX_RESEARCH_RESULTS = 3;
const RESEARCH_TIMEOUT_MS = 10_000;

export interface ResearchContext {
    goalInsights: string;
    errorFixes: string;
    totalQueries: number;
    totalResults: number;
}

/**
 * Research the evolution goal before code generation.
 * Returns distilled insights to inject into the coder's prompt.
 */
export async function researchGoal(goal: string): Promise<string> {
    try {
        // Build a focused technical search query
        const query = buildTechnicalQuery(goal);
        console.log(`[WebResearch] 🌐 Searching: "${query}"`);

        const rawResult = await Promise.race([
            webSearch({ query }),
            timeout(RESEARCH_TIMEOUT_MS, "[WebResearch] Search timed out"),
        ]) as string;

        if (!rawResult || rawResult.includes("Không tìm thấy")) {
            return "";
        }

        // Distill to concise context for the LLM
        return distillSearchResults(rawResult, MAX_RESEARCH_RESULTS);
    } catch (e: any) {
        console.warn(`[WebResearch] Goal research failed (non-fatal): ${e.message}`);
        return "";
    }
}

/**
 * Research specific error messages from AST diagnostics.
 * Called after a failed compile to find fixes.
 */
export async function researchErrors(asiReport: string): Promise<string> {
    try {
        // Extract unique error codes/messages from ASI report
        const errorQueries = extractErrorQueries(asiReport);
        if (errorQueries.length === 0) return "";

        console.log(`[WebResearch] 🔍 Researching ${errorQueries.length} error(s)...`);

        const results: string[] = [];
        // Search up to 2 errors to avoid rate limiting
        for (const query of errorQueries.slice(0, 2)) {
            try {
                const rawResult = await Promise.race([
                    webSearch({ query: `TypeScript ${query} fix site:stackoverflow.com OR site:github.com` }),
                    timeout(RESEARCH_TIMEOUT_MS, "timeout"),
                ]) as string;

                if (rawResult && !rawResult.includes("Không tìm thấy")) {
                    results.push(distillSearchResults(rawResult, 2));
                }
            } catch {
                // Non-fatal, continue with other errors
            }
        }

        if (results.length === 0) return "";

        return `\n<web_research>\n  <error_research>\n    ${results.join("\n    ")}\n  </error_research>\n</web_research>`;

    } catch (e: any) {
        console.warn(`[WebResearch] Error research failed (non-fatal): ${e.message}`);
        return "";
    }
}

/**
 * Full research pipeline: goal + optional error context.
 * Returns combined XML context block for injection into the coder prompt.
 */
export async function fullResearch(
    goal: string, 
    previousErrors?: string
): Promise<ResearchContext> {
    let totalQueries = 0;
    let totalResults = 0;

    // Goal research
    const goalInsights = await researchGoal(goal);
    if (goalInsights) totalQueries++;

    // Error research (only if previous cycle had failures)
    let errorFixes = "";
    if (previousErrors) {
        errorFixes = await researchErrors(previousErrors);
        if (errorFixes) totalQueries++;
    }

    if (goalInsights) totalResults++;
    if (errorFixes) totalResults++;

    return { goalInsights, errorFixes, totalQueries, totalResults };
}

// =============================================================================
// Helper Functions
// =============================================================================

/**
 * Transform a high-level goal into a focused technical search query.
 */
function buildTechnicalQuery(goal: string): string {
    // Strip Vietnamese and keep technical terms
    let query = goal
        .replaceAll(/[^\w\s\-\.\/#]/g, " ")  // Remove non-ASCII
        .replaceAll(/\s+/g, " ")
        .trim();

    // If goal is mostly non-English, use it as-is (DuckDuckGo handles Vi)
    if (query.length < 10) {
        query = goal.slice(0, 100);
    }

    // Add TypeScript context
    if (!query.toLowerCase().includes("typescript") && !query.toLowerCase().includes("ts")) {
        query = `TypeScript ${query}`;
    }

    // Cap query length
    return query.slice(0, 150);
}

/**
 * Extract searchable error messages from ASI diagnostic report.
 */
function extractErrorQueries(asiReport: string): string[] {
    const queries: string[] = [];
    const seen = new Set<string>();

    // Match TypeScript error codes (TS2304, TS1005, etc.)
    const tsErrorMatches = asiReport.matchAll(/TS(\d{4})[:\s]+([^\n]{10,80})/g);
    for (const match of tsErrorMatches) {
        const errorCode = `TS${match[1]}`;
        if (!seen.has(errorCode)) {
            seen.add(errorCode);
            queries.push(`${errorCode} ${match[2].trim().slice(0, 60)}`);
        }
    }

    // Match generic error messages
    if (queries.length === 0) {
        const genericErrors = asiReport.matchAll(/(?:error|Error|lỗi)[:\s]+([^\n]{15,100})/g);
        for (const match of genericErrors) {
            const msg = match[1].trim().slice(0, 80);
            if (!seen.has(msg)) {
                seen.add(msg);
                queries.push(msg);
            }
        }
    }

    return queries;
}

/**
 * Distill raw search results to concise, useful context.
 */
function distillSearchResults(raw: string, maxItems: number): string {
    const lines = raw.split("\n").filter(l => l.trim());
    const items: string[] = [];

    for (const line of lines) {
        if (line.match(/^\d+\.\s/)) {
            items.push(line.trim());
            if (items.length >= maxItems) break;
        }
    }

    return items.join("\n");
}

/**
 * Promise-based timeout helper.
 */
function timeout(ms: number, msg: string): Promise<never> {
    return new Promise((_, reject) => {
        setTimeout(() => reject(new Error(msg)), ms);
    });
}
