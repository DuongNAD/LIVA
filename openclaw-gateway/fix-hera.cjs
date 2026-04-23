const fs = require('fs');

let content = fs.readFileSync('src/memory/HeraCompass.ts', 'utf8');

content = content.replace(
    /catch\s*\(\s*e\s*\)\s*\{\s*logger\.error\("\[HeraCompass\] Lỗi nạp Database Kinh nghiệm:",\s*e\);\s*\}/,
    'catch (e: any) { logger.error(e, "[HeraCompass] Lỗi nạp Database Kinh nghiệm:"); }'
);

content = content.replace(
    /catch\s*\(\s*e\s*\)\s*\{\s*logger\.error\("\[HeraCompass\] Lỗi lưu Database Kinh nghiệm:",\s*e\);\s*\}/,
    'catch (e: any) { logger.error(e, "[HeraCompass] Lỗi lưu Database Kinh nghiệm:"); }'
);

content = content.replace(
    /catch\s*\(\s*e\s*\)\s*\{\s*logger\.error\(`\[HeraCompass\] Lỗi gọi E4B Extractor:`, e\);\s*return null;\s*\}/,
    'catch (e: any) { logger.error(e, `[HeraCompass] Lỗi gọi E4B Extractor:`); return null; }'
);

content = content.replace(
    /public getRelatedInsight\(failedContext: string, toolTarget: string\): HeraInsight \| null \{[\s\S]*?return null;\n    \}/,
    `public getRelatedInsight(failedContext: string, toolTarget: string, options: { limit?: number, minScore?: number } = {}): HeraInsight[] {
        if (!this.flexIndex || this.insights.length === 0) return [];
        const limit = options.limit || 2;
        const minScore = options.minScore || 0;
        
        // Flexsearch returns multiple field arrays: [{ field: "error_trace", result: ["id1"] }]
        const results = this.flexIndex.search(failedContext, 5) as any[]; 
        
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
    }`
);

fs.writeFileSync('src/memory/HeraCompass.ts', content);
console.log('Fixed HeraCompass.ts');
