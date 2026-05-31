---
name: feed-scraper
version: 2.0.0
description: Core guidelines and diagnostic protocols for Antigravity-Vibe's 4-stage extraction pipeline, stealth browser evasions, ranked selector discovery, and automated favicon probing.
triggers: ["scraper", "extraction", "playwright readability", "stealth browser", "favicon discovery"]
dependencies: ["playwright-extra", "turndown", "gemini-sdk", "mcp:playwright", "mcp:firecrawl"]
---

# Feed Scraper, Content Extraction & Discovery Systems

Antigravity-Vibe relies on a highly sophisticated extraction pipeline with multiple layers of redundancy, stealth evasions, and automated discovery tools to deliver clean content.

---

## 1. 4-Stage Content Extraction & AI Enrichment Pipeline

Content extraction is fully automated and manages failure gracefully using a resilient four-stage cascade:

```
┌──────────────────────────────────────┐
│  Stage 1: Playwright + Readability   │ ──► [ContentFastExtractionService]
└──────────────────────────────────────┘
                   │
         (Fails or < 200 chars?)
                   ▼
┌──────────────────────────────────────┐
│  Stage 2: Firecrawl /scrape API     │ ──► [ContentVerboseExtractionService] (Fallback)
└──────────────────────────────────────┘
                   │
                   ▼
┌──────────────────────────────────────┐
│  Stage 3: Gemini Semantic Structuring│ ──► [ContentAiAssistedExtractionService]
└──────────────────────────────────────┘
                   │
                   ▼
┌──────────────────────────────────────┐
│  Stage 4: Language-Agnostic Analysis │ ──► [aiService.ts] (Headline & Summary)
└──────────────────────────────────────┘
```

### Stage 1: Playwright + Readability + Turndown (Fast)
* **Execution**: `ContentFastExtractionService.ts` fetches page content in a Chromium page from the centralized `BrowserPool.ts`.
* **JSON-LD**: Always extracts and parses `application/ld+json` first. If present, it determines the primary fields (headline, date, author, image, body) with top-level priority.
* **Auto Mode**: Mozilla's `Readability` engine extracts the raw main text container, which `turndown` converts to clean Markdown.
* **Custom Mode**: Evaluates specific custom CSS fields (`headline`, `body`, `date`, `byline`, `image`) directly from the DOM, overriding any Readability/AI extractions.

### Stage 2: Firecrawl Fallback (Verbose)
* **Execution**: Triggered by `articleService.ts` when the primary Stage 1 extraction fails, encounters network errors, or yields extremely thin content (< 200 characters).
* **Selector Bridging (Intellectual Property)**:
  * Comma-separated `Feed.removeSelectors` are mapped to Firecrawl's `excludeTags` array (stripping boilerplate elements before parsing).
  * Comma-separated `Feed.targetSelectors` are mapped to Firecrawl's `includeTags` array (forcing the scraper to isolate the article content container).
  * Implemented using `ContentVerboseExtractionService.scrape(url, { excludeTags, includeTags })`.

### Stage 3: Gemini Structured Semantic Extraction (AI-Assisted)
* **Execution**: `ContentAiAssistedExtractionService.ts` takes the Markdown payload and passes it to the `gemini-2.5-flash` model utilizing a strict JSON schema definition.
* **Outputs**: Returns a structured JSON payload detailing the headline, subheadline, byline, lead, body text, subheads, sidebar, and publication date.

### Stage 4: Language-Agnostic AI Analysis
* **Execution**: Runs `article_analysis_prompt.md` using `gemini-2.0-flash` to construct a 3-sentence `ai_summary` and a crisp, readable `ai_headline`.

---

## 2. Stealth Browser Evasion Engine (v2.8.17 Baseline)

To prevent Cloudflare, Akamai, and generic bot protection systems from blocking extraction requests:
* **Stealth Hook**: All scraper services import `{ chromium }` from `./extraction/stealthBrowser.ts` instead of vanilla `playwright`.
* **Augmentation**: Employs `playwright-extra` coupled with the `puppeteer-extra-plugin-stealth` middleware.
* **Mechanisms**: Performs navigator spoofing, user-agent randomizing, browser fingerprint masking, and automated WebGL/canvas override injections.
* *Note: E2E tests and debugging scripts intentionally retain vanilla `playwright` imports to avoid masking execution timings.*

---

## 3. Automated Favicon & Brand Discovery

`FaviconDiscoveryService.ts` uses a lightweight, parallelized three-step lookup sequence to resolve branding:
1. **HTML Meta-Scrape (Fastest)**: Fetches the source homepage HTML and regex-parses `<link rel="icon">`, `shortcut icon`, or `apple-touch-icon` paths.
2. **Parallel Path Probing**: Sequentially issues HTTP `HEAD` probes across **139 ranked common favicon paths** in parallel batches of 5 to verify existence.
3. **Apex Domain Fallback**: If a subdomain (e.g. `feeds.bbci.co.uk`) yields no results, the parser isolates the apex hostname (`bbci.co.uk`) and retries the entire HTML/HEAD probing process.
4. **Sentinel Fallback**: If all paths fail, the service returns the sentinel asset: `https://antigravity.vibe/No_valid_logo_or_favicon_found.webp`.

---

## 4. E2E Test Clerk OTP Interception
* **Mechanism**: During automated testing routines, E2E scripts bypass the manual email authentication step by intercepting Clerk's One-Time Password (OTP) directly via the Graph API mailbox poller (`interceptClerkOtp()`).

---

## 5. Troubleshooting & Diagnostics

| Symptom | Probable Cause | Action |
|:---|:---|:---|
| **Empty content / Cloudflare Wall** | Scraper blocked despite stealth browser. | Verify Firecrawl Fallback is active in the Feed Config Fallback Tab. |
| **Short content with Target Selector** | The defined target selector container is too narrow. | Clear the target selector and use a broader selector or rely on auto-mode Readability. |
| **Extraction queue stuck** | BullMQ queue congestion or dead Redis connection. | Inspect Redis jobs: `mcp_redis_list` with pattern `bull:extraction:*`. Reset the worker pool if needed. |
| **Wrong default scraping language** | Localized language not recognized. | Set `scraping_language` in the Feed settings. |
