import { SensoryManager } from "../../memory/SensoryManager";
import { notifyZalo } from "../../utils/ZaloNotifier";
import type { EventCatalog } from "../events/EventCatalog";
import { TypedEventBus } from "../events/TypedEventBus";

export interface AgentLoopCallbacks {
    readonly onThinkingStart?: () => void | Promise<void>;
    readonly onThinkingEnd?: () => void | Promise<void>;
    readonly onStreamStart?: () => void | Promise<void>;
    readonly onStreamChunk?: (chunk: string) => void | Promise<void>;
    readonly onThoughtChunk?: (chunk: string) => void | Promise<void>;
    readonly onSpokenResponse?: (text: string) => void | Promise<void>;
}

export interface ResponseRouterOptions {
    readonly eventBus: TypedEventBus<EventCatalog>;
    readonly registry: SkillExecutor;
}

export interface SkillExecutor {
    executeSkill(name: string, args: unknown): Promise<unknown>;
}

export type ErrorRouteAction = "queue_zalo" | "notified_zalo" | "spoken_error";

export interface ErrorRouteResult {
    readonly action: ErrorRouteAction;
    readonly message: string;
}

export class ResponseRouter {
    #eventBus: TypedEventBus<EventCatalog>;
    #registry: SkillExecutor;
    #activeStreamUnsubscribers: Array<() => void> = [];

    public constructor(options: ResponseRouterOptions) {
        this.#eventBus = options.eventBus;
        this.#registry = options.registry;
    }

    public connectStreamCallbacks(callbacks: Pick<AgentLoopCallbacks, "onStreamStart" | "onStreamChunk">): () => void {
        const unsubscribers = [
            this.#eventBus.on("ai:stream_start", () => {
                void callbacks.onStreamStart?.();
            }),
            this.#eventBus.on("ai:stream_chunk", (payload) => {
                void callbacks.onStreamChunk?.(payload.text);
            }),
        ];

        this.#activeStreamUnsubscribers.push(...unsubscribers);

        return () => {
            for (const unsubscribe of unsubscribers) {
                unsubscribe();
                const index = this.#activeStreamUnsubscribers.indexOf(unsubscribe);
                if (index !== -1) {
                    this.#activeStreamUnsubscribers.splice(index, 1);
                }
            }
        };
    }

    public routeThinkingStart(callbacks: Pick<AgentLoopCallbacks, "onThinkingStart">): void {
        void callbacks.onThinkingStart?.();
    }

    public async acknowledgeZaloRequest(userText: string): Promise<void> {
        if (!this.isZaloMessage(userText)) {
            return;
        }

        try {
            await this.#registry.executeSkill("send_zalo_bot", {
                message: "LIVA da tiep nhan yeu cau va dang danh gia. Du kien mat 10-15s neu la tim kiem nhe, hoac 1-2 phut neu can chuyen giao nao chuyen gia.",
            });
        } catch {
            return;
        }
    }

    public async acknowledgeExpertHandoff(userText: string): Promise<void> {
        if (!this.isZaloMessage(userText)) {
            return;
        }

        try {
            await this.#registry.executeSkill("send_zalo_bot", {
                message: "LIVA dang chuyen tac vu sang Expert 26B tren VRAM. Khong can reload toan bo he thong, vui long cho khoang 5s.",
            });
        } catch {
            return;
        }
    }

    public async routeFinalResponse(
        userText: string,
        finalReply: string,
        callbacks: Pick<AgentLoopCallbacks, "onThinkingEnd" | "onSpokenResponse">,
    ): Promise<void> {
        SensoryManager.getInstance().flush();
        void callbacks.onThinkingEnd?.();
        void callbacks.onSpokenResponse?.(finalReply);

        if (this.isZaloMessage(userText)) {
            await notifyZalo(finalReply);
        }
    }

    public async routeError(
        userText: string,
        error: unknown,
        callbacks: Pick<AgentLoopCallbacks, "onThinkingEnd" | "onSpokenResponse">,
    ): Promise<ErrorRouteResult> {
        const message = this.#errorMessage(error);
        void callbacks.onThinkingEnd?.();

        if (this.isZaloMessage(userText)) {
            if (this.isQueueableAiErrorMessage(message)) {
                return { action: "queue_zalo", message };
            }

            await notifyZalo(`Loi he thong Zalo: ${message}`);
            return { action: "notified_zalo", message };
        }

        void callbacks.onSpokenResponse?.(`Vang Native AI: ${message}`);
        return { action: "spoken_error", message };
    }

    public isZaloMessage(userText: string): boolean {
        return userText.includes("[Tin nhắn từ Zalo điện thoại]");
    }

    public isQueueableAiErrorMessage(message: string): boolean {
        return message.includes("ECONNREFUSED")
            || message.includes("fetch failed")
            || message.includes("timeout");
    }

    public dispose(): void {
        for (const unsubscribe of this.#activeStreamUnsubscribers.splice(0)) {
            unsubscribe();
        }
    }

    #errorMessage(error: unknown): string {
        return error instanceof Error ? error.message : String(error);
    }
}
