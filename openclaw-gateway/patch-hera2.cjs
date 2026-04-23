const fs = require('fs');

let content = fs.readFileSync('src/memory/HeraCompass.ts', 'utf8');

const startStr = '    public getRelatedInsight(failedContext: string, toolTarget: string): HeraInsight | null {';
const endStr = '        return null;\r\n    }';
const endStr2 = '        return null;\n    }';

let startIndex = content.indexOf(startStr);
let endIndex = content.indexOf(endStr, startIndex);
if (endIndex === -1) {
    endIndex = content.indexOf(endStr2, startIndex);
    if (endIndex !== -1) endIndex += endStr2.length;
} else {
    endIndex += endStr.length;
}

if (startIndex !== -1 && endIndex !== -1) {
    const replacement = `    public getRelatedInsight(failedContext: string, toolTarget: string, options: { limit?: number, minScore?: number } = {}): HeraInsight[] {
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
    }`;
    
    content = content.substring(0, startIndex) + replacement + content.substring(endIndex);
    fs.writeFileSync('src/memory/HeraCompass.ts', content);
    console.log("Patched successfully!");
} else {
    console.log("Could not find start or end strings.", startIndex, endIndex);
}
