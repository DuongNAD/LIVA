# LIVA Skill Development Guide

This guide describes how to build, validate, secure, test, and maintain `AgentSkill` components (MCP tools) inside the LIVA gateway ecosystem.

---

## 1. Structure of an `AgentSkill`

Every skill in LIVA is a self-contained module located under `src/skills/` (categorized under folders like `core/`, `data/`, `devops/`, `web/`, etc.). 

A skill module must export two main components:
1. **`metadata`**: Declares the tool schema, category, description, and security/performance requirements.
2. **`execute`**: An asynchronous function containing the core tool logic.

### Skill Metadata Schema

The metadata is defined as a plain JavaScript object:

```typescript
export interface SkillMetadata {
  name: string;               // Unique name (lowercase, alphanumeric, with underscores)
  category?: SkillCategory;   // Core, web, devops, data, docs, personal, social, agentic
  short_desc?: string;        // Extremely short description (max 80 chars) for filtered routing
  description: string;        // Full description of what the tool does (used by LLM attention)
  search_keywords?: string[]; // Keywords used for fallback keyword-matching search
  requires_hitl?: boolean;    // Security flag: requires Human-In-The-Loop approval before execution
  is_cpu_heavy?: boolean;     // Performance flag: alerts that this blocks the event loop
  isCoreSkill?: boolean;      // Marks system-critical skills that are always loaded
  kit?: string;               // Dynamic gating kit name (e.g. DEVOPS_KIT)
  parameters: {               // JSON Schema for tool inputs
    type: "object";
    properties: Record<string, { type: string; description: string; enum?: string[] }>;
    required: string[];
  };
}
```

### Complete Code Template

Below is a standard template for writing a new skill (`src/skills/core/MyCustomSkill.ts`):

```typescript
import { z } from "zod";
import { logger } from "@utils/logger";
import { safeFetch } from "@utils/HttpClient";
import { HITLGuard } from "@security/HITLGuard";

// 1. Zod Schema matching the parameters JSON schema for runtime strict check
const MyCustomSkillSchema = z.object({
  action: z.enum(["retrieve", "modify"]),
  recordId: z.string().min(1, "recordId must not be empty"),
  newValue: z.string().optional(),
});

type MyCustomSkillArgs = z.infer<typeof MyCustomSkillSchema>;

// 2. Metadata Definition exported as 'metadata'
export const metadata = {
  name: "my_custom_skill",
  category: "core",
  short_desc: "Short summary of what custom skill does.",
  description: "Detailed prompt instructions telling the LLM when and how to call this tool.",
  search_keywords: ["custom", "retrieval", "modify"],
  requires_hitl: true, // Requires user authorization for modification actions
  parameters: {
    type: "object",
    properties: {
      action: {
        type: "string",
        enum: ["retrieve", "modify"],
        description: "The action to perform: 'retrieve' read-only or 'modify' state-changing.",
      },
      recordId: {
        type: "string",
        description: "The unique identifier of the target record.",
      },
      newValue: {
        type: "string",
        description: "The new value to set (required only for 'modify' action).",
      },
    },
    required: ["action", "recordId"],
  },
};

// 3. Execution Logic exported as 'execute'
export const execute = async (args: any): Promise<string> => {
  try {
    // Parameter Validation
    const parsedArgs = MyCustomSkillSchema.parse(args);
    logger.info({ parsedArgs }, `[Skill: my_custom_skill] Starting execution...`);

    const API_URL = process.env.CUSTOM_API_URL || "https://api.example.com/records";

    // Human-In-The-Loop (HITL) Gate for write/state-changing action
    if (parsedArgs.action === "modify") {
      logger.warn(`[Skill: my_custom_skill] 'modify' action requested. Prompting user for approval.`);
      try {
        await HITLGuard.requestApproval({
          toolName: "my_custom_skill",
          args: parsedArgs,
          reason: `LIVA wants to modify record "${parsedArgs.recordId}" to "${parsedArgs.newValue}".`,
        });
      } catch (hitlError: unknown) {
        const errMsg = hitlError instanceof Error ? hitlError.message : String(hitlError);
        return `[BLOCKED] Action declined by user: ${errMsg}`;
      }
    }

    // Secure Network Request
    if (parsedArgs.action === "retrieve") {
      const res = await safeFetch(`${API_URL}/${parsedArgs.recordId}`, { method: "GET" }, 5000);
      const data = await res.json();
      return `[SUCCESS] Retrieved record: ${JSON.stringify(data)}`;
    } else {
      // modify path
      const res = await safeFetch(`${API_URL}/${parsedArgs.recordId}`, {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ value: parsedArgs.newValue }),
      }, 5000);
      const data = await res.json();
      return `[SUCCESS] Modified record: ${JSON.stringify(data)}`;
    }

  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    logger.error(`[Skill: my_custom_skill] Execution failed: ${errMsg}`);
    
    // Provide Mock Fallback during tests or when API is offline
    if (process.env.NODE_ENV === "test" || !process.env.CUSTOM_API_URL) {
      logger.warn(`[Skill: my_custom_skill] API URL missing or in test mode. Returning mock fallback.`);
      return `[SUCCESS] (MOCK) Executed action "${args.action}" on record "${args.recordId}"`;
    }

    if (error instanceof z.ZodError) {
      return `[ERROR] Invalid parameters: ${error.issues.map(e => `${e.path.join(".")}: ${e.message}`).join(", ")}`;
    }
    return `[ERROR] System failed: ${errMsg}`;
  }
};
```

---

## 2. Validation using Zod

LIVA enforces a **Double-Gate Validation** system for local skills to ensure parameter security and catch malformed schema issues immediately:

### Gate 1: Load-time Metadata Validation
When the gateway starts up or reloads a skill, `LocalMCPServer` runs `validateSkillMetadata` (located in `src/mcp/SkillMetadataSchema.ts`):
* Validates that the skill's name matches `/^[a-z][a-z0-9_]*$/`.
* Enforces that the description is at least 5 characters.
* Rejects any skill metadata exceeding 50KB to avoid memory overflow.
* Prevents malformed skills from being loaded into memory.

### Gate 2: Execution Argument Validation
Every skill execution goes through strict parameter validation inside the MCP server before `execute` is called:
1. `LocalMCPServer` dynamically compiles the raw JSON schema defined in `metadata.parameters` into a strict runtime Zod schema using the internal `compileZodSchema` helper.
2. It parses the incoming payload against the schema, applying strict object constraints:
   ```typescript
   const compiledSchema = compileZodSchema(skill.parameters);
   const validatedArgs = compiledSchema.parse(rawArgs); // Throws on unknown properties (strict) or missing fields
   ```
3. Inside the skill's own `execute` function, developers must also define a local static Zod schema (e.g. `MyCustomSkillSchema.parse(args)`) to ensure type safety and schema consistency during unit tests where `execute` might be called directly without going through the MCP server framework.

---

## 3. Secure and Robust API Usage

To protect the Event Loop and maintain standard distributed tracing, direct node networking libraries are banned in production code.

### 🚫 Banned Libraries
* **Do NOT use `fetch` directly**: Native fetch does not throw on HTTP `4xx` or `5xx` errors. It resolves successfully, leading to silent failures.
* **Do NOT use `axios`**: Axious has been completely replaced across the codebase due to performance overhead and bundle footprint.
* **Do NOT use `console.log` or `console.error`**: Standard outputs block the single-threaded Event Loop and pollute STDOUT which is reserved strictly for Tauri IPC handshake packets.

### ✅ safeFetch Utility
Always use `safeFetch` from `@utils/HttpClient`:
* **HTTP Error Handling**: Automatically throws an error if the response status code is not 2xx.
* **Leak-Free Timeouts**: Aborts requests taking too long (default 5000ms) and guarantees timer cleanup in `finally` blocks.
* **Distributed Tracing**: Automatically injects the context trace ID (`X-Trace-Id`) into headers for log correlation.

```typescript
import { safeFetch } from "@utils/HttpClient";

// Good usage
const res = await safeFetch("https://api.example.com/data", {
  method: "POST",
  body: JSON.stringify({ key: "value" })
}, 5000);
```

### ✅ Pino Logger
Always import `logger` from `@utils/logger` for system logs:
* Logs are written asynchronously through worker thread transports, preventing main-thread lag.
* Console logging redirects automatically to `stderr` in development, preventing Tauri IPC STDOUT corruption.

```typescript
import { logger } from "@utils/logger";

logger.info({ userId: "123" }, "User profile retrieved successfully");
logger.error(err, "Failed to connect to database");
```

---

## 4. Human-In-The-Loop Approval (HITL)

For security hardening, LIVA implements a zero-trust model for write operations. Any action that alters state (creates files, modifies database entities, sends messages, updates issues, etc.) **must** request user approval.

### Code Implementation
Call `HITLGuard.requestApproval` inside your skill:

```typescript
import { HITLGuard } from "@security/HITLGuard";

try {
  const approved = await HITLGuard.requestApproval({
    toolName: "name_of_your_tool",
    args: parsedArgs,
    reason: "Why LIVA is requesting this write/state-changing action."
  });
  
  if (!approved) {
    return "[BLOCKED] User rejected request.";
  }
} catch (e: unknown) {
  // Catch user rejection (REJECTED_BY_USER) or timeout (REJECTED_BY_TIMEOUT)
  const errMsg = e instanceof Error ? e.message : String(e);
  return `[BLOCKED] Action failed: ${errMsg}`;
}
```

### How HITL Works Under the Hood
1. `HITLGuard.requestApproval` generates a unique approval ID (`hitl-xxx`).
2. It registers a pending approval with a **300-second timeout** to avoid hanging deadlock.
3. It emits a `hitl_request` event via `EventEmitter`. The Gateway forwards this event to the WebSocket channel (`ui` or remote adapters like `telegram`, `zalo`).
4. **Zalo/Telegram Adapters**: Automatically prompt the user with inline buttons ("Approve", "Reject") or request a simple chat reply ("yes", "no").
5. **Tauri UI**: Dynamically injects an approval modal or speech bubble directly into the chat interface.
6. Once the user clicks "Approve" or responds, `HITLGuard.respond(id, true/false)` resolves the pending promise, allowing the skill to proceed.

---

## 5. Testing and Mocking Conventions

All unit and integration tests are written using **Vitest**. Network requests and credentials must be mocked to ensure that tests run locally, fast, and in a deterministic environment.

### Network Mocking
Mock `safeFetch` by intercepting `@utils/HttpClient` module imports:

```typescript
import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock safeFetch
const mockSafeFetch = vi.fn();
vi.mock("../../src/utils/HttpClient", () => ({
  safeFetch: (...args: any[]) => mockSafeFetch(...args),
}));

import * as GetWeather from "../../src/skills/core/GetWeather";

describe("Weather Skill", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should parse weather forecast data successfully", async () => {
    mockSafeFetch.mockResolvedValueOnce({
      json: async () => ({
        current: { temperature_2m: 25, relative_humidity_2m: 60, weather_code: 0 }
      })
    });

    const result = await GetWeather.execute({ location: "Hanoi" });
    expect(result).toContain("25");
    expect(result).toContain("Clear sky");
  });
});
```

### Mocking Credentials & Environmental Variable Fallbacks
Skills should not fail if credentials (like API keys) are missing during local test suites or when the app is freshly installed. Ensure your skill has a test fallback check:

```typescript
const apiKey = process.env.MY_SERVICE_API_KEY;

if (process.env.NODE_ENV === "test" || !apiKey) {
  // Safe local mocking/fallback mode
  logger.warn("Credentials missing or in test mode. Falling back to mock data.");
  return `[SUCCESS] (MOCK) Mocked payload returned.`;
}
```

---

## 6. Hot-Reloading System Overview

LIVA features a **Sequential Hot-Swap File Watcher** to enable seamless, live updates to skills without rebooting the main Gateway server.

```plaintext
┌─────────────────┐       chokidar      ┌─────────────────┐       reloadLocalSkill()      ┌──────────────────┐
│   src/skills/   │ ──────────────────> │   CoreKernel    │ ────────────────────────────> │  SkillRegistry   │
│  File Mutation  │                     │  File Watcher   │                               └──────────────────┘
└─────────────────┘                     └─────────────────┘                                         │
                                                                                                    ▼
┌─────────────────┐                     ┌─────────────────┐       Dynamic Import URL        ┌──────────────────┐
│   Client/LLM    │ <────────────────── │  LocalMCPServer │ <────────────────────────────── │  Import Module   │
│ ToolListChanged │   sendToolList..    │  (In-Memory DB) │    pathToFileURL() + '?v=...'   │  (Bust Cache)    │
└─────────────────┘                     └─────────────────┘                                 └──────────────────┘
```

### 1. File Monitoring System
* `CoreKernel.#watchSkillMutations` uses `chokidar` to recursively watch changes under the `src/skills/` directory.
* Ignored files include files starting with `.`, internal classes (`SkillMetadata.ts`, `BaseSkill.ts`), and unit tests (`*.test.ts`).
* The system buffers mutations using a **1-second debounce** timer to avoid thrashed reloads when saving a file multiple times.

### 2. The Hot-Swap Flow
When a file is added, modified, or unlinked, the watcher triggers `SkillRegistry.reloadLocalSkill(filePath, event)`:
1. **Unlink Event**: If the file was deleted, the skill name is removed from the memory registry map.
2. **Add / Change Event**: The file path is converted to a file URL. The system attaches a dynamic cache-busting parameter (`?v=timestamp`) to force the V8 Engine to reload the script instead of pulling it from the module cache:
   ```typescript
   const fileUrl = pathToFileURL(normalizedPath).href + `?v=${Date.now()}`;
   const module = await import(fileUrl);
   ```
3. **Validation**: The newly loaded module's metadata is re-validated through Zod (`validateSkillMetadata`). If valid, the execution handler is bound dynamically to the memory cache.
4. **Clean cache & semantic re-index**: The Skill Registry clears `descEmbeddingCache` and triggers `warmUpCache()` in the background to recalculate semantic embeddings for the updated tool description.
5. **Sync clients**: The MCP server calls `this.server.sendToolListChanged()` to broadcast the updated tool catalog to all connected LLM clients.
