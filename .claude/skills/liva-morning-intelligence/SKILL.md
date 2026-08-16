---
name: liva-morning-intelligence
description: Scrape real-time multi-platform web intelligence (GitHub, Twitter/X, Reddit, YouTube, RSS, News) and synthesize personalized morning briefings based on Agent-Reach patterns. Use when scheduling morning digests, fetching real-time topic updates, crawling community trends, or dispatching daily briefings to Telegram and Obsidian.
---

# LIVA Morning Intelligence

## Workflow

1. **Load User Interests & Topics**:
   - Query user focus topics from `data/liva-config.json` (`system.digestInterestsTopics` / `system.digestFocusTopics`) and Obsidian user profile notes.
   - Topics span Tech/AI trends, GitHub repositories, financial/market updates, research papers, and curated news.

2. **Real-time Multi-Platform Scraping (Agent-Reach Engine)**:
   - Execute zero-fee lightweight scrapers and public search collectors inspired by `Agent-Reach`:
     - **GitHub Trending & Releases**: Top starred repositories, release notes, and breakthrough tools.
     - **Developer & Tech Communities**: Reddit (e.g., r/LocalLLaMA, r/Rust, r/MachineLearning), Twitter/X tech feeds, Hacker News.
     - **Multimedia & Publications**: YouTube tech transcripts, arXiv papers, and curated RSS feeds.
   - Filter out duplicate posts, clickbait, and irrelevant advertisements.

3. **Synthesis & Deep Briefing Generation**:
   - Structure intelligence into a high-signal executive briefing:
     - ⚡ **Top 3 Breaking Headlines**: Core developments with source citations.
     - 💻 **Tech & Open-Source Highlights**: New repos, libraries, and architectural paradigms.
     - 📊 **Domain-Specific Insights**: Deep-dive summaries customized to user interests.
     - 🎯 **Key Takeaways & Action Points**: Suggested follow-ups or tools to experiment with.

4. **Multi-Channel Dispatch**:
   - **Vault Archival**: Write full markdown briefing to `teamwork_projects/obsidian_llm_wiki/vault/Knowledge/Daily_Briefings/YYYY-MM-DD.md` via `liva-pkm-obsidian`.
   - **Telegram Push**: Deliver formatted markdown summary to the user's Telegram DM via LIVA's `telegram:send_text` at the configured morning trigger (default 07:00 AM).
   - **Voice Briefing**: Provide a concise 60-second summary script ready for TTS speech synthesis on morning startup.

## Stop Conditions

Stop and report when:
- Network scraping encounters persistent captive portals or unresolvable anti-bot blocks without fallback.
- No user interest topics are configured, prompting the user to define focus areas.
- Configured delivery channels (Telegram Bot Token / Obsidian Vault) are inaccessible.
