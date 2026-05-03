import { StructuredMemory } from "./StructuredMemory";
import { logger } from "../utils/logger";
import OpenAI from "openai";
import type { MemoryRoute } from "./SemanticRouter";

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
 *
 * [v4.0] Optimizations:
 *   - Route-based filtering: skip system_command / deep_reasoning (G-5)
 *   - Buffered micro-batching: accumulate turns, extract when idle 60s or buffer >200 chars
 *   - Fact Reconciliation: soft-deprecate conflicting facts (G-9)
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

TRẢ LỜI ĐÚNG ĐỊNH DẠNG JSON ARRAY. Mỗi item gồm: key (snake_case ngắn gọn), value (nội dung), category (1 trong: Người thân, Sở thích, Thói quen, Công việc, Sự kiện, Cảm xúc), replaces_key (nếu thông tin này CẬP NHẬT/THAY THẾ một fact cũ, ghi key cũ ở đây, nếu không thì null).

Nếu KHÔNG có thông tin cá nhân nào, trả về: []

Ví dụ output:
[{"key": "ten_me", "value": "Mẹ tên Lan, thích nấu ăn", "category": "Người thân", "replaces_key": null}, {"key": "cong_ty_hien_tai", "value": "Đang làm ở Viettel", "category": "Công việc", "replaces_key": "cong_ty_hien_tai"}]

QUAN TRỌNG: Chỉ trích xuất SỰ THẬT CỤ THỂ, không suy đoán. Trả về JSON thuần, KHÔNG markdown.`;

/** [v4.0] Routes where PKE extraction is skipped to save tokens */
const SKIP_ROUTES: MemoryRoute[] = ["system_command", "deep_reasoning", "tool_recall"];

export class PersonalKnowledgeExtractor {
    private readonly structuredMemory: StructuredMemory;
    private readonly aiClient: OpenAI;
    private extractionCount = 0;

    // [v4.0] Buffered micro-batching state
    private pendingBuffer: string[] = [];
    private idleTimer: NodeJS.Timeout | null = null;

    constructor(structuredMemory: StructuredMemory, aiClient: OpenAI) {
        this.structuredMemory = structuredMemory;
        this.aiClient = aiClient;
    }

    /**
     * [v4.0] Queue a turn for extraction with route-based filtering
     * and buffered micro-batching. Replaces direct extractAndStore() calls.
     * 
     * @param route - SemanticRouter route classification for this turn
     */
    public queueForExtraction(userMessage: string, aiReply: string, route?: MemoryRoute): void {
        // [v4.0] G-5: Skip routes that rarely contain personal facts
        if (route && SKIP_ROUTES.includes(route)) return;
        // Skip very short or trivial messages
        if (!userMessage || userMessage.length < 10) return;
/* istanbul ignore next */
        if (/^(hi|hello|xin chào|chào|hey|ok|oke|được|vâng|dạ)\s*$/i.test(userMessage.trim())) return;

        // Buffer the turn
        this.pendingBuffer.push(`Người dùng: ${userMessage}\nLIVA: ${aiReply}`);

        // Reset idle timer
        if (this.idleTimer) clearTimeout(this.idleTimer);
        this.idleTimer = setTimeout(() => this.flushBuffer(), 60_000); // Idle 60s → flush

        // Flush immediately if buffer is large enough
        const totalLength = this.pendingBuffer.reduce((sum, s) => sum + s.length, 0);
        if (totalLength > 200) {
            this.flushBuffer();
        }
    }

    /**
     * [v4.0] Flush the buffer and run extraction on accumulated turns.
     */
    private flushBuffer(): void {
        if (this.idleTimer) {
            clearTimeout(this.idleTimer);
            this.idleTimer = null;
        }
        if (this.pendingBuffer.length === 0) return;

        const batchedContent = this.pendingBuffer.join("\n---\n");
        this.pendingBuffer = [];

        // Fire-and-forget extraction
        this.extractAndStore(batchedContent).catch((e: any) => {
            logger.warn(`[PersonalKnowledge] Batch extraction failed (non-critical): ${e.message}`);
        });
    }

    /**
     * Extract personal facts from conversation content and store them.
     * [v4.0] Now called via flushBuffer() with batched content.
     */
    async extractAndStore(conversationSnippet: string): Promise<void> {
        try {
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
            let facts: Array<{ key: string; value: string; category: string; replaces_key?: string | null }>;
            try {
                const jsonStr = raw.replace(/^```json?\n?/, "").replace(/\n?```$/, "").trim();
                facts = JSON.parse(jsonStr);
            } catch {
                logger.warn(`[PersonalKnowledge] JSON parse failed, skipping: ${raw.substring(0, 100)}`);
                return;
            }

/* istanbul ignore next */
            if (!Array.isArray(facts) || facts.length === 0) return;

            // Store each fact
            let storedCount = 0;
            for (const fact of facts) {
/* istanbul ignore next */
                if (!fact.key || !fact.value || !fact.category) continue;

                // Validate category
                const validCategories = ["Người thân", "Sở thích", "Thói quen", "Công việc", "Sự kiện", "Cảm xúc"];
/* istanbul ignore next */
                const category = validCategories.includes(fact.category) ? fact.category : "Chung";

                // Set TTL based on category
                let ttlDays: number | undefined;
/* istanbul ignore next */
                if (category === "Cảm xúc") ttlDays = 7;
/* istanbul ignore next */
                else if (category === "Sự kiện") ttlDays = 30;

                // [v4.0] G-9: Fact Reconciliation — soft-deprecate conflicting fact
                if (fact.replaces_key && fact.replaces_key !== fact.key) {
                    this.structuredMemory.setFactImportance(fact.replaces_key, 0.1);
                    logger.info(`[PersonalKnowledge/Reconciliation] Deprecated old fact: "${fact.replaces_key}" (replaced by "${fact.key}")`);
                }

                this.structuredMemory.setFact(fact.key, fact.value, {
                    source: "auto_extract",
                    category,
                    ttlDays
                });
                storedCount++;
            }

/* istanbul ignore next */
            if (storedCount > 0) {
                this.extractionCount += storedCount;
                logger.info(`[PersonalKnowledge] Đã tự động ghi nhớ ${storedCount} thông tin cá nhân (Tổng: ${this.extractionCount})`);
            }
        } catch (error: any) {
            logger.warn(`[PersonalKnowledge] Extraction failed (non-critical): ${error.message}`);
        }
    }

    /** Get total number of facts extracted in this session */
    get totalExtracted(): number {
        return this.extractionCount;
    }

    /** [v4.0] Cleanup timers on shutdown */
    public dispose(): void {
        if (this.idleTimer) {
            clearTimeout(this.idleTimer);
            this.idleTimer = null;
        }
        // Flush remaining buffer before shutdown
        this.flushBuffer();
    }
}
