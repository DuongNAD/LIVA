import { StructuredMemory } from "./StructuredMemory";
import { logger } from "../utils/logger";
import OpenAI from "openai";

/**
 * PersonalKnowledgeExtractor — AI Auto-Extraction of Personal Facts
 * ==================================================================
 * After every conversation turn, this module asks the Router AI to
 * identify personal facts about the user and automatically stores
 * them in StructuredMemory.
 * 
 * What it detects:
 *   - Người thân: "Mẹ tên Lan", "Em gái học lớp 5"
 *   - Sở thích: "Thích cà phê đen", "Fan MU"
 *   - Thói quen: "6h sáng chạy bộ", "Tối thứ 7 xem phim"
 *   - Công việc: "Đang làm dự án LIVA", "Sếp tên Hùng"
 *   - Sự kiện: "Sinh nhật 15/3", "Họp deadline thứ 6"
 *   - Cảm xúc: "Hôm nay buồn", "Đang stress deadline"
 * 
 * Philosophy: User should NEVER need to say "hãy nhớ..." — AI just knows.
 */

// Extraction prompt — carefully tuned to extract personal facts
const EXTRACTION_PROMPT = `Bạn là một hệ thống trích xuất kiến thức cá nhân. Phân tích đoạn hội thoại sau và trích xuất các THÔNG TIN CÁ NHÂN của người dùng (KHÔNG phải thông tin chung).

Các loại thông tin cần tìm:
- Người thân: tên, mối quan hệ, thông tin về họ
- Sở thích: đồ ăn, âm nhạc, phim, thể thao, sách, game...  
- Thói quen: lịch trình, routine hàng ngày, cách làm việc
- Công việc: nghề nghiệp, dự án, đồng nghiệp, sếp
- Sự kiện: sinh nhật, kỷ niệm, deadline, lịch hẹn
- Cảm xúc: tâm trạng hiện tại, lo lắng, vui mừng

TRẢ LỜI ĐÚNG ĐỊNH DẠNG JSON ARRAY. Mỗi item gồm: key (snake_case ngắn gọn), value (nội dung), category (1 trong: Người thân, Sở thích, Thói quen, Công việc, Sự kiện, Cảm xúc).

Nếu KHÔNG có thông tin cá nhân nào, trả về: []

Ví dụ output:
[{"key": "ten_me", "value": "Mẹ tên Lan, thích nấu ăn", "category": "Người thân"}, {"key": "so_thich_ca_phe", "value": "Thích uống cà phê đen buổi sáng", "category": "Sở thích"}]

QUAN TRỌNG: Chỉ trích xuất SỰ THẬT CỤ THỂ, không suy đoán. Trả về JSON thuần, KHÔNG markdown.`;

export class PersonalKnowledgeExtractor {
    private structuredMemory: StructuredMemory;
    private aiClient: OpenAI;
    private extractionCount = 0;

    constructor(structuredMemory: StructuredMemory, aiClient: OpenAI) {
        this.structuredMemory = structuredMemory;
        this.aiClient = aiClient;
    }

    /**
     * Extract personal facts from a conversation turn and store them.
     * Called asynchronously after every AI response (fire-and-forget).
     */
    async extractAndStore(userMessage: string, aiReply: string): Promise<void> {
        try {
            // Skip very short messages or system messages
            if (!userMessage || userMessage.length < 10) return;
            // Skip greetings/trivial messages
            if (/^(hi|hello|xin chào|chào|hey|ok|oke|được|vâng|dạ)\s*$/i.test(userMessage.trim())) return;

            const conversationSnippet = `Người dùng: ${userMessage}\nLIVA: ${aiReply}`;

            const response = await this.aiClient.chat.completions.create({
                model: "router",
                messages: [
                    { role: "system", content: EXTRACTION_PROMPT },
                    { role: "user", content: conversationSnippet }
                ],
                temperature: 0.1,
                max_tokens: 500,
            });

            const raw = response.choices[0]?.message?.content?.trim();
            if (!raw || raw === "[]" || raw.length < 5) return;

            // Parse JSON response
            let facts: Array<{ key: string; value: string; category: string }>;
            try {
                // Handle potential markdown wrapping
                const jsonStr = raw.replace(/^```json?\n?/, "").replace(/\n?```$/, "").trim();
                facts = JSON.parse(jsonStr);
            } catch {
                logger.warn(`[PersonalKnowledge] JSON parse failed, skipping: ${raw.substring(0, 100)}`);
                return;
            }

            if (!Array.isArray(facts) || facts.length === 0) return;

            // Store each fact
            let storedCount = 0;
            for (const fact of facts) {
                if (!fact.key || !fact.value || !fact.category) continue;

                // Validate category
                const validCategories = ["Người thân", "Sở thích", "Thói quen", "Công việc", "Sự kiện", "Cảm xúc"];
                const category = validCategories.includes(fact.category) ? fact.category : "Chung";

                // Set TTL based on category
                let ttlDays: number | undefined;
                if (category === "Cảm xúc") ttlDays = 7;       // Emotions fade after 1 week
                else if (category === "Sự kiện") ttlDays = 30;  // Events expire after 1 month
                // Others: permanent (no TTL)

                this.structuredMemory.setFact(fact.key, fact.value, {
                    source: "auto_extract",
                    category,
                    ttlDays
                });
                storedCount++;
            }

            if (storedCount > 0) {
                this.extractionCount += storedCount;
                logger.info(`[PersonalKnowledge] Đã tự động ghi nhớ ${storedCount} thông tin cá nhân (Tổng: ${this.extractionCount})`);
            }
        } catch (error: any) {
            // Never crash the main flow — extraction is best-effort
            logger.warn(`[PersonalKnowledge] Extraction failed (non-critical): ${error.message}`);
        }
    }

    /**
     * Get total number of facts extracted in this session
     */
    get totalExtracted(): number {
        return this.extractionCount;
    }
}
