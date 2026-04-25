import OpenAI from "openai";
import { LanceMemoryManager } from "../memory/LanceMemory";
import { evoLogger } from "./EvolutionLogger";
import { EvolutionContext } from "./types";

const EXPERT_API_URL = "http://127.0.0.1:8001/v1";
const GLOBAL_MEMORY = new LanceMemoryManager();
let isMemoryConnected = false;

export class KnowledgeDistiller {
    static async run(ctx: EvolutionContext) {
        evoLogger.info(`[KnowledgeDistiller] Đang trích xuất Lịch sử Tiến Hóa đa chiều từ LanceDB...`);
        if (!isMemoryConnected) {
            await GLOBAL_MEMORY.connect();
            isMemoryConnected = true;
        }

        let pastExperiences = "";
        try {
            const episodes = await GLOBAL_MEMORY.getAllEpisodicMemories();
            for (const ep of episodes) {
                pastExperiences += `[${ep.type}] TARGET: ${ep.fileTarget}\n${ep.text}\n---\n`;
                if (ep.type === "DEAD-END" || ep.type === "SUCCESS") {
                    ctx.blacklistFiles.push(ep.fileTarget);
                }
            }
            ctx.blacklistFiles = [...new Set(ctx.blacklistFiles)].slice(0, 14);
        } catch {
            pastExperiences = "Chưa có kinh nghiệm nào. Đây là lần tiến hóa đầu tiên.";
        }

        if (pastExperiences.length >= 2500) {
            await this.distillKnowledge(pastExperiences, GLOBAL_MEMORY);
        }
        ctx.pastExperiences = pastExperiences;

        let axioms = "";
        try {
            const relevantAxiomTags = await GLOBAL_MEMORY.searchMemory(ctx.projectSurfaceInfo.slice(0, 500), 5);
            axioms = relevantAxiomTags.join("\n");
            if (!axioms) axioms = "No strict axioms defined yet.";
        } catch (e) { void e; }
        ctx.axioms = axioms;
    }

    static async distillKnowledge(rawJournal: string, memory: LanceMemoryManager) {
        evoLogger.warn(`[Lò Luyện Đan] Lịch sử Tiến Hóa đã quá Dày! Kích hoạt thuật toán Chưng Cất Tri Thức...`);
        
        const aiClient = new OpenAI({ baseURL: EXPERT_API_URL, apiKey: "liva-ghost-expert" });
        let existAxioms = "";
        try {
            existAxioms = (await memory.searchMemory("CORE_ARCHITECTURE TYPESCRIPT_SAFETY", 15)).join('\n');
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
            
            await memory.addMemory("AXIOM", newAxioms, "SYSTEM_CORE");
            await memory.clearEpisodicMemories();
            
            evoLogger.info(`[Trí Nhớ Tiên Đề] Chưng cất thành công! Rác Log bị làm trống, AXIOM ĐÃ ĐƯỢC NHÚNG VÀO LANCEDB.`);
        } catch (e: any) {
            evoLogger.error({ err: e }, `[Trí Nhớ Tiên Đề] Lỗi chưng cất`);
        }
    }
}
