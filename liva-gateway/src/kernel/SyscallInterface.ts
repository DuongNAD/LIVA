export enum SyscallPriority {
    HRT = 0, // Hard Real-Time (Voice Interrupt, HITL, Emergency)
    SRT = 1, // Soft Real-Time (User Chat, Agent Reasoning)
    DT = 2   // Delay-Tolerant (Consolidation, Indexing, GC)
}

export type SyscallType = 
    | "syscall_infer" 
    | "syscall_vector_search" 
    | "syscall_execute_tool" 
    | "syscall_read_memory" 
    | "syscall_write_memory"
    | "syscall_snapshot_save"
    | "syscall_snapshot_restore"
    | "syscall_a2a_message";

export interface SyscallRequest<T = unknown> {
    id: string;
    type: SyscallType;
    priority: SyscallPriority;
    payload: Record<string, unknown>;
    resolve?: (value: T) => void;
    reject?: (reason?: unknown) => void;
}
