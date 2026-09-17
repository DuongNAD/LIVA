---
name: liva-deep-research
description: Conduct autonomous multi-source web research, crawling, evidence extraction, and structured synthesis. Use when investigating technical topics, aggregating news across developer communities, discovering scientific papers, verifying claims with citation graphs, or generating deep research reports for Obsidian and Telegram.
---

# LIVA Deep Research

## Workflow

1. **Research Scope Formulation & Query Decomposition**:
   - Ingest user research question or topic objective.
   - Decompose into primary search vectors, alternative keyword permutations, and domain-specific filters (e.g., GitHub repos, arXiv preprints, technical RFCs, forum discussions).

2. **Multi-Provider Search & Ingestion (Agent-Reach Architecture)**:
   - Dispatch queries in parallel across configured search endpoints (SearXNG, Brave Search, Tavily, OpenAlex, arXiv API, Reddit/Hacker News collectors).
   - Apply rate-limiting backoffs, handle anti-bot captchas via failover providers, and cache raw search results locally in SQLite WAL (`research_cache`).

3. **Content Extraction & Cleaning**:
   - Fetch target URLs using lightweight, zero-overhead HTTP streaming clients (`reqwest`).
   - Strip boilerplate, ads, cookie banners, navigation links, and tracking scripts, converting content into structured markdown with intact headers, code blocks, and tables.

4. **Evidence Synthesis & Contradiction Analysis**:
   - Extract key factual claims, release dates, benchmark metrics, and architectural patterns.
   - Cross-reference claims across multiple sources:
     - Assign source credibility weights (Official documentation / Peer-reviewed > Engineering blogs > Social media).
     - Flag and explicitly highlight contradictory statements or unresolved ambiguities.

5. **Dossier Compilation & Citation Graphing**:
   - Assemble comprehensive research dossier:
     - 📌 **Executive Summary**: Core findings in 3 bullet points.
     - 🔬 **Detailed Analysis**: Structured technical breakdown with verbatim code/data snippets.
     - ⚖️ **Trade-offs & Comparison Table**: Feature/performance matrix.
     - 📚 **Citation Index**: Full list of referenced URLs with retrieval timestamps.

6. **Obsidian Archival & Multi-Channel Delivery**:
   - Persist complete research report into `teamwork_projects/obsidian_llm_wiki/vault/Knowledge/Research - <Topic_Title>.md` via `write_markdown`.
   - Adhere strictly to the Obsidian frontmatter standard (`title`, `tags: [liva/knowledge, liva/research, web/intelligence]`, `author: "codex"`, `last_update`).
   - Generate a 2-minute concise digest formatted for Telegram or voice briefing.

## Platform Constraints

- **Execution Mode**: Autonomous research and synthesis. Web scraping uses public read-only endpoints without credential exfiltration.
- **Network Compliance**: All outbound requests must respect `vault/Rules/network_restrictions.md` (no unapproved external telemetry; search queries route through verified API providers).
- **Data Privacy**: Never include private keys, passwords, or PII extracted from crawled pages in research dossiers.

## Stop Conditions

Stop and report immediately when:
- All search providers fail simultaneously or encounter permanent IP bans/captive portals.
- The research query requests private, copyrighted, or unauthorized internal corporate data.
- Search results return zero relevant sources after 3 iterative query reformulations.
