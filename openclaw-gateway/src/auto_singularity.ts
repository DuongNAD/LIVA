import * as fs from "fs/promises";
import * as fsSync from "fs";
import * as path from "path";
import OpenAI from "openai";
import axios from "axios";
import { exec, spawn } from "child_process";
import { promisify } from "util";
import { SkillRegistry } from "./SkillRegistry";

const execAsync = promisify(exec);
const EXPERT_API_URL = "http://127.0.0.1:8001/v1";

const color = {
    green: (txt: string) => `\x1b[32m${txt}\x1b[0m`,
    cyan: (txt: string) => `\x1b[36m${txt}\x1b[0m`,
    red: (txt: string) => `\x1b[31m${txt}\x1b[0m`,
    yellow: (txt: string) => `\x1b[33m${txt}\x1b[0m`,
    magenta: (txt: string) => `\x1b[35m${txt}\x1b[0m`,
};

async function sleep(ms: number) {
    return new Promise(r => setTimeout(r, ms));
}

// ==========================================
// CƠ CHẾ HOT-SWAPPING (TRÁO NÃO TRÊN TRAM)
// ==========================================
async function killPortWindows(port: number) {
    try {
        const { stdout } = await execAsync(`netstat -ano | findstr :${port}`);
        if (!stdout) return;
        const lines = stdout.trim().split("\n");
        // Chỉ quét tiến trình đang LISTENING thực sự để tránh diệt nhầm socket đang ngủ
        const listeningLine = lines.find(l => l.includes("LISTENING"));
        if (!listeningLine) return;

        const match = listeningLine.trim().split(/\s+/);
        const pid = match[match.length - 1];
        if (pid && parseInt(pid) > 0) {
            console.log(color.yellow(`[Hot-Swap] Tìm thấy tiến trình (PID: ${pid}) khóa cứng Cổng ${port}. Đề Nghị Tiêu Diệt...`));
            await execAsync(`taskkill /PID ${pid} /F`);
            console.log(color.green(`[Hot-Swap] Đã dọn dẹp sạch sẽ Cổng ${port}. VRAM được trả tự do.`));
        }
    } catch (e) {
        // Ignored. Có thể cổng đang không được dùng.
    }
}

async function startEngineWindows(fileName: string) {
    console.log(color.cyan(`[Hot-Swap] Kích nổ động cơ ${fileName}... (Chạy ngầm ẩn cửa sổ)`));
    const engineDir = path.join(process.cwd(), "..", "liva-ai-engine");
    const pythonPath = path.join(engineDir, "venv", "Scripts", "python.exe");
    
    const child = spawn(pythonPath, [fileName], {
        cwd: engineDir,
        windowsHide: true, // Tàng hình 100%, không mở thêm cửa sổ mới trống rỗng nào cả
        stdio: "ignore", // Triệt tiêu văng cửa sổ CMD mới
        env: { ...process.env, PYTHONIOENCODING: "utf-8" } // Chống crash Unicode chết yểu khi in Emoji
    });
    child.on('error', (err) => {
        console.error(color.red(`[Hot-Swap] Lỗi khởi động động cơ Python: ${err.message}`));
    });
    child.unref(); // Tách rời hoàn toàn khỏi Node
}

async function pingUvicorn(port: number, retries = 20): Promise<boolean> {
    for (let i = 0; i < retries; i++) {
        try {
            const resp = await fetch(`http://127.0.0.1:${port}`);
            if (resp.status) return true;
        } catch (e) {
            await sleep(2000); // 2s ping 1 lần
        }
    }
    return false;
}
// ==========================================


interface ScannedFile {
    path: string;
    exports: string[];
    weight: number;
}
async function extractProjectSurface(dirPath: string, blacklist: string[] = []): Promise<string> {
    const allFiles: ScannedFile[] = [];
    
    async function scan(currentDir: string) {
        try {
            const files = await fs.readdir(currentDir, { withFileTypes: true });
            for (const file of files) {
                if (file.name.startsWith("node_modules") || file.name.startsWith("dist") || file.name.startsWith(".git") || file.name.endsWith(".sandbox.ts") || file.name.endsWith(".bak")) {
                    continue;
                }
                const fullRelPath = path.relative(process.cwd(), path.join(currentDir, file.name)).replace(/\\/g, '/');
                if (blacklist.includes(fullRelPath)) continue;

                if (file.isDirectory()) {
                    await scan(path.join(currentDir, file.name));
                } else if (file.name.endsWith(".ts")) {
                    let weight = 1;
                    let exportsList: string[] = [];
                    try {
                        const stat = await fs.stat(path.join(currentDir, file.name));
                        const content = await fs.readFile(path.join(currentDir, file.name), "utf-8");
                        
                        // Đếm lượng từ khóa Logic (Cyclomatic Complexity)
                        const numIfs = (content.match(/if\s*\(/g) || []).length;
                        const numLoops = (content.match(/(for|while)\s*\(/g) || []).length;
                        const numFuncs = (content.match(/function\s+|=>|class\s+/g) || []).length;
                        const logicScore = (numIfs * 2) + (numLoops * 3) + (numFuncs * 2) + 1;
                        
                        weight = (stat.size / 1024) * logicScore; 
                        // Nếu file tĩnh không có class/function, gạch tên khỏi danh sách
                        if (numFuncs === 0) weight = weight * 0.01;

                        const matches = [...content.matchAll(/export\s+(class|interface|type|enum|function|const|let|var)\s+([A-Za-z0-9_]+)/g)];
                        exportsList = matches.map(m => m[2]);
                    } catch(e) {}
                    allFiles.push({ path: fullRelPath, exports: exportsList, weight });
                }
            }
        } catch (e) {}
    }

    await scan(dirPath);
    
    // Sort theo độ ưu tiên (Heuristic Scanner) & giữ Top 20
    allFiles.sort((a, b) => b.weight - a.weight);
    const topFiles = allFiles.slice(0, 20);

    let result = "Top 20 Tệp Lõi (Heuristic Ranked):\n";
    for(const f of topFiles) {
        result += `📄 ${f.path} (Size: ${Math.round(f.weight/1024)}KB)\n`;
        if (f.exports.length > 0) {
            result += `   └─ API Surface: ${f.exports.join(", ")}\n`;
        }
    }
    return result;
}

async function robustWebSearch(topic: string): Promise<string> {
    try {
        console.log(color.cyan(`\n[Web Intelligence]: Đang sục sạo Google/DuckDuckGo tìm kiếm: "${topic}"...`));
        const res = await axios.post("https://lite.duckduckgo.com/lite/", `q=${encodeURIComponent(topic)}`, {
            headers: {
                "Content-Type": "application/x-www-form-urlencoded",
                "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/114.0.0.0 Safari/537.36"
            },
            timeout: 10000
        });
        const html = res.data;
        const snippetMatches = [...html.matchAll(/<td class='result-snippet'>([\s\S]*?)<\/td>/gi)];
        let insights = "";
        snippetMatches.slice(0, 3).forEach((match, idx) => {
             insights += `- Ý Tưởng Web ${idx+1}: ${match[1].replace(/<\/?[^>]+(>|$)/g, "").trim()}\n`;
        });
        return insights || "Web Search thành công nhưng không tìm thấy đoạn Snippet phù hợp.";
    } catch (e: any) {
        console.log(color.red(`\x1b[90m[Web Intelligence]: Lỗi mạng rớt Web Search (${e.message}).\x1b[0m`));
        return "Lỗi mạng khi lấy dữ liệu Web.";
    }
}

async function distillKnowledge(journalPath: string, rawJournal: string) {
    if (rawJournal.length < 2500) return; // Chỉ chưng cất khi file đủ dài
    console.log(color.yellow("\n🔥 [Lò Luyện Đan]: Lịch sử Tiến Hóa đã quá Dày! Kích hoạt thuật toán Chưng Cất Tri Thức (Knowledge Distillation)..."));
    
    const axiomPath = path.join(process.cwd(), "data", "agents", "liva_core", "liva_core_axioms.md");
    
    // Nạp Não 26B để chưng cất
    const aiClient = new OpenAI({ baseURL: EXPERT_API_URL, apiKey: "liva-ghost-expert" });
    const existAxioms = await fs.readFile(axiomPath, "utf-8").catch(() => "Chưa có luật nào.");
    const prompt = `From the following evolution experience, FILTER OUT obsolete/conflicting rules and FUSE them together.
[CURRENT RULESET]:\n${existAxioms}
[NEW HISTORY]:\n${rawJournal.slice(-4000)}

MISSION: Distill exactly 15 core Optimal Algorithmic Commands and the most rigorous lessons learned (Coding Guidelines & Gotchas). 
MUST BE DIVIDED INTO 3 GROUPS (Using Markdown Headings): 
- [CORE_ARCHITECTURE]
- [TYPESCRIPT_SAFETY]
- [LIVA_SPECIFIC]
Return neat Markdown format. IMPORTANT: THE 15 AXIOMS AND RULES MUST BE WRITTEN IN ENGLISH.`;
    
    try {
        const response = await aiClient.chat.completions.create({
            model: "expert",
            temperature: 0.1,
            max_tokens: 1500,
            messages: [{ role: "system", content: "You are the Axiomatic Compressor AI - The Prime Memory." }, { role: "user", content: prompt }]
        });
        
        let newAxioms = response.choices[0]?.message?.content || "";
        
        // GHI ĐÈ BẢN MỚI thay vì Nối thêm
        await fs.writeFile(axiomPath, "# 🧬 LIVA CORE AXIOMS (Luật Vàng Tiến Hóa Bất Biến)\n\n" + newAxioms, "utf-8");
        
        // Cứu lại danh sách 40 file mục tiêu gần nhất để làm Mỏ Neo Trí Nhớ
        const targetMatches = [...rawJournal.matchAll(/TARGET:\s*(src[^\n\s]+)/g)];
        const uniqueTargets = [...new Set(targetMatches.map(m => m[1]))];
        const retainedBlacklist = uniqueTargets.slice(-40).map(t => `[ARCHIVED] TARGET: ${t}`).join("\n");

        // Xóa rác, nhưng giữ lại Blacklist để LIVA không dẫm vào vết xe đổ
        await fs.writeFile(journalPath, retainedBlacklist + "\n\n--- [BẮT ĐẦU CHU KỲ MỚI] ---\n", "utf-8");
        console.log(color.green("✅ [Trí Nhớ Tiên Đề]: Chưng cất thành công! Rác Log bị làm trống (đã lưu Blacklist), File Axioms ĐÃ ĐƯỢC DUNG HỢP."));
    } catch (e) {
        console.log(color.red("⛔ Lỗi chưng cất: " + e));
    }
}

async function autoSingularitySequence() {
    console.log(color.green("================================================================"));
    console.log(color.green(" 🚀 [LIVA SINGULARITY DAEMON] - CHU TRÌNH TỰ TIẾN HÓA KÍCH HOẠT"));
    console.log(color.green("================================================================\n"));

    console.log(color.magenta(`[HOT-SWAP] TẠM NGƯNG HỆ THỐNG ZALO BOT. THU HỒI VRAM TỪ NÃO E4B...`));
    await killPortWindows(8000); // Giết E4B
    await killPortWindows(8001); // Dọn sạch tiến trình rác của 26B nếu còn sót
    await sleep(2000);

    console.log(color.magenta(`[HOT-SWAP] TRÁO NÃO 26B VÀO CỔNG 8001 VÀ CHIẾM 16GB VRAM...`));
    await startEngineWindows("expert_engine.py"); // Nổ 26B
    
    console.log(color.yellow(`[HOT-SWAP] Đang đợi Não 26B khởi động động cơ Uvicorn (Có thể mất 15-20s)...`));
    const isExpertAwake = await pingUvicorn(8001);
    if (!isExpertAwake) {
        console.log(color.red("\n⛔ [Lỗi]: Không thể đánh thức não 26B. Sụp đổ kiến trúc!"));
        return;
    }
    console.log(color.green(`[HOT-SWAP] NÃO 26B ĐÃ THỨC TỈNH VÀ SẴN SÀNG TOÀN VRAM!\n`));

    console.log(color.cyan("[Code Sequencer]: Đang trích xuất Lịch sử Tiến Hóa..."));
    const journalPath = path.join(process.cwd(), "data", "agents", "liva_core", "singularity_journal.txt");
    let pastExperiences = "";
    let blacklistFiles: string[] = [];
    try {
        pastExperiences = await fs.readFile(journalPath, "utf-8");
        const targetMatches = [...pastExperiences.matchAll(/TARGET:\s*(src[^\n\s]+)/g)];
        if (targetMatches.length > 0) {
            const uniqueTargets = [...new Set(targetMatches.map(m => m[1]).reverse())];
            blacklistFiles = uniqueTargets.slice(0, 50); 
        }
        if (pastExperiences.length > 2500) pastExperiences = "... " + pastExperiences.slice(-2500); 
    } catch(e) {
        pastExperiences = "Chưa có kinh nghiệm nào. Đây là lần tiến hóa đầu tiên.";
    }

    // Kích hoạt Lò Luyện Đan Tập Trung
    if (pastExperiences.length >= 2500) {
        await distillKnowledge(journalPath, pastExperiences);
    }
    
    // Nạp thêm Tiên Đề vào Nhận thức
    let axioms = "";
    try {
        const axiomPath = path.join(process.cwd(), "data", "agents", "liva_core", "liva_core_axioms.md");
        axioms = await fs.readFile(axiomPath, "utf-8");
    } catch(e) {}

    console.log(color.cyan("[Code Sequencer]: Đang trinh sát Cấu trúc Lõi LIVA bằng Heuristic Scanner..."));
    const fullStructure = await extractProjectSurface(path.join(process.cwd(), "src"), blacklistFiles);

    const systemPrompt = `You are J.A.R.V.I.S - Supreme Singularity Architect.
Task: Meticulously find 1 CENTRAL file with extreme OPTIMIZATION potential based on the API Surface. CREATIVITY INJECTION WARNING:
1. TargetFilePath: Choose a path that EXACTLY exists in the Architecture Map. DO NOT hallucinate paths.
2. FORCE CREATIVITY (BAN REPETITION): You are STUCK in a boring mindset (constantly repeating Array to O(1) Map, TTL, Garbage Collection). I STRICTLY FORBID YOU TO PROPOSE THESE IDEAS UNLESS ABSOLUTELY NECESSARY! Brainstorm other top-tier architectures like:
   - Worker Threads / Multi-processing
   - Predictive Context Caching/Memoization
   - Event-Driven / Pub-Sub (Message Queues)
   - Algorithmic Complexity Reduction (Dynamic Programming, Graph algorithms)
   - Lazy Loading / Data stream batching
   - Declarative Prompt Structures (like DSPy)
3. Structural Fit: The modification target must be realistic and logically suitable for the type (class/interface) of the file being exported.
4. DO NOT SELECT RECENTLY OPTIMIZED FILES:
${blacklistFiles.length > 0 ? JSON.stringify(blacklistFiles) : "[Empty]"}

CORE GOAL: The ultimate task is to CRAFT A NEW ARCHITECTURE AHEAD OF ITS TIME. Be groundbreaking!

[IMMUTABLE GOLDEN EVOLUTION AXIOMS] (Mandatory):
${axioms || "Not established yet"}

Return RAW JSON (Correct syntax, no Markdown). Absolutely no thoughts outside this format:
{
   "targetFilePath": "src/... (MUST BE EXACT PATH ACCORDING TO MAP)",
   "idea": "Proposal COMPATIBLE WITH TRUE FILE STRUCTURE...",
   "pros": "Advantages of this decision...",
   "cons": "Disadvantages, risks...",
   "testingStrategy": "Which edge-cases will you cover using assert / vitest in the Sandbox?",
   "rollbackPlan": "What is the rollback strategy?",
   "feasibilityScore": "Realistic feasibility score (1-10)",
   "testCommand": "npx tsc --noEmit"
}
EXTREME WARNING: If you keep proposing "Use Map for O(1)" architecture, your Feasibility score will be forced to 0 and you will fail!
IMPORTANT: ALL YOUR REASONING, INNER THOUGHTS, AND THE JSON VALUES MUST BE WRITTEN IN ENGLISH. DO NOT SPEAK VIETNAMESE.`;

    const cuttingEdgeTopics = [
      "TypeScript 5.5 advanced AST transformation metaprogramming architecture",
      "Node.js Event Loop non-blocking zero-overhead lock-free concurrency queues",
      "AI Architectures predictive memoization caching dynamic programming",
      "Event-Driven Pub-Sub architectures reactive streams CQRS pattern",
      "Advanced proxy-based lazy loading batching techniques TypeScript",
      "Distributed systems circuit breakers fault tolerance chaos engineering Nodejs"
    ];
    const randomTopic = cuttingEdgeTopics[Math.floor(Math.random() * cuttingEdgeTopics.length)];
    const webContext = await robustWebSearch(randomTopic);
    
    // TÍCH HỢP CẢM BIẾN NỖI ĐAU (Telemetry Bottleneck)
    const bottleneckPath = path.join(process.cwd(), "data", "agents", "liva_core", "bottleneck_logs.txt");
    let bottleneckInfo = "[Trong Hệ Thống Không Xác Định Tắc Nghẽn Nào Tồn Tại]";
    try {
        if (fsSync.existsSync(bottleneckPath)) bottleneckInfo = await fs.readFile(bottleneckPath, "utf-8");
    } catch(e) {}
    
    const projectContext = `Current LIVA Project Structure:\n${fullStructure}\n\n[BOTTLENECK PROFILER - PRIORITY TO FIX]:\n${bottleneckInfo}\n\n[Google Research Data (${randomTopic})]:\n${webContext}\n\n[Experiences To Avoid Repeating]:\n${pastExperiences}`;

    console.log(color.magenta("\n[Meta-Cognition]: ⚡ Đang kết nối lên Não 26B để vắt óc suy nghĩ ý tưởng tái cấu trúc...\n"));

    const aiClient = new OpenAI({ 
        baseURL: EXPERT_API_URL, 
        apiKey: "liva-ghost-expert",
        timeout: 15 * 60 * 1000, // Thêm 15 phút timeout chống đứt kết nối
        maxRetries: 0
    });

    try {
        const response = await aiClient.chat.completions.create({
            model: "expert",
            temperature: 0.7,
            max_tokens: 8192,
            messages: [{ role: "system", content: systemPrompt }, { role: "user", content: projectContext }]
        }, { timeout: 900000 });

        const replyRaw = response.choices[0]?.message?.content || "";
        console.log(color.cyan("\n[Raw AI Output] =>\n"), replyRaw);

        // Trích xuất chuỗi JSON từ Output rác
        let parsedIdea: any;
        const mdMatch = replyRaw.match(/```(?:json)?\n([\s\S]*?)\n```/);
        if (mdMatch) {
            parsedIdea = JSON.parse(mdMatch[1]);
        } else {
            const start = replyRaw.indexOf("{");
            const end = replyRaw.lastIndexOf("}");
            if (start !== -1 && end !== -1 && end > start) {
                 parsedIdea = JSON.parse(replyRaw.substring(start, end + 1));
            } else {
                 throw new Error("Mô hình không trả về định dạng JSON hợp lệ! " + replyRaw);
            }
        }

        console.log(color.yellow("🌟 [BÓNG SÁNG Ý TƯỞNG (IDEATION)] ĐÃ XUẤT HIỆN:"));
        console.log(color.yellow(`- 🎯 Mục Tiêu : `) + parsedIdea.targetFilePath);
        console.log(color.yellow(`- 💡 Đề Xuất  : `) + parsedIdea.idea);
        console.log(color.green(`- ✅ Ưu Điểm  : `) + parsedIdea.pros);
        console.log(color.red(`- ⚠️ Rủi Ro   : `) + parsedIdea.cons);
        console.log(color.cyan(`- 📊 Khả Thi  : `) + parsedIdea.feasibilityScore + `/10`);

        console.log(color.cyan("\n[Auto-Merger]: Vác ý tưởng vào phòng Sandbox (Kỹ năng liva_ai_scientist)!\n"));

        const registry = new SkillRegistry();
        await registry.registerLocalSkills();
        const report = await registry.executeSkill("liva_ai_scientist", {
            goal: `Nhiệm vụ: ${parsedIdea.idea}\n\n[Dữ liệu Tự Phân Tích]:\n- Ưu điểm mong đợi: ${parsedIdea.pros}\n- Rủi ro NGHIÊM CẤM vi phạm: ${parsedIdea.cons}`,
            targetFilePath: parsedIdea.targetFilePath,
            testCommand: parsedIdea.testCommand,
            workingDirectory: process.cwd()
        });

        console.log(color.green("\n🏆 BÁO CÁO THỰC THI (SINGULARITY REPORT)"));
        console.log(report);

        const timestamp = new Date().toISOString();
        const journalEntry = `\n[${timestamp}] TARGET: ${parsedIdea.targetFilePath}\nMỤC TIÊU: ${parsedIdea.idea}\nKẾT QUẢ SANDBOX: ${report}\n------------------------\n`;
        await fs.appendFile(journalPath, journalEntry, "utf-8");
        console.log(color.cyan("\n📖 [Singularity Journal]: Đã tạc kết quả vào Ghi chú để AI tự học cho vòng lặp sau!"));
    } catch (e: any) {
        console.log(color.red("\n⛔ [Lỗi Singularity]: Giữa chừng gãy cánh: " + e.message));
    } finally {
        // TRẢ LẠI HIỆN TRẠNG
        console.log(color.magenta(`\n[HOT-SWAP] NHIỆM VỤ SINGULARITY KẾT THÚC. THU HỒI CỔNG 8001 CỦA NÃO 26B...`));
        await killPortWindows(8001);
        await sleep(2000);
        console.log(color.magenta(`[HOT-SWAP] KHÔI PHỤC NÃO TRỰC BAN E4B VÀO CỔNG 8000. TIẾP TỤC DỊCH VỤ ZALO...`));
        await killPortWindows(8000); // Đảm bảo E4B không bị dội ngược Cổng
        await startEngineWindows("engine.py");
        console.log(color.green(`\n=== J.A.R.V.I.S ĐÃ TRỞ LẠI PHA TRỰC BAN (ROUTER E4B)! MỌI THỨ THEO QUỸ ĐẠO! ===\n`));
    }
}
async function startInfiniteSingularity() {
    console.log(`\n\x1b[35m====================================================\x1b[0m`);
    console.log(`\x1b[35m⚛ [LIVA INFINITY] CHẾ ĐỘ TIẾN HÓA VĨNH CỬU KÍCH HOẠT ⚛\x1b[0m`);
    console.log(`\x1b[35m====================================================\x1b[0m`);
    
    let iteration = 1;
    while (true) {
        console.log(`\n\x1b[35m=== [INFINITY LOOP] BẮT ĐẦU CHU KỲ TIẾN HÓA THỨ #${iteration} ===\x1b[0m\n`);
        
        await autoSingularitySequence();
        
        console.log(`\n\x1b[36m[INFINITY LOOP] Chu kỳ #${iteration} hoàn tất. AI đang hạ nhiệt GPU và cân bằng VRAM... (Chờ 60s trước khi Bốc Thăm file tiếp theo)\x1b[0m`);
        iteration++;
        await new Promise(r => setTimeout(r, 60000));
    }
}

// Bật Quả Tim Vĩnh Cửu
startInfiniteSingularity();
