import { logger } from "../utils/logger";
import { safeFetch } from "../utils/HttpClient";

const PROXY_URL = "http://localhost:8081/v1/chat/completions";

export class GeminiAPI {
    /**
     * Generate text using the local gemini-web2api proxy.
     */
    static async generateText(prompt: string, model: string = "gemini-3.5-flash"): Promise<string> {
        try {
            const response = await safeFetch(PROXY_URL, {
                method: "POST",
                headers: {
                    "Content-Type": "application/json"
                },
                body: JSON.stringify({
                    model: model,
                    messages: [{ role: "user", content: prompt }]
                })
            }, 30000); // 30s timeout
            
            const data = await response.json();
            return data.choices?.[0]?.message?.content || "";
        } catch (error) {
            logger.error(`[GeminiAPI] Failed to generate text via proxy: ${error}`);
            throw error;
        }
    }

    /**
     * Generate text with structured JSON output via tool calling.
     * Note: gemini-web2api supports function calling, so we use it to enforce JSON schema.
     */
    static async generateStructured<T>(prompt: string, schema: any, model: string = "gemini-3.5-flash"): Promise<T> {
        try {
            const response = await safeFetch(PROXY_URL, {
                method: "POST",
                headers: {
                    "Content-Type": "application/json"
                },
                body: JSON.stringify({
                    model: model,
                    messages: [{ role: "user", content: prompt }],
                    tools: [{
                        type: "function",
                        function: {
                            name: "return_json",
                            description: "Return the requested JSON data",
                            parameters: schema
                        }
                    }],
                    tool_choice: { type: "function", function: { name: "return_json" } }
                })
            }, 30000);
            
            const data = await response.json();
            const toolCall = data.choices?.[0]?.message?.tool_calls?.[0];
            if (toolCall?.function?.arguments) {
                return JSON.parse(toolCall.function.arguments) as T;
            }
            return {} as T;
        } catch (error) {
            logger.error(`[GeminiAPI] Failed to generate structured data via proxy: ${error}`);
            throw error;
        }
    }

    /**
     * NOTE: Multimodal image analysis is NOT supported by gemini-web2api.
     * Attempting to use this will throw an error.
     */
    static async analyzeImage(base64Image: string, mimeType: string, prompt: string, model: string = "gemini-3.5-flash"): Promise<string> {
        throw new Error("Multimodal image input is not supported by the local gemini-web2api proxy.");
    }
}
