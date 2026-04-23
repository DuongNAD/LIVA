// ============================================================
// Cross-Layer Contracts — Interfaces between modules
// ============================================================
// These interfaces define the boundaries between architectural layers.
// Consumers depend on interfaces, not concrete classes.
// Concrete classes `implements` these contracts.
// ============================================================

import type { StructuredFact } from "../memory/StructuredMemory";
import type { StructuredMemory } from "../memory/StructuredMemory";

// Re-export StructuredFact for convenience
export type { StructuredFact };

/** Chat message structure shared across memory layer */
export interface ChatMessage {
    role: "user" | "assistant" | "system";
    content: string;
    timestamp: number;
}

/** Skill metadata structure */
export interface SkillMetadata {
    name: string;
    description: string;
    parameters: Record<string, unknown>;
    search_keywords?: string[];
    isCoreSkill?: boolean;
    requiresApproval?: boolean;
    execute?: (args: Record<string, unknown>) => Promise<unknown>;
}

/** Options for setting structured facts */
export interface FactOptions {
    ttlDays?: number;
    source?: string;
    category?: string;
}

// ============================================================
// Layer Contracts
// ============================================================

/** Contract: Memory Layer → Core Layer */
export interface IMemoryProvider {
    addMessage(role: "user" | "assistant" | "system", content: string): Promise<void>;
    getHybridContext(query: string, windowSize?: number): Promise<ChatMessage[]>;
    getShortTermHistory(): Promise<ChatMessage[]>;
    getLongTermContext(): Promise<string>;
    updateLongTermMemory(category: string, facts: string[]): Promise<void>;
    getStructuredMemoryPrompt(): string;
    getStructuredFacts(): StructuredFact[];
    setStructuredFact(key: string, value: string, options?: FactOptions): void;
    deleteStructuredFact(key: string): boolean;
    getStructuredMemoryInstance(): StructuredMemory;
    getUserProfile(): Promise<Record<string, unknown> | null>;
    updateUserProfile(updates: Record<string, unknown>): Promise<void>;
    initialize(): Promise<void>;
    dispose(): void;
}

/** Contract: Skill Layer → Core Layer */
export interface ISkillExecutor {
    executeSkill(name: string, args: Record<string, unknown>): Promise<string>;
    getAllSkills(): SkillMetadata[];
    registerSkill(skill: SkillMetadata): void;
    registerLocalSkills(): Promise<void>;
}

/** Contract: Security Layer → Core Layer */
export interface ISecurityGuard {
    executeAutoRemediation(toolOutput: string, sourceToolName: string): string;
}

/** Contract: Sensory Layer → Core Layer */
export interface ISensoryProvider {
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
