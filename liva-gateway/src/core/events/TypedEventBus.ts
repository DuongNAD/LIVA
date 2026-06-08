import { EventEmitter } from "node:events";
import { createHash } from "crypto";
import { logger } from "../../utils/logger";

export interface EventMetadata {
    eventId: string;
    timestamp: number;
    source: string;
    correlationId?: string;
    retryCount?: number;
}

export interface DeadLetter {
    topic: string;
    payload: any;
    metadata: EventMetadata;
    error: string;
}

type EventKey<TEvents extends object> = Extract<keyof TEvents, string | symbol>;
type NodeEventHandler = (...args: unknown[]) => void;

type EmitArguments<TEvents extends object, TEvent extends EventKey<TEvents>> =
    [TEvents[TEvent]] extends [void]
        ? [eventName: TEvent]
        : [eventName: TEvent, payload: TEvents[TEvent]];

type EventSubscription<TEvents extends object> = {
    readonly eventName: EventKey<TEvents>;
    readonly listener: NodeEventHandler;
    readonly originalHandler: NodeEventHandler;
};

export type TypedEventHandler<TPayload> =
    [TPayload] extends [void] ? () => void : (payload: TPayload) => void;

export class TypedEventBus<TEvents extends object> {
    #emitter: EventEmitter;
    #subscriptions: EventSubscription<TEvents>[] = [];
    #disposed = false;
    private recentEventsCache: Map<string, number> = new Map();
    private deadLetterQueue: DeadLetter[] = [];
    private readonly CACHE_TTL_MS = 2500;
    private cleanupInterval: NodeJS.Timeout;

    public constructor(emitter: EventEmitter = new EventEmitter()) {
        this.#emitter = emitter;
        this.cleanupInterval = setInterval(() => this.cleanupCache(), this.CACHE_TTL_MS);
        if (this.cleanupInterval.unref) {
            this.cleanupInterval.unref();
        }
    }

    private generateEventHash(topic: string, payload: unknown): string {
        const safePayload = typeof payload === 'object' ? JSON.stringify(payload, this.getCircularReplacer()) : String(payload);
        return createHash('sha256').update(`${topic}:${safePayload}`).digest('hex');
    }

    private getCircularReplacer() {
        const seen = new WeakSet();
        return (key: string, value: any) => {
            if (typeof value === "object" && value !== null) {
                if (seen.has(value)) return;
                seen.add(value);
            }
            return value;
        };
    }

    private cleanupCache(): void {
        const now = Date.now();
        for (const [hash, timestamp] of this.recentEventsCache.entries()) {
            if (now - timestamp > this.CACHE_TTL_MS) {
                this.recentEventsCache.delete(hash);
            }
        }
    }

    public getDeadLetters(): DeadLetter[] {
        return [...this.deadLetterQueue];
    }

    public clearDeadLetters(): void {
        this.deadLetterQueue = [];
    }

    public get isDisposed(): boolean {
        return this.#disposed;
    }

    public on<TEvent extends EventKey<TEvents>>(
        eventName: TEvent,
        handler: TypedEventHandler<TEvents[TEvent]>,
    ): () => void {
        this.#assertActive();

        const listener = async (payload: any) => {
            try {
                await (handler as any)(payload);
            } catch (error: any) {
                logger.error({ err: error, eventName: String(eventName) }, `[TypedEventBus] Listener failed for ${String(eventName)}. Routing to DLQ.`);
                this.deadLetterQueue.push({
                    topic: String(eventName),
                    payload,
                    metadata: {
                        eventId: crypto.randomUUID(),
                        timestamp: Date.now(),
                        source: 'system-core',
                    },
                    error: error.message || String(error)
                });
            }
        };

        const subscription: EventSubscription<TEvents> = { eventName, listener, originalHandler: handler as NodeEventHandler };
        this.#subscriptions.push(subscription);
        this.#emitter.on(eventName, listener);

        return () => {
            this.#removeSubscription(subscription);
        };
    }

    public off<TEvent extends EventKey<TEvents>>(
        eventName: TEvent,
        handler: TypedEventHandler<TEvents[TEvent]>,
    ): void {
        const listener = handler as NodeEventHandler;
        const subscription = this.#subscriptions.find(
            (candidate) => candidate.eventName === eventName && candidate.originalHandler === listener,
        );

        if (subscription) {
            this.#removeSubscription(subscription);
        }
    }

    public emit<TEvent extends EventKey<TEvents>>(
        ...args: EmitArguments<TEvents, TEvent>
    ): boolean {
        this.#assertActive();

        const eventName = args[0];
        const payload = args[1];

        if (payload !== undefined) {
            const eventHash = this.generateEventHash(String(eventName), payload);
            const now = Date.now();

            if (this.recentEventsCache.has(eventHash)) {
                logger.warn(`[TypedEventBus] Dropped duplicate event storm for topic: ${String(eventName)}`);
                return false;
            }
            this.recentEventsCache.set(eventHash, now);
        }

        if (args.length === 1) {
            return this.#emitter.emit(eventName);
        }

        return this.#emitter.emit(eventName, args[1] as TEvents[TEvent]);
    }

    public listenerCount<TEvent extends EventKey<TEvents>>(eventName: TEvent): number {
        return this.#emitter.listenerCount(eventName);
    }

    public dispose(): void {
        if (this.#disposed) {
            return;
        }

        clearInterval(this.cleanupInterval);

        for (const subscription of this.#subscriptions.splice(0)) {
            this.#emitter.off(subscription.eventName, subscription.listener);
        }

        this.#disposed = true;
    }

    #removeSubscription(subscription: EventSubscription<TEvents>): void {
        const index = this.#subscriptions.indexOf(subscription);
        if (index === -1) {
            return;
        }

        this.#subscriptions.splice(index, 1);
        this.#emitter.off(subscription.eventName, subscription.listener);
    }

    #assertActive(): void {
        if (this.#disposed) {
            throw new Error("TypedEventBus has been disposed");
        }
    }
}

export const globalEventBus = new TypedEventBus<any>();
