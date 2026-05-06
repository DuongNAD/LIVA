import type { KernelConfig } from "../config/KernelConfig";

export type CoreEventSource = "websocket" | "voice" | "telegram" | "meta" | "system";

export interface EventCatalog {
    "ai:stream_start": {
        readonly id: string;
    };
    "ai:stream_chunk": {
        readonly id: string;
        readonly text: string;
        readonly index?: number;
    };
    "ai:stream_complete": {
        readonly id: string;
        readonly text: string;
    };
    "ai:stream_error": {
        readonly id: string;
        readonly error: Error;
    };
    "ui:user_input": {
        readonly text: string;
        readonly source: CoreEventSource;
        readonly sessionId?: string;
    };
    "ui:interrupt": {
        readonly reason?: string;
    };
    "voice:suspend_peripherals": {
        readonly reason?: string;
    };
    "voice:resume_peripherals": {
        readonly reason?: string;
    };
    "kernel:ready": {
        readonly startedAt: Date;
    };
    "kernel:error": {
        readonly phase: string;
        readonly error: Error;
    };
    "kernel:config_updated": {
        readonly config: KernelConfig;
    };
}

export type EventName = keyof EventCatalog;
export type EventPayload<TEvent extends EventName> = EventCatalog[TEvent];
