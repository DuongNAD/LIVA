import { AsyncLocalStorage } from "node:async_hooks";
import { setup, createActor, assign } from "xstate";
import { logger } from "../../utils/logger";
import type { AgentLoopEvent } from "../AgentLoop";

export interface LoopStateDelegate {
    _executeUserInput(text: string, isHeartbeat: boolean, bypassRateLimit: boolean, isDryRun?: boolean): void;
    internalBargeIn(): void;
    handleUserInput(text: string, isHeartbeat: boolean, bypassRateLimit: boolean): Promise<void>;
    onSystemBusy?: (message: string) => void | Promise<void>;
    unloadModel(): Promise<void>;
}

export class LoopStateManager {
    public activeTurnCount = 0;
    public readonly turnStorage = new AsyncLocalStorage<number>();
    
    private lastInputTime = 0;
    private readonly RATE_LIMIT_MS = 1000; // 1 second minimum between messages
    
    private idleTimer: NodeJS.Timeout | null = null;
    private readonly IDLE_TIMEOUT_MS = 5 * 60 * 1000; // 5 minutes
    
    public streamAbortController: AbortController | null = null;
    public spokenTokenCount = 0;
    public currentStreamedText = "";
    public wasBargedIn = false;
    
    public readonly stateMachineActor: ReturnType<typeof createActor>;
    public delegate: LoopStateDelegate;

    constructor(delegate: LoopStateDelegate) {
        this.delegate = delegate;
        
        const agentMachine = setup({
            types: {
                context: {} as {
                    nextPendingMessage: string | null;
                    manager: LoopStateManager;
                },
                events: {} as AgentLoopEvent,
                input: {} as { manager: LoopStateManager }
            },
            actions: {
                queuePendingMessage: assign({
                    nextPendingMessage: ({ event }) => event.type === 'USER_INPUT' ? event.text : null
                }),
                triggerAbort: ({ context }) => {
                    context.manager.internalBargeIn();
                },
                notifyBusy: ({ context }) => {
                    if (context.manager.delegate.onSystemBusy) {
                        context.manager.delegate.onSystemBusy("Liva đang dừng suy nghĩ cũ để xử lý câu hỏi mới của bạn!");
                    }
                },
                startExecution: ({ context, event }) => {
                    if (event.type === 'USER_INPUT') {
                        context.manager.delegate._executeUserInput(event.text, event.isHeartbeat, event.bypassRateLimit, event.isDryRun);
                    }
                },
                checkPendingMessage: assign({
                    nextPendingMessage: ({ context }) => {
                        if (context.nextPendingMessage) {
                            const msg = context.nextPendingMessage;
                            // Execute on next tick to avoid synchronous loop
                            setTimeout(() => {
                                context.manager.delegate.handleUserInput(msg, false, true);
                            }, 0);
                        }
                        return null;
                    }
                }),
                clearPendingMessage: assign({
                    nextPendingMessage: null
                })
            }
        }).createMachine({
            id: 'agentLoop',
            initial: 'idle',
            context: ({ input }) => ({
                nextPendingMessage: null,
                manager: input.manager
            }),
            states: {
                idle: {
                    entry: ['checkPendingMessage'],
                    on: {
                        USER_INPUT: {
                            target: 'thinking',
                            actions: ['startExecution']
                        },
                        BARGE_IN: {}, // Ignore
                        SPEECH_START: {} // Ignore
                    }
                },
                thinking: {
                    on: {
                        USER_INPUT: {
                            target: 'aborting',
                            actions: ['queuePendingMessage', 'triggerAbort', 'notifyBusy']
                        },
                        SPEECH_START: {
                            target: 'aborting',
                            actions: ['triggerAbort']
                        },
                        BARGE_IN: {
                            target: 'aborting',
                            actions: ['triggerAbort']
                        },
                        STREAM_START: {
                            target: 'streaming'
                        },
                        EXECUTION_DONE: { target: 'idle' },
                        EXECUTION_ERROR: { target: 'idle' }
                    }
                },
                streaming: {
                    on: {
                        USER_INPUT: {
                            target: 'aborting',
                            actions: ['queuePendingMessage', 'triggerAbort', 'notifyBusy']
                        },
                        SPEECH_START: {
                            target: 'aborting',
                            actions: ['triggerAbort']
                        },
                        BARGE_IN: {
                            target: 'aborting',
                            actions: ['triggerAbort']
                        },
                        EXECUTION_DONE: { target: 'idle' },
                        EXECUTION_ERROR: { target: 'idle' }
                    }
                },
                aborting: {
                    on: {
                        USER_INPUT: {
                            actions: ['queuePendingMessage', 'notifyBusy']
                        },
                        SPEECH_START: {}, // Already aborting
                        BARGE_IN: {}, // Already aborting
                        STREAM_START: {}, // Ignore
                        EXECUTION_DONE: { target: 'idle' },
                        EXECUTION_ERROR: { target: 'idle' }
                    }
                }
            }
        });

        this.stateMachineActor = createActor(agentMachine, { input: { manager: this } });
        this.stateMachineActor.start();

        this.stateMachineActor.subscribe((state) => {
            if (state.value === 'idle') {
                this.touchIdleTimer();
            } else {
                if (this.idleTimer) {
                    clearTimeout(this.idleTimer);
                    this.idleTimer = null;
                }
            }
        });
    }

    public sendActorEvent(event: AgentLoopEvent): void {
        if (this.stateMachineActor && this.stateMachineActor.getSnapshot().status === 'active') {
            this.stateMachineActor.send(event);
        }
    }

    public getCurrentStateValue(): string {
        return this.stateMachineActor.getSnapshot().value as string;
    }

    public wrapCallback<T extends (...args: never[]) => unknown>(callback?: T): T | undefined {
        if (!callback) return undefined;
        return new Proxy(callback, {
            apply: (target, thisArg, argumentsList) => {
                const store = this.turnStorage.getStore();
                if (store !== undefined && store !== this.activeTurnCount) {
                    logger.info(`[Turn Guard] 🤫 Suppressed stale callback execution for turn ${store} (active: ${this.activeTurnCount})`);
                    return;
                }
                return Reflect.apply(target, thisArg, argumentsList);
            },
            get(target, prop, receiver) {
                return Reflect.get(target, prop, receiver);
            }
        }) as unknown as T;
    }

    public touchIdleTimer(): void {
        if (this.idleTimer) {
            clearTimeout(this.idleTimer);
        }
        this.idleTimer = setTimeout(async () => {
            this.idleTimer = null;
            logger.info("[AgentLoop] ♻️ LIVA has been inactive for 5 minutes. Unloading AI model/server to free VRAM.");
            try {
                await this.delegate.unloadModel();
            } catch (err: unknown) {
                const msg = err instanceof Error ? err.message : String(err);
                logger.error(`[AgentLoop] Error unloading llama-server: ${msg}`);
            }
        }, this.IDLE_TIMEOUT_MS);
        this.idleTimer.unref();
    }

    public clearIdleTimer(): void {
        if (this.idleTimer) {
            clearTimeout(this.idleTimer);
            this.idleTimer = null;
        }
    }

    public checkRateLimit(userText: string, isHeartbeat: boolean, bypassRateLimit: boolean): boolean {
        const now = Date.now();
        if (!isHeartbeat && !bypassRateLimit) {
            if (now - this.lastInputTime < this.RATE_LIMIT_MS) {
                logger.warn(`[Rate Limiter] Thao tác quá nhanh! Bỏ qua tin nhắn: ${userText.substring(0, 50)}`);
                return false;
            }
            this.lastInputTime = now;
        }
        return true;
    }

    public checkVramGuard(userText: string): boolean {
        const MAX_INPUT_LENGTH = 20000; // Khoảng 6000 tokens
        if (userText.length > MAX_INPUT_LENGTH) {
            logger.warn(`[VRAM Guard] Từ chối input quá dài (${userText.length} ký tự). Tránh Segfault!`);
            return false;
        }
        return true;
    }

    public bargeIn(type: 'BARGE_IN' | 'SPEECH_START' = 'BARGE_IN'): void {
        this.sendActorEvent({ type });
    }

    public internalBargeIn(): void {
        if (this.streamAbortController) {
            this.streamAbortController.abort();
            this.streamAbortController = null;
            this.wasBargedIn = true;
            logger.warn(`[Barge-in] 🛑 LLM stream aborted. Spoken: ${this.spokenTokenCount} tokens, ${this.currentStreamedText.length} chars.`);
            
            this.delegate.internalBargeIn();
        }
    }

    public shutdown(): void {
        if (this.idleTimer) {
            clearTimeout(this.idleTimer);
            this.idleTimer = null;
        }
        if (this.stateMachineActor) {
            this.stateMachineActor.stop();
        }
    }
}
