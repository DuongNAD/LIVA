import type OpenAI from "openai";
import { GemmaLLMProvider } from "./GemmaLLMProvider";
import { ChatCompletionRequest } from "../../utils/NativeIPCClient";

/**
 * LlamaLLMProvider — Implements ILLMProvider for Llama models.
 * Extends GemmaLLMProvider but defaults the model parameter to "llama".
 */
export class LlamaLLMProvider extends GemmaLLMProvider {
    public override chat = {
        completions: {
            create: (
                params: ChatCompletionRequest | OpenAI.ChatCompletionCreateParams,
                retryCount?: number
            ): Promise<unknown> => {
                const modelParams = { ...params, model: params.model || "llama" };
                const client = this.client as unknown as {
                    chat: {
                        completions: {
                            create(
                                params: ChatCompletionRequest | OpenAI.ChatCompletionCreateParams,
                                retryCount?: number
                            ): Promise<unknown>;
                        };
                    };
                };
                return client.chat.completions.create(modelParams, retryCount);
            }
        }
    };
}
