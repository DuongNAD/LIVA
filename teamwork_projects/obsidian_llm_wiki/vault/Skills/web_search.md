---
title: "web_search"
tags:
  - liva/skill
  - web-search
author: "Explorer Agent"
last_update: "2026-06-21T01:17:06Z"
status: "active"
inputs:
  - name: "query"
    type: "string"
    description: "Search engine query string"
outputs:
  - name: "results"
    type: "array"
    description: "List of matching web page titles, snippets, and URLs"
associated_tools:
  - search_web
---

# Skill: Web Search Capability

## Description
Enables LIVA to query external search engines to find up-to-date documentation, research, and technical guides.

## Prerequisites & Setup
- API keys configured for the search provider in `.env` (e.g., `SEARCH_API_KEY`).

## Execution Flow / Steps
1. Parse the user request to identify search terms.
2. Call the `search_web` tool with the query.
3. Filter and parse the results, selecting the top 3-5 pages for deep inspection.

## Usage Examples
```javascript
// Call search_web tool
const results = await mcp.callTool('search_web', { query: 'Model Context Protocol spec' });
```

## Error Handling
- **Error**: Rate limit exceeded (429).
  - **Resolution**: Wait and retry with exponential backoff.

## Verification Method
Run a test query via the CLI:
`node scripts/test-search.js "test query"`
Verify that a list of results is returned without errors.
