/**
 * @file src/types/AgentTypes.ts
 * Single Source of Truth for Agent Domain Types.
 * STRICT RULE: NO class implementations or runtime logic in this file.
 */

export type Brand<T, TBread> = T & { readonly __brand_identity: TBread };

export type AgentPhaseType = Brand<string, "AgentPhase">;
export type TaskLaneType = Brand<string, "TaskLane">;

// Helper to create branded types without runtime class overhead
const createPhase = (p: string): AgentPhaseType => p as unknown as AgentPhaseType;
const createLane = (l: string): TaskLaneType => l as unknown as TaskLaneType;

export const AgentPhase = {
    INITIALIZING: createPhase("INITIALIZING"),
    RUNNING: createPhase("RUNNING"),
    PAUSING: createPhase("PAUSING"),
    TERMINATING: createPhase("TERMINATING"),
} as const;
export type AgentPhase = AgentPhaseType;

export const TaskLane = {
    UI_INTERACTION: createLane("ui_interaction"),
    LLM_REASONING: createLane("llm_reasoning"),
    BACKGROUND_JOB: createLane("background_job"),
} as const;
export type TaskLane = TaskLaneType;

export enum TaskState {
    PENDING = "PENDING",
    EXECUTING = "EXECUTING",
    COMPLETED = "COMPLETED",
    FAILED = "FAILED"
}

export interface AuthorityToken<S extends AgentPhase> {
    readonly phase: S;
    isValid(expectedPhase: S, expectedSecret: string): boolean;
}

export interface MessageTask {
    id: string;
    lane: TaskLane;
    data: any;
    state?: TaskState;
    execute: (token: AuthorityToken<AgentPhase>) => Promise<void>;
}
