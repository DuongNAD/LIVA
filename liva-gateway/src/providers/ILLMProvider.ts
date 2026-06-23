import type OpenAI from "openai";
import { ChatCompletionRequest, NativeEmbeddingResponse, SwapModelResult } from "../utils/NativeIPCClient";

export interface ILLMProvider {
    chat: {
        completions: {
            create(
                params: ChatCompletionRequest | OpenAI.ChatCompletionCreateParams,
                retryCount?: number
            ): Promise<unknown>;
        };
    };
    healthCheck(): Promise<boolean>;
    swapModel(
        modelPath: string,
        nCtx?: number,
        nGpuLayers?: number,
        backend?: string
    ): Promise<SwapModelResult>;
    embed(input: string | string[]): Promise<NativeEmbeddingResponse>;
    destroy(): Promise<void> | void;
}
