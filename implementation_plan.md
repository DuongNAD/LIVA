# Giảm Coupling — Refactor Kiến Trúc 3 Phase

## Bối cảnh

Qua audit, 1 lỗi (ví dụ: extract sub-agent) gây **cascade 49 test failures** vì:
1. `AgentLoop.ts` vừa là FSM vừa là barrel export → sửa nó = sửa mọi nơi import
2. Singleton `getInstance()` = hidden dependency → không inject mock được
3. Concrete class coupling → đổi signature = ripple effect toàn hệ thống

## Nguyên tắc

- **Mỗi phase deploy độc lập** — không phase nào phụ thuộc phase khác
- **Zero breaking change** — backward compatible 100%, test phải pass liên tục
- **Chỉ sửa import paths + thêm interface** — KHÔNG refactor business logic

---

## Phase 1: Tách Barrel Export ra `src/core/index.ts`

### Vấn đề hiện tại

```
AgentLoop.ts (490 dòng) = FSM Logic + Barrel Export Hub
├── export * from "../types/AgentTypes"
├── export { CoreKernelAuthority }
├── export { DualPortController }
├── export { ToolExecutionOrchestrator }
├── export { LTCOrchestrator }
└── export { TaskLaneWorker }
```

Khi thêm/xóa 1 sub-agent → phải sửa AgentLoop.ts → tất cả file import từ AgentLoop bị ảnh hưởng.

### Giải pháp

Tạo `src/core/index.ts` làm barrel, xóa re-exports khỏi `AgentLoop.ts`:

#### [NEW] [index.ts](file:///e:/project/openclaw_remake/openclaw-gateway/src/core/index.ts)

```typescript
// src/core/index.ts — Barrel Export for /core module
export { AgentLoop } from "./AgentLoop";
export { CoreKernel } from "./CoreKernel";
export { CoreKernelAuthority } from "./CoreKernelAuthority";
export { DualPortController } from "./DualPortController";
export { ToolExecutionOrchestrator } from "./ToolExecutionOrchestrator";
export { LTCOrchestrator } from "./LTCOrchestrator";
export { TaskLaneWorker } from "./TaskLaneWorker";
export { ModelOrchestrator } from "./ModelOrchestrator";
export { PromptBuilder } from "./PromptBuilder";
export { UIController } from "./UIController";
export { ZaloPolling } from "./ZaloPolling";
export { TelemetryProfiler } from "./TelemetryProfiler";
export * from "../types/AgentTypes";
```

#### [MODIFY] [AgentLoop.ts](file:///e:/project/openclaw_remake/openclaw-gateway/src/core/AgentLoop.ts)

```diff
-export * from "../types/AgentTypes";
-export { CoreKernelAuthority } from "./CoreKernelAuthority";
-export { DualPortController } from "./DualPortController";
-export { ToolExecutionOrchestrator } from "./ToolExecutionOrchestrator";
-export { LTCOrchestrator } from "./LTCOrchestrator";
-export { TaskLaneWorker } from "./TaskLaneWorker";
+// All sub-agent exports moved to src/core/index.ts
```

#### [MODIFY] Consumer imports

| File | Trước | Sau |
|------|-------|-----|
| `CoreKernel.ts` L2 | `from "../core/AgentLoop"` | `from "../core"` (chỉ AgentLoop) |
| `AgentLoop.test.ts` L14 | `from "../../src/core/AgentLoop"` | `from "../../src/core"` |
| `test_heuristic_sanitize.ts` L1 | `from "../src/core/AgentLoop.js"` | `from "../src/core"` |

### Blast Radius: 4 files

> [!NOTE]
> Sau Phase 1, khi thêm sub-agent mới → chỉ sửa `src/core/index.ts` (1 file), không động vào `AgentLoop.ts`.

---

## Phase 2: Định nghĩa Interfaces (Contracts)

### Vấn đề hiện tại

```typescript
// AgentLoop.ts — phụ thuộc concrete class
constructor(memory: MemoryManager, registry: SkillRegistry) { ... }

// PromptBuilder.ts — gọi thẳng concrete singleton
const sensoryPrompt = SensoryManager.getInstance().injectSensoryPrompt();
```

Khi `MemoryManager` thêm method → AgentLoop, PromptBuilder, và tất cả test phải biết.

### Giải pháp

Tạo `src/types/Contracts.ts` định nghĩa interface cho từng boundary:

#### [NEW] [Contracts.ts](file:///e:/project/openclaw_remake/openclaw-gateway/src/types/Contracts.ts)

```typescript
// ============================================================
// Cross-Layer Contracts — Interface giữa các module
// ============================================================

import type { ChatMessage } from "../MemoryManager";
import type { AgentSkill } from "../SkillRegistry";

/** Contract: Memory Layer → Core Layer */
export interface IMemoryProvider {
    addMessage(role: "user" | "assistant" | "system", content: string): Promise<void>;
    getHybridContext(query: string, windowSize?: number): Promise<ChatMessage[]>;
    getShortTermHistory(): Promise<ChatMessage[]>;
    getLongTermContext(): Promise<string>;
    updateLongTermMemory(category: string, facts: string[]): Promise<void>;
    getStructuredMemoryPrompt(): string;
    getStructuredFacts(): any[];
    setStructuredFact(key: string, value: string, options?: any): void;
    getStructuredMemoryInstance(): any;
    initialize(): Promise<void>;
    dispose(): void;
}

/** Contract: Skill Layer → Core Layer */
export interface ISkillExecutor {
    executeSkill(name: string, args: any): Promise<string>;
    getAllSkills(): AgentSkill[];
    registerSkill(skill: AgentSkill): void;
}

/** Contract: Security Layer → Core Layer */
export interface ISecurityGuard {
    sanitizeInput(text: string): { sanitized: string; blocked: boolean; reason?: string };
    sanitizeOutput(text: string): { sanitized: string; flagged: boolean };
}

/** Contract: Sensory Layer → Core Layer */
export interface ISensoryProvider {
    captureContext(): Promise<void>;
    injectSensoryPrompt(): string;
    flush(): void;
    dispose(): void;
}

/** Contract: Embedding Layer → Memory Layer */
export interface IEmbeddingProvider {
    embed(text: string): Promise<number[]>;
    embedWithTimeout(text: string, timeoutMs: number): Promise<number[]>;
    ensureReady(): Promise<void>;
    readonly ready: boolean;
    dispose(): void;
}
```

#### [MODIFY] Concrete classes `implements` interface

| File | Thay đổi |
|------|----------|
| `MemoryManager.ts` | `export class MemoryManager implements IMemoryProvider` |
| `SkillRegistry.ts` | `export class SkillRegistry implements ISkillExecutor` |
| `ZMAS_Guard.ts` | `export class ZMAS_Guard implements ISecurityGuard` (static → instance) |
| `SensoryManager.ts` | `export class SensoryManager implements ISensoryProvider` |
| `EmbeddingService.ts` | `export class EmbeddingService implements IEmbeddingProvider` |

> [!IMPORTANT]
> Chỉ thêm `implements`, KHÔNG sửa body. TypeScript compiler sẽ tự kiểm tra compliance.

### Blast Radius: 6 files (1 new + 5 add `implements`)

---

## Phase 3: Constructor Injection cho Singletons

### Vấn đề hiện tại

```typescript
// 5 nơi gọi getInstance() trực tiếp — hidden dependency
SensoryManager.getInstance()    // PromptBuilder.ts, AgentLoop.ts, CoreKernel.ts
EmbeddingService.getInstance()  // MemoryManager.ts, LanceMemory.ts, LearningLog.ts, CoreKernel.ts
CoreKernelAuthority.getInstance() // AgentLoop.ts
MCPClientManager.getInstance()    // SkillRegistry.ts
```

### Giải pháp

Consumer nhận dependency qua constructor, **CoreKernel** (composition root) wire tất cả:

#### [MODIFY] [AgentLoop.ts](file:///e:/project/openclaw_remake/openclaw-gateway/src/core/AgentLoop.ts)

```diff
 // Trước
-constructor(memory: MemoryManager, registry: SkillRegistry) {
-    this.#authority = CoreKernelAuthority.getInstance();

 // Sau
+constructor(
+    memory: IMemoryProvider,
+    registry: ISkillExecutor,
+    authority: CoreKernelAuthority  // injected instead of getInstance()
+) {
+    this.#authority = authority;
```

#### [MODIFY] [PromptBuilder.ts](file:///e:/project/openclaw_remake/openclaw-gateway/src/core/PromptBuilder.ts)

```diff
 // Trước
-const sensoryPrompt = SensoryManager.getInstance().injectSensoryPrompt();

 // Sau — sensory injected via constructor
+constructor(private sensory: ISensoryProvider) {}
+
+buildSystemPrompt() {
+    const sensoryPrompt = this.sensory.injectSensoryPrompt();
```

#### [MODIFY] [MemoryManager.ts](file:///e:/project/openclaw_remake/openclaw-gateway/src/MemoryManager.ts)

```diff
 // Trước
-this.embeddingService = EmbeddingService.getInstance();

 // Sau
+constructor(agentId: string, embeddingService: IEmbeddingProvider) {
+    this.embeddingService = embeddingService;
```

#### [MODIFY] [CoreKernel.ts](file:///e:/project/openclaw_remake/openclaw-gateway/src/core/CoreKernel.ts) — Composition Root

```diff
 constructor() {
+    const embedding = EmbeddingService.getInstance();
+    const sensory = SensoryManager.getInstance();
+    const authority = CoreKernelAuthority.getInstance();
+
-    this.memory = new MemoryManager("liv_async_core");
+    this.memory = new MemoryManager("liv_async_core", embedding);
     this.registry = new SkillRegistry();
     this.ui = new UIController(8082);
-    this.agentLoop = new AgentLoop(this.memory, this.registry);
+    this.agentLoop = new AgentLoop(this.memory, this.registry, authority);
```

> [!IMPORTANT]
> **Singleton vẫn tồn tại** — chỉ đổi nơi gọi `getInstance()` từ consumer sang CoreKernel (composition root). Đây là pattern **"Poor Man's DI"** — không cần DI framework.

### Blast Radius: 5 files

| File | Thay đổi |
|------|----------|
| `AgentLoop.ts` | Constructor nhận `authority` param |
| `PromptBuilder.ts` | Constructor nhận `sensory` param |
| `MemoryManager.ts` | Constructor nhận `embeddingService` param |
| `CoreKernel.ts` | Wire tất cả tại composition root |
| `AgentLoop.test.ts` | Mock inject qua constructor (không cần `as any` hack) |

---

## Tổng kết Impact

| Phase | Files modified | Risk | Benefit |
|-------|---------------|------|---------|
| 1. Barrel Export | 4 | 🟢 Low | Thêm sub-agent = sửa 1 file thay vì N |
| 2. Interfaces | 6 | 🟢 Low | TypeScript compiler bắt breaking changes |
| 3. Constructor DI | 5 | 🟡 Medium | Test mock dễ, không cần `(Class as any).instance = null` |
| **Total** | **~15 files** | | |

## Thứ tự thực hiện

```
Phase 1 (Barrel) → chạy test → Phase 2 (Interfaces) → chạy test → Phase 3 (DI) → chạy test
```

Mỗi phase kết thúc bằng `npx vitest run` — 299 tests phải pass 100%.

## Open Questions

> [!IMPORTANT]
> **ZMAS_Guard hiện dùng static methods** (`ZMAS_Guard.sanitizeInput()`). Phase 2 interface yêu cầu instance methods. Có 2 lựa chọn:
> 1. Giữ static, interface chỉ wrap static calls
> 2. Chuyển sang instance (breaking change nhỏ)
> 
> Bạn chọn phương án nào?

> [!NOTE]
> **LanceMemory.ts** và **LearningLog.ts** cũng gọi `EmbeddingService.getInstance()`. Phase 3 có nên inject vào 2 file này luôn không? Hay để sau (chúng ít thay đổi)?
