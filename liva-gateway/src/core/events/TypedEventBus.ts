import { EventEmitter } from "node:events";

type EventKey<TEvents extends object> = Extract<keyof TEvents, string | symbol>;
type NodeEventHandler = (...args: unknown[]) => void;

type EmitArguments<TEvents extends object, TEvent extends EventKey<TEvents>> =
    [TEvents[TEvent]] extends [void]
        ? [eventName: TEvent]
        : [eventName: TEvent, payload: TEvents[TEvent]];

type EventSubscription<TEvents extends object> = {
    readonly eventName: EventKey<TEvents>;
    readonly listener: NodeEventHandler;
};

export type TypedEventHandler<TPayload> =
    [TPayload] extends [void] ? () => void : (payload: TPayload) => void;

export class TypedEventBus<TEvents extends object> {
    #emitter: EventEmitter;
    #subscriptions: EventSubscription<TEvents>[] = [];
    #disposed = false;

    public constructor(emitter: EventEmitter = new EventEmitter()) {
        this.#emitter = emitter;
    }

    public get isDisposed(): boolean {
        return this.#disposed;
    }

    public on<TEvent extends EventKey<TEvents>>(
        eventName: TEvent,
        handler: TypedEventHandler<TEvents[TEvent]>,
    ): () => void {
        this.#assertActive();

        const listener = handler as NodeEventHandler;
        const subscription: EventSubscription<TEvents> = { eventName, listener };
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
            (candidate) => candidate.eventName === eventName && candidate.listener === listener,
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
