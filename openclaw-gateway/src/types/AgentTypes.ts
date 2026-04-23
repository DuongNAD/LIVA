export type Brand<T, TBread> = T & { readonly __brand_identity: TBread };

export type AgentPhaseType = Brand<string, "AgentPhase">;
export type TaskLaneType = Brand<string, "TaskLane">;

const createPhase = (p: string): AgentPhaseType => p as unknown as AgentPhaseType;
const createLane = (l: string): TaskLaneType => l as unknown as TaskLaneType;

export const AgentPhase = {
    INITIALIZING: createPhase("INITIALIZING"),
    IDLE: createPhase("IDLE"),
    RUNNING: createPhase("RUNNING"),
    AWAITING_APPROVAL: createPhase("AWAITING_APPROVAL"),
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

export class AuthorityToken<S extends AgentPhase> {
    public readonly phase: S;
    #secret: string;

    constructor(phase: S, secret: string) {
        this.phase = phase;
        this.#secret = secret;
    }

    public isValid(expectedPhase: S, expectedSecret: string): boolean {
        return this.phase === expectedPhase && this.#secret === expectedSecret;
    }
}

export interface MessageTask {
    id: string;
    lane: TaskLane;
    data: any;
    state?: TaskState;
    execute: (token: AuthorityToken<AgentPhase>) => Promise<void>;
}
