import * as fs from "node:fs";
import { promises as fsp } from "node:fs";
import * as path from "node:path";
import { Document } from "flexsearch";
import { logger } from "../utils/logger";
import OpenAI from "openai";
import { v4 as uuidv4 } from "uuid";
import { jsonrepair } from "jsonrepair";

export interface HeraInsight {
    [key: string]: any;
    insight_id: string;
    tool_target: string;
    actionable_rule: string;
    error_trace: string;
    utility_score: number;
    status: "Draft" | "Verified";
}

export class HeraCompass {
    private static instance: HeraCompass;
    private dbPath: string;
    private insights: HeraInsight[] = [];
    private flexIndex: InstanceType<typeof Document> | null = null;
    private saveTimeout: NodeJS.Timeout | null = null;

    private constructor() {
        this.dbPath = path.join(process.cwd(), "data", "agents", "liva_core", "hera_insights.json");
        this.loadInsights();
    }

    public static getInstance(): HeraCompass {
        if (!HeraCompass.instance) {
            HeraCompass.instance = new HeraCompass();
        }
        return HeraCompass.instance;
    }

    // TODO: [Tech Debt] loadInsights() uses blocking fs.readFileSync/mkdirSync/existsSync.
    // Per AI_CONTEXT.md §4.3, blocking I/O is BANNED on the main thread.
    // Impact: runs only once at startup, so acceptable for now.
    // Fix: Convert to async factory pattern (static async create()) in next refactor.
    private loadInsights() {
        try {
            const dir = path.dirname(this.dbPath);
            if (!fs.existsSync(dir)) fs.mkdirSync(dir, { recursive: true });

            if (fs.existsSync(this.dbPath)) {
                const data = fs.readFileSync(this.dbPath, "utf-8");
                this.insights = JSON.parse(data);
            } else {
                this.insights = [];
            }
            this.rebuildIndex();
        } catch (e: any) { logger.error(e, "[HeraCompass] Lỗi nạp Database Kinh nghiệm:"); }
    }

    private rebuildIndex() {
        // Chỉ lập chỉ mục tối đa 500 insight mới nhất (utility > -2) để chống nghẽn Event Loop O(1)
        const validInsights = this.insights.filter(i => i.utility_score > -2).slice(-500);
        this.flexIndex = new Document({
            document: {
                id: "insight_id",
                index: ["error_trace", "actionable_rule", "tool_target"]
            },
            tokenize: "forward",
            resolution: 9
        });

        for (const item of validInsights) {
             this.flexIndex.add(item);
        }
    }

    private saveDebounced() {
        if (this.saveTimeout) clearTimeout(this.saveTimeout);
        this.saveTimeout = setTimeout(async () => {
            try {
                // 🔒 [Audit Fix M-2] Atomic Write: ghi ra .tmp trước, rename đè lên file thật
                // Ngăn chặn corrupt file nếu I/O bị gán chồng debounce
                const tmpPath = `${this.dbPath}.tmp`;
                const data = JSON.stringify(this.insights, null, 2);
                await fsp.writeFile(tmpPath, data, "utf-8");
                await fsp.rename(tmpPath, this.dbPath); // rename là Atomic trên cùng filesystem
                this.rebuildIndex();
                logger.info(`💾 [HeraCompass] Đã đồng bộ ${this.insights.length} kinh nghiệm xuống ổ cứng (Atomic Write).`);
            } catch (e: any) { logger.error(e, "[HeraCompass] Lỗi lưu Database Kinh nghiệm:"); }
        }, 5000); // Debounce 5s
    }

    // [Defensive Parsing] Bóc tách JSON an toàn khỏi Rác Markdown
    private extractJSON<T>(text: string): T | null {
        const first = text.indexOf('{');
        const last = text.lastIndexOf('}');
        if (first === -1 || last === -1 || last < first) return null;
        try {
            const raw = text.substring(first, last + 1);
            return JSON.parse(jsonrepair(raw));
        } catch {
            return null;
        }
    }

    /**
     * [Hook 1] RAG Retrieval - Bốc 1 kinh nghiệm gần nhất
     */
    public getRelatedInsight(failedContext: string, toolTarget: string, options: { limit?: number, minScore?: number } = {}): HeraInsight[] {
        if (!this.flexIndex || this.insights.length === 0) return [];
        const limit = options.limit || 2;
        const minScore = options.minScore || 0;
        
        // Flexsearch returns multiple field arrays: [{ field: "error_trace", result: ["id1"] }]
        const results = (this.flexIndex.search(failedContext, 5) || []) as any[]; 
        
        const uniqueIds = new Set<string>();
        for (const fieldResult of results) {
            for (const id of fieldResult.result) {
                uniqueIds.add(id as string);
            }
        }
        
        const matchedInsights: HeraInsight[] = [];
        for (const targetId of uniqueIds) {
            const item = this.insights.find(i => i.insight_id === targetId);
            if (item && (item.tool_target === toolTarget || !item.tool_target) && item.utility_score >= minScore) {
                matchedInsights.push(item);
                if (matchedInsights.length >= limit) break;
            }
        }
        return matchedInsights;
    }

    /**
     * [Hook 2] Asymmetric Orchestration - Gọi E4B sinh Insight không tắt 26B
     */
    public async learnFromError(
        routerClient: OpenAI, 
        toolTarget: string,
        failedActionContext: string, 
        execErr: string
    ): Promise<string | null> {
        // Mượt mà trôi qua nếu lỗi không có Trace vật lý cụ thể (VD: người dùng dừng lệnh)
        if (!execErr || execErr.length < 10) return null;
        
        logger.warn(`🧠 [HeraCompass/RoPE] Đang đúc kết La bàn Kinh nghiệm từ Lỗi...`);

        try {
            const prompt = `System: You are HERA. Analyze the error and provide ONE rule (under 15 words) to prevent it. 
Output FORMAT EXACTLY like this:
RULE: <your_short_rule>

Action: ${failedActionContext.substring(0, 500)}
Error: ${execErr.substring(0, 800)}`;

            const res = await routerClient.chat.completions.create({
                model: "router",
                messages: [{ role: "user", content: prompt }],
                max_tokens: 30, // Khóa cứng giới hạn siêu nhỏ
                temperature: 0.1, // Zero creativity, purely diagnostic
            });

            const text = res.choices[0].message?.content || "";
            const match = text.match(/RULE:\s*(.*)/i);
            
            if (match && match[1]) {
                const newInsight: HeraInsight = {
                    insight_id: uuidv4(),
                    tool_target: toolTarget,
                    actionable_rule: match[1].trim(),
                    error_trace: execErr.substring(0, 100),
                    utility_score: 0,
                    status: "Draft"
                };

                this.insights.push(newInsight);
                this.saveDebounced();
                logger.info(`🟢 [HeraCompass] Đã sinh Insight Nháp: "${newInsight.actionable_rule}"`);
                return newInsight.insight_id;
            } else {
                logger.warn(`🔴 [HeraCompass] E4B sinh rác, huỷ lưu Insight.`);
                return null;
            }
        } catch (e: any) { logger.error(e, `[HeraCompass] Lỗi gọi E4B Extractor:`); return null; }
    }

    /**
     * [Hook 3] Decaying Utility Score - Thanh lọc rác
     */
    public updateUtilityScore(insightId: string, isSuccess: boolean) {
        const idx = this.insights.findIndex(i => i.insight_id === insightId);
        if (idx === -1) return;

        const insight = this.insights[idx];
        if (isSuccess) {
            insight.utility_score += 1;
            insight.status = "Verified";
            logger.info(`📈 [HeraCompass] Thăng cấp Insight [${insight.actionable_rule.substring(0, 20)}...] (Score: ${insight.utility_score})`);
        } else {
            insight.utility_score -= 1;
            logger.warn(`📉 [HeraCompass] Khấu trừ điểm Insight [${insight.actionable_rule.substring(0, 20)}...] (Score: ${insight.utility_score})`);
            if (insight.utility_score <= -2) {
                this.insights.splice(idx, 1);
                logger.warn(`🗑️ [HeraCompass] Tiêu huỷ vĩnh viễn Insight rác!`);
            }
        }
        this.saveDebounced();
    }
}
