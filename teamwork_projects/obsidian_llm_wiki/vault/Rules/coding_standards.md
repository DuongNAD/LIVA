---
title: "coding_standards"
tags:
  - liva/rule
author: "worker"
last_update: "2026-06-21T02:21:19Z"
severity: "CRITICAL"
scope: "all-agents"
---

# Rule: Coding Standards

## Rule Statement
All code must adhere to LIVA standards, including Event Loop protection, TypeScript branded types, Vue 3 reactivity rules, Windows PowerShell terminal paths, and Zero-any policy.

## Rationale
- **Event Loop Protection**: Node.js is single-threaded. Blocking the event loop freezes the gateway and leads to unresponsive UI or service crashes.
- **Vue 3 Reactivity**: Deep reactive proxies on rapid streams (e.g. 60 tokens/second) overwhelm Vue's tracking and block rendering.
- **PowerShell Paths**: The developer environment is Windows 11 with PowerShell 7, requiring specific syntax for local commands.

## Detailed Coding Standards

### Event Loop Protection
- Offload any synchronous operation taking >10ms CPU-time to worker threads (e.g. AST mutations via `ts-morph` in `ASTWorker`, Nemotron/VAD inference).
- Synchronous file system calls (e.g., `fs.readFileSync`) are STRICTLY BANNED in the main Gateway event loop.

### TypeScript Conventions
- **Branded Types**: Use branded types for security-sensitive IDs.
- **Early Return**: Maximum nesting depth is 3. Flatten with guard clauses.
- **Zero any Policy**: Prohibited in production code. Use `unknown` and narrow with type guards or schemas. When parsing JSON, Zod schemas are mandatory.
- **Private Fields**: Use ECMAScript private fields (`#name`) to enforce encapsulation for security keys and process handlers.

### Vue 3 Reactivity & Zombie RAM
- **Streaming Output**: MUST use `shallowRef` + `triggerRef` instead of deep `ref` to avoid CPU overload during rapid token stream updates.
- **KeepAlive Timers**: Global timers inside components cached by `<KeepAlive>` MUST be started in `onActivated` and stopped in `onDeactivated` to prevent memory leaks (Zombie RAM).

### Windows & PowerShell Conventions (Local Terminal Only)
- Path separator: Backslash `\` (not `/`).
- Environment variables: `$env:VAR` (not `export VAR`).
- Command chaining: `;` (not `&&`).

## Examples

### Complanded TypeScript Branded Type
```typescript
type TaskToken<T extends string> = T & { readonly __brand: unique symbol };
```

### Compliant Vue 3 Reactivity (shallowRef)
```typescript
import { shallowRef, triggerRef } from "vue";

const messages = shallowRef<{ role: string; text: string }[]>([]);
// Modify array and trigger reactivity manually
messages.value[messages.value.length - 1].text += chunk;
triggerRef(messages);
```

### Non-Compliant Vue 3 Reactivity (ref)
```typescript
import { ref } from "vue";

// Deep reactivity proxy will cause UI freezes on fast stream updates
const messages = ref<{ role: string; text: string }[]>([]);
messages.value[messages.value.length - 1].text += chunk;
```

## Exceptions
- Local terminal conventions only apply to local developer machine commands, not Git commands or npm cross-platform scripts.
