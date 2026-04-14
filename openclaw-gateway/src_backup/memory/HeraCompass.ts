import * as fs from "fs";
import * as path from "path";
import Fuse from "fuse.js";
import { logger } from "../utils/logger";
import OpenAI from "openai";
import { v4 as uuidv4 } from "uuid";

export interface HeraInsight {
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
    private fuseIndex: Fuse<HeraInsight> | null = null;
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
        } catch (e) {
            logger.error("[HeraCompass] Lỗi nạp Database Kinh nghiệm:", e);
        }
    }

    private rebuildIndex() {
        // Chỉ lập chỉ mục tối đa 500 insight mới nhất (utility > -2) để chống nghẽn Event Loop O(1)
        const validInsights = this.insights.filter(i => i.utility_score > -2).slice(-500);
        this.fuseIndex = new Fuse(validInsights, {
            keys: ['error_trace', 'actionable_rule', 'tool_target'],
            shouldSort: true,
            threshold: 0.4,
            includeScore: true
        });
    }

    private saveDebounced() {
        if (this.saveTimeout) clearTimeout(this.saveTimeout);
        this.saveTimeout = setTimeout(() => {
            try {
                fs.writeFileSync(this.dbPath, JSON.stringify(this.insights, null, 2), "utf-8");
                this.rebuildIndex();
                logger.info(`💾 [HeraCompass] Đã đồng bộ ${this.insights.length} kinh nghiệm xuống ổ cứng.`);
            } catch (e) {
                logger.error("[HeraCompass] Lỗi lưu Database Kinh nghiệm:", e);
            }
        }, 5000); // Debounce 5s
    }

    // [Defensive Parsing] Bóc tách JSON an toàn khỏi Rác Markdown
    private extractJSON<T>(text: string): T | null {
        const match = text.match(/\{[\s\S]*\}/);
        if (!match) return null;
        try {
            return JSON.parse(match[0]);
        } catch (e) {
            return null;
        }
    }

    /**
     * [Hook 1] RAG Retrieval - Bốc 1 kinh nghiệm gần nhất
     */
    public getRelatedInsight(failedContext: string, toolTarget: string): HeraInsight | null {
        if (!this.fuseIndex || this.insights.length === 0) return null;
        
        const results = this.fuseIndex.search(failedContext);
        
        for (const res of results) {
            if (res.item.tool_target === toolTarget || !res.item.tool_target) {
                return res.item;
            }
        }
        return null;
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
        } catch (e) {
            logger.error(`[HeraCompass] Lỗi gọi E4B Extractor:`, e);
            return null;
        }
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
