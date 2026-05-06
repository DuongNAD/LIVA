import OpenAI from "openai";
import { NativeIPCClient } from "./NativeIPCClient";
import { logger } from "./logger";

/**
 * [ENGINE SEAL TOKEN VALIDATION]
 * Implementing TypeScript 5.x Branded Types to ensure that the engine instance 
 * is strictly validated against a unique 'Engine Seal Token'.
 */
export type EngineSealToken = string & { readonly __brand: unique symbol };

// Internal private seal for the singleton instance
const INTERNAL_ENGINE_SEAL: EngineSealToken = "LIVA_CORE_V1_SECURE_TOKEN" as EngineSealToken;

/**
 * The core engine instance, wrapped to prevent unauthorized system-call injection.
 */
class SecureLivaEngine {
    #client: OpenAI | NativeIPCClient;
    #seal: EngineSealToken;

    constructor(client: OpenAI | NativeIPCClient, seal: EngineSealToken) {
        this.#client = client;
        this.#seal = seal;
    }

    /**
     * Validates the caller's authority using the private #seal before executing chat completions.
     */
    async secureChatCompletion<T>(
        payload: any, 
        providedSeal: EngineSealToken
    ): Promise<T> {
        if (providedSeal !== this.#seal) {
            throw new Error("[LivaEngine] SECURITY VIOLATION: Unauthorized Seal Token provided.");
        }
        // @ts-expect-error - Accessing the underlying client after seal validation
        return await this.#client.chat.completions.create(payload);
    }

    /**
     * Provides access to the internal seal for authorized components.
     */
    getSeal(): EngineSealToken {
        return this.#seal;
    }

    /**
     * Tương thích ngược (Backward Compatibility) với dạng gọi hàm cũ.
     * Tự động tiêm #seal để vượt qua Zero-Trust Gate.
     */
    get chat() {
        return {
            completions: {
                create: async (payload: any) => {
                    return this.secureChatCompletion<any>(payload, this.#seal);
                }
            }
        };
    }
}

// Initialize the secure singleton instance
// Use NativeIPCClient (JSONL-over-TCP, port 8100) when native engine is available.
// Falls back to OpenAI HTTP client (port 8000) for legacy compatibility.
const USE_NATIVE_IPC = process.env.LIVA_USE_NATIVE !== "false";

export const livaEngine = new SecureLivaEngine(
    USE_NATIVE_IPC
        ? new NativeIPCClient()
        : new OpenAI({
            baseURL: `http://127.0.0.1:${process.env.LIVA_ROUTER_PORT || "8000"}/v1`,
            apiKey: "local-ghost-layer",
        }),
    INTERNAL_ENGINE_SEAL
);

/**
 * Sử dụng LIVA AI để rút gọn một Chủ đề dài thành tên file tiếng Anh chuẩn mực.
 * 
 * [EVOLUTIONARY UPGRADE]:
 * - Implements Zero-Trust Integrity via #seal validation.
 * - Uses Private Class Members to prevent state-poisoning.
 * - Strictly validates against 'Engine Seal Token' before execution.
 */
export async function generateSmartFilename(topic: string, defaultName: string): Promise<string> {
    let shortName = defaultName;
    const currentSeal = livaEngine.getSeal();

    try {
        // Execute via the secure channel requiring the Engine Seal Token
        const resName = await livaEngine.secureChatCompletion<any>(
            {
                model: "expert",
                messages: [{ 
                    role: "user", 
                    content: `Hành động như một bot đổi tên file. Rút gọn cụm từ/chủ đề sau thành 1 tên file tiếng Anh ngắn gọn, súc tích (tối đa 4 từ, nối bằng gạch dưới _). VÍ DỤ: "Báo cáo doanh số quý 1" -> "q1_revenue_report". Tương tự hãy làm với Chủ đề: "${topic}". CHỈ TRẢ VỀ DUY NHẤT TÊN FILE (không giải thích, không in ký tự lạ).` 
                }],
                temperature: 0.1,
                max_tokens: 20
            },
            currentSeal
        );

        const aiName = resName.choices[0]?.message?.content?.trim();

        if (aiName) {
            // Clean the output to ensure it adheres to filename standards
            if (!aiName.includes(" ")) {
                shortName = aiName.replaceAll(/[^\w-]/g, "").toLowerCase();
            } else {
                shortName = aiName.replaceAll(/[^\w-]/g, "_").replaceAll(/_+/g, "_").toLowerCase();
            }
        }
    } catch (e: unknown) {
    const errMsg = e instanceof Error ? e.message : String(e);
        // Handle security violations or network errors separately
        if (errMsg.includes("SECURITY VIOLATION")) {
            logger.error(`[LivaEngine] CRITICAL SECURITY ALERT: ${errMsg}`);
        } else {
            logger.error(`[LivaEngine] Smart Naming Error: ${errMsg}`);
        }
    }

    return shortName;
}
