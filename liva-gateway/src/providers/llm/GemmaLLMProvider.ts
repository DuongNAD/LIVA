import type OpenAI from "openai";
import { ILLMProvider } from "../ILLMProvider";
import { NativeIPCClient, ChatCompletionRequest, NativeEmbeddingResponse, SwapModelResult } from "../../utils/NativeIPCClient";

/**
 * GemmaLLMProvider — Implements ILLMProvider for Gemma models by wrapping NativeIPCClient or OpenAI.
 */
interface ChatCompletionClient {
    chat: {
        completions: {
            create(
                params: ChatCompletionRequest | OpenAI.ChatCompletionCreateParams,
                retryCount?: number
            ): Promise<unknown>;
        };
    };
    healthCheck?(): Promise<boolean>;
    swapModel?(
        modelPath: string,
        nCtx?: number,
        nGpuLayers?: number,
        backend?: string
    ): Promise<SwapModelResult>;
    embed?(input: string | string[]): Promise<NativeEmbeddingResponse>;
    destroy?(): Promise<void> | void;
}

export class GemmaLLMProvider implements ILLMProvider {
    protected client: OpenAI | NativeIPCClient;

    constructor(client?: OpenAI | NativeIPCClient) {
        this.client = client || new NativeIPCClient();
    }

    public chat = {
        completions: {
            create: (
                params: ChatCompletionRequest | OpenAI.ChatCompletionCreateParams,
                retryCount?: number
            ): Promise<unknown> => {
                const modelParams = { ...params, model: params.model || "gemma" };
                const client = this.client as unknown as ChatCompletionClient;
                return client.chat.completions.create(modelParams, retryCount);
            }
        }
    };

    /**
     * Check if the backend inference server is responsive.
     */
    public async healthCheck(): Promise<boolean> {
        const client = this.client as unknown as ChatCompletionClient;
        if (typeof client.healthCheck === "function") {
            return client.healthCheck();
        }
        return true;
    }

    /**
     * Swap the current active model in the Python engine.
     */
    public async swapModel(
        modelPath: string,
        nCtx?: number,
        nGpuLayers?: number,
        backend?: string
    ): Promise<SwapModelResult> {
        const client = this.client as unknown as ChatCompletionClient;
        if (typeof client.swapModel === "function") {
            return client.swapModel(modelPath, nCtx, nGpuLayers, backend);
        }
        return { success: true, errorMessage: "", loadedModel: modelPath, swapDurationMs: 0 };
    }

    /**
     * Generate text embeddings.
     */
    public async embed(input: string | string[]): Promise<NativeEmbeddingResponse> {
        const client = this.client as unknown as ChatCompletionClient;
        if (typeof client.embed === "function") {
            return client.embed(input);
        }
        return { data: [], model: "fallback", dimensions: 0 };
    }

    /**
     * Clean up client resources.
     */
    public async destroy(): Promise<void> {
        const client = this.client as unknown as ChatCompletionClient;
        if (typeof client.destroy === "function") {
            client.destroy();
        }
    }
}

