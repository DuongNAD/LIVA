import OpenAI from "openai";
import { StructuredMemory } from "../memory/StructuredMemory";
import { EmbeddingService } from "../services/EmbeddingService";
import { evoLogger } from "./EvolutionLogger";
import { EvolutionContext } from "./types";

const EXPERT_API_URL = "http://127.0.0.1:8001/v1";

// [v19] Lazy singleton for StructuredMemory (will be replaced by DI)
let _sm: StructuredMemory | null = null;
const getSM = async (): Promise<StructuredMemory> => {
    if (!_sm) _sm = await StructuredMemory.create("liva_core");
    return _sm;
};

export class KnowledgeDistiller {
    static async run(ctx: EvolutionContext) {
        evoLogger.info(`[KnowledgeDistiller] Đang trích xuất Lịch sử Tiến Hóa đa chiều từ sqlite-vec...`);
        const sm = await getSM();
        const embedding = EmbeddingService.getInstance();

        let pastExperiences = "";
        try {
            // Search for episodic memories via vector search
            const queryVec = await embedding.embed("evolution experience dead-end success");
            const episodes = await sm.searchSimilarVectors(queryVec, 20);
            for (const ep of episodes) {
                pastExperiences += `[${ep.type}] TARGET: ${ep.domain}\n${ep.content}\n---\n`;
                if (ep.type === "DEAD-END" || ep.type === "SUCCESS") {
                    ctx.blacklistFiles.push(ep.domain);
                }
            }
            ctx.blacklistFiles = [...new Set(ctx.blacklistFiles)].slice(0, 14);
        } catch {
            pastExperiences = "Chưa có kinh nghiệm nào. Đây là lần tiến hóa đầu tiên.";
        }

        if (pastExperiences.length >= 2500) {
            await this.distillKnowledge(pastExperiences, sm, embedding);
        }
        ctx.pastExperiences = pastExperiences;

        let axioms = "";
        try {
            const axiomVec = await embedding.embed(ctx.projectSurfaceInfo.slice(0, 500));
            const relevantAxioms = await sm.searchSimilarVectors(axiomVec, 5, 'AXIOM');
            axioms = relevantAxioms.map(a => a.content).join("\n");
            if (!axioms) axioms = "No strict axioms defined yet.";
        } catch (e) { void e; }
        ctx.axioms = axioms;
    }

    static async distillKnowledge(rawJournal: string, memory: StructuredMemory, embeddingService: EmbeddingService) {
        evoLogger.warn(`[Lò Luyện Đan] Lịch sử Tiến Hóa đã quá Dày! Kích hoạt thuật toán Chưng Cất Tri Thức...`);
        
        const aiClient = new OpenAI({ baseURL: EXPERT_API_URL, apiKey: "liva-ghost-expert" });
        let existAxioms = "";
        try {
            const axiomVec = await embeddingService.embed("CORE_ARCHITECTURE TYPESCRIPT_SAFETY");
            const results = await memory.searchSimilarVectors(axiomVec, 15, 'AXIOM');
            existAxioms = results.map(r => r.content).join('\n');
        } catch (e) { void e; }

        const prompt = `From the following evolution experience, FILTER OUT obsolete/conflicting rules and FUSE them together.
[CURRENT RULESET]:\n${existAxioms}
[NEW HISTORY]:\n${rawJournal.slice(-4000)}

MISSION: Distill exactly 15 core Optimal Algorithmic Commands and the most rigorous lessons learned (Coding Guidelines & Gotchas). 
MUST BE DIVIDED INTO 3 GROUPS (Using Markdown Headings): 
- [CORE_ARCHITECTURE]
- [TYPESCRIPT_SAFETY]
- [LIVA_SPECIFIC]
Return neat Markdown format.`;
        
        try {
            const response = await aiClient.chat.completions.create({
                model: "expert",
                temperature: 0.1,
                max_tokens: 1500,
                stop: ["<start_of_turn>", "<end_of_turn>", "```\n\nWait", "Wait, I see"],
                messages: [{ role: "user", content: `System: You are the Axiomatic Compressor AI - The Prime Memory.\n\n${prompt}` }]
            });
            
            let newAxioms = response.choices[0]?.message?.content || "";
            
            const vec = await embeddingService.embed(newAxioms.substring(0, 500));
            memory.upsertVector({
                vecId: `axiom_distilled_${Date.now()}`,
                type: 'AXIOM',
                content: newAxioms,
                vector: vec,
                domain: 'SYSTEM_CORE',
            });

            // Clear old episodic memories (SUCCESS/DEAD-END only)
            // Note: In v19, we don't have clearEpisodicMemories — this is handled by GC in ConsolidationCron
            
            evoLogger.info(`[Trí Nhớ Tiên Đề] Chưng cất thành công! AXIOM ĐÃ ĐƯỢC NHÚNG VÀO sqlite-vec.`);
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            evoLogger.error({ err: errMsg }, `[Trí Nhớ Tiên Đề] Lỗi chưng cất`);
        }
    }
}
