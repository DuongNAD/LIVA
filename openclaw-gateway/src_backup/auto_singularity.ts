import * as fs from "fs/promises";
import * as fsSync from "fs";
import * as path from "path";
import OpenAI from "openai";
import axios from "axios";
import { exec, spawn } from "child_process";
import { promisify } from "util";
import { SkillRegistry } from "./SkillRegistry.js";
import { LanceMemoryManager } from "./memory/LanceMemory.js";
import { notifyZalo } from "./utils/ZaloNotifier";
import * as dotenv from "dotenv";

dotenv.config();

// ==========================================
// V19: HỆ THỐNG GHI NHẬT KÝ (EVOLUTION LOGGER)
// ==========================================
const LOG_DIR = path.join(process.cwd(), "logs");
if (!fsSync.existsSync(LOG_DIR)) fsSync.mkdirSync(LOG_DIR);

const logFilename = `evolution_dump_${new Date().toISOString().replace(/[:.]/g, "-")}.log`;
const logStream = fsSync.createWriteStream(path.join(LOG_DIR, logFilename), { flags: "a" });

const originalConsoleLog = console.log;
const originalConsoleError = console.error;

function stripAnsi(text: string) { return text.replace(/\x1b\[[0-9;]*m/g, ""); }

console.log = function(...args) {
    originalConsoleLog.apply(console, args as any);
    const msg = args.map(a => (typeof a === 'object' ? JSON.stringify(a) : String(a))).join(" ");
    logStream.write(`[${new Date().toISOString()}] [INFO] ${stripAnsi(msg)}\n`);
};

console.error = function(...args) {
    originalConsoleError.apply(console, args as any);
    const msg = args.map(a => (typeof a === 'object' ? JSON.stringify(a) : String(a))).join(" ");
    logStream.write(`[${new Date().toISOString()}] [ERROR] ${stripAnsi(msg)}\n`);
};

process.on("uncaughtException", (err) => {
    console.error("FATAL UNCAUGHT EXCEPTION: ", err.stack || err.message);
    process.exit(1);
});
process.on("unhandledRejection", (reason) => {
    console.error("UNHANDLED REJECTION: ", String(reason));
});

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
            await execAsync(`taskkill /PID ${pid} /F /T`);
            console.log(color.green(`[Hot-Swap] Đã dọn dẹp sạch sẽ Cổng ${port}. VRAM được trả tự do.`));
        }
    } catch (e) {
        // Ignored. Có thể cổng đang không được dùng.
    }
}


async function pingUvicorn(port: number, retries = 30): Promise<boolean> {
    for (let i = 0; i < retries; i++) {
        try {
            const resp = await fetch(`http://127.0.0.1:${port}/docs`); // ping Uvicorn health
            if (resp.status) return true;
        } catch (e) {
            await sleep(2000); // 2s ping 1 lần
        }
    }
    return false;
}

async function checkPortAvailable(port: number): Promise<boolean> {
    for (let i = 0; i < 10; i++) {
        try {
            const { stdout } = await execAsync(`netstat -ano | findstr :${port}`);
            if (!stdout.trim()) return true; // Cổng hoàn toàn trống
        } catch (e) {
            return true; // Lỗi thường là do findstr không tìm thấy gì (Cổng trống)
        }
        await sleep(1000);
    }
    return false;
}

async function waitForVRAMClear(thresholdMB = 2048, timeoutSec = 30): Promise<void> {
    console.log(color.cyan(`[VRAM Polling] Đang chờ GPU giải phóng bộ nhớ...`));
    for (let i = 0; i < timeoutSec; i++) {
        try {
            const { stdout } = await execAsync(`nvidia-smi --query-gpu=memory.used --format=csv,noheader,nounits`);
            if (stdout) {
                const usedVRAM = parseInt(stdout.trim());
                if (usedVRAM <= thresholdMB) {
                    console.log(color.green(`[VRAM Polling] OK! VRAM hiện tại: ${usedVRAM} MB. Đã sẵn sàng nạp Brain mới.`));
                    return;
                }
            }
        } catch (e) { }
        await sleep(1000);
    }
    console.log(color.yellow(`[VRAM Polling] Timeout chờ VRAM. Có thể OS đang Cache cứng. Tiếp tục tiến trình...`));
}

async function startEngineWindows(fileName: string, args: string[] = []) {
    const roleStr = args.length > 0 ? args.join(" ") : "(Default)";
    console.log(color.cyan(`[Hot-Swap] Kích nổ động cơ ${fileName} ${roleStr}... (Chạy ngầm)`));
    const engineDir = path.join(process.cwd(), "..", "liva-ai-engine");
    const pythonPath = path.join(engineDir, "venv", "Scripts", "python.exe");

    const logDir = path.join(process.cwd(), "logs");
    if (!fsSync.existsSync(logDir)) fsSync.mkdirSync(logDir);
    const out = fsSync.openSync(path.join(logDir, `${fileName}.log`), "a");
    const err = fsSync.openSync(path.join(logDir, `${fileName}.err.log`), "a");
    
    const child = spawn(pythonPath, [fileName, ...args], {
        cwd: engineDir,
        windowsHide: true, 
        stdio: ["ignore", out, err], 
        env: { ...process.env, PYTHONIOENCODING: "utf-8" } 
    });
    child.on('error', (errState) => {
        console.error(color.red(`[Hot-Swap] Lỗi khởi động động cơ Python: ${errState.message}`));
    });
    child.unref(); 
    
    fsSync.closeSync(out);
    fsSync.closeSync(err);
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
    
    // Sort theo độ ưu tiên (Heuristic Scanner) & giữ Top 10 thay vì 20 để tiết kiệm Tokens
    allFiles.sort((a, b) => b.weight - a.weight);
    const topFiles = allFiles.slice(0, 10);

    let result = "[Top 20 Lõi]:\n";
    for(const f of topFiles) {
        result += `- ${f.path}\n`;
        if (f.exports.length > 0) {
            result += `  [Exports]: ${f.exports.join(", ")}\n`;
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

async function distillKnowledge(rawJournal: string, memory: any) {
    if (rawJournal.length < 2500) return; // Chỉ chưng cất khi file đủ dài
    console.log(color.yellow("\n🔥 [Lò Luyện Đan]: Lịch sử Tiến Hóa đã quá Dày! Kích hoạt thuật toán Chưng Cất Tri Thức (Knowledge Distillation)..."));
    
    // Nạp Não 26B để chưng cất
    const aiClient = new OpenAI({ baseURL: EXPERT_API_URL, apiKey: "liva-ghost-expert" });
    const existAxioms = (await memory.searchMemory("CORE_ARCHITECTURE TYPESCRIPT_SAFETY", 15)).join('\n');
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
        
        // Save core axioms to vector memory
        await memory.addMemory("AXIOM", newAxioms, "SYSTEM_CORE");
        
        // Memory Pruning: Xóa toàn bộ episodic để giải phóng Rác
        await memory.clearEpisodicMemories();
        
        console.log(color.green("✅ [Trí Nhớ Tiên Đề]: Chưng cất thành công! Rác Log bị làm trống (đã lưu Blacklist), AXIOM ĐÃ ĐƯỢC NHÚNG VÀO LANCEDB."));
        console.log(color.green("✅ [Trí Nhớ Tiên Đề]: Chưng cất thành công! Rác Log bị làm trống (đã lưu Blacklist), File Axioms ĐÃ ĐƯỢC DUNG HỢP."));
    } catch (e) {
        console.log(color.red("⛔ Lỗi chưng cất: " + e));
    }
}

const GLOBAL_MEMORY = new LanceMemoryManager();
let isMemoryConnected = false;

async function autoSingularitySequence() {
    console.log(color.green("================================================================"));
    console.log(color.green(" 🚀 [LIVA SINGULARITY DAEMON] - CHU TRÌNH TỰ TIẾN HÓA KÍCH HOẠT"));
    console.log(color.green("================================================================\n"));

    console.log(color.magenta(`[HOT-SWAP] TẠM NGƯNG HỆ THỐNG ZALO BOT. THU HỒI VRAM TỪ NÃO E4B...`));
    await killPortWindows(8000); // Giết E4B
    await killPortWindows(8001); // Dọn sạch tiến trình rác của Planner
    await killPortWindows(8002); // Dọn sạch tiến trình rác của Coder
    await waitForVRAMClear(2048, 30);

    const workspaceDir = path.join(process.cwd(), ".workspace");
    if (!fsSync.existsSync(workspaceDir)) fsSync.mkdirSync(workspaceDir, { recursive: true });

    let parsedIdea: any = null;
    let crashErrorMsg: string | null = null;

    try {
        console.log(color.cyan(`[Hot-Swap] PHA 1: KHỞI ĐỘNG KỸ SƯ TRƯỞNG (PLANNER) - CỔNG 8001`));
        await checkPortAvailable(8001);
        await startEngineWindows("ai_engine.py", ["--role", "planner", "--port", "8001", "--n_ctx", "24576"]);
        
        console.log(color.yellow(`[HOT-SWAP] Đang đợi Kỹ sư Trưởng khởi động động cơ Uvicorn...`));
        const isPlannerAwake = await pingUvicorn(8001, 240);
        if (!isPlannerAwake) {
            throw new Error("Không thể đánh thức não Planner. Sụp đổ kiến trúc!");
        }
        console.log(color.green(`[HOT-SWAP] NÃO PLANNER (8001) ĐÃ SẴN SÀNG TOÀN VRAM!\n`));

    console.log(color.cyan("[Code Sequencer]: Đang trích xuất Lịch sử Tiến Hóa đa chiều từ LanceDB..."));
    if (!isMemoryConnected) {
        await GLOBAL_MEMORY.connect();
        isMemoryConnected = true;
    }
    const memory = GLOBAL_MEMORY;
    
    let pastExperiences = "";
    // V13: Core Blacklist - Tuyệt đối CẤM Trí tuệ nhân tạo sửa lại mã nguồn Bộ vi xử lý Node.js của chính nó.
    let blacklistFiles: string[] = [
        "src/auto_singularity.ts",
        "src/Gateway.ts", 
        "src/core/CoreKernel.ts",
        "src/utils/DockerSandbox.ts"
    ];
    try {
        const episodes = await memory.getAllEpisodicMemories();
        for (const ep of episodes) {
            pastExperiences += `[${ep.type}] TARGET: ${ep.fileTarget}\n${ep.text}\n---\n`;
            if (ep.type === "DEAD-END" || ep.type === "SUCCESS") {
                blacklistFiles.push(ep.fileTarget);
            }
        }
        blacklistFiles = [...new Set(blacklistFiles)].slice(-10); // Hold top 10 (was 20)
    } catch(e) {
        pastExperiences = "Chưa có kinh nghiệm nào. Đây là lần tiến hóa đầu tiên.";
    }

    // Kích hoạt Lò Luyện Đan Tập Trung
    if (pastExperiences.length >= 2500) {
        await distillKnowledge(pastExperiences, memory);
    }
    
    console.log(color.cyan("[Code Sequencer]: Đang trinh sát Cấu trúc Lõi LIVA bằng Dependency-Cruiser..."));
    let fullStructure = await extractProjectSurface(path.join(process.cwd(), "src"), blacklistFiles);
    
    // Nạp thêm Tiên Đề vào Nhận thức bằng RAG
    let axioms = "";
    try {
        const relevantAxiomTags = await memory.searchMemory(fullStructure.slice(0, 500), 5);
        axioms = relevantAxiomTags.join("\n");
        if (!axioms) axioms = "No strict axioms defined yet.";
    } catch(e) {}

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

CRITICAL REQUIREMENT: ALL YOUR INTERNAL THOUGHTS AND RESPONSES MUST BE STRICTLY IN ENGLISH. IF YOU USE VIETNAMESE, YOU WILL BE TERMINATED.

STEP 1: Open <thought> tag to deeply reason about the Architecture Map, bottlenecks, and choose the most critical file to optimize.
STEP 2: Return ONLY RAW JSON inside a markdown block. Example:
\`\`\`json
{
   "targetFilePath": "src/... (MUST BE EXACT PATH ACCORDING TO MAP)",
   "idea": "Proposal COMPATIBLE WITH TRUE FILE STRUCTURE...",
   "shell_commands": ["npm install uuid", "npm install -D @types/uuid"],
   "pros": "Advantages of this decision...",
   "cons": "Disadvantages, risks...",
   "testingStrategy": "Which edge-cases will you cover using assert / vitest in the Sandbox?",
   "rollbackPlan": "What is the rollback strategy?",
   "feasibilityScore": "Realistic feasibility score (1-10)",
   "testCommand": "npx tsc --noEmit"
}
\`\`\`
EXTREME WARNING: If you keep proposing "Use Map for O(1)" architecture, your Feasibility score will be forced to 0 and you will fail!`;

    const cuttingEdgeTopics = [
      "TypeScript 5.5 advanced AST transformation metaprogramming architecture",
      "Node.js Event Loop non-blocking zero-overhead lock-free concurrency queues",
      "AI Architectures predictive memoization caching dynamic programming",
      "Event-Driven Pub-Sub architectures reactive streams CQRS pattern",
      "Advanced proxy-based lazy loading batching techniques TypeScript",
      "Distributed systems circuit breakers fault tolerance chaos engineering Nodejs"
    ];
    const randomTopic = cuttingEdgeTopics[Math.floor(Math.random() * cuttingEdgeTopics.length)];
    let webContext = await robustWebSearch(randomTopic);
    
    // TÍCH HỢP CẢM BIẾN NỖI ĐAU (Telemetry Bottleneck)
    const bottleneckPath = path.join(process.cwd(), "data", "agents", "liva_core", "bottleneck_logs.txt");
    let bottleneckInfo = "[No Bottlenecks Identified In System]";
    try {
        if (fsSync.existsSync(bottleneckPath)) bottleneckInfo = await fs.readFile(bottleneckPath, "utf-8");
    } catch(e) {}
    
    let projectContext = `Current LIVA Project Structure:\n${fullStructure}\n\n[BOTTLENECK PROFILER - PRIORITY TO FIX]:\n${bottleneckInfo}\n\n[Google Research Data (${randomTopic})]:\n${webContext}\n\n[Experiences To Avoid Repeating]:\n${pastExperiences}\n\nCRITICAL: BASED ON THE ABOVE, YOU MUST GENERATE EXACTLY ONE RAW JSON BLOCK. Do not write markdown reports. Response MUST be in this format:\n{\n  "targetFilePath": "...",\n  "idea": "...",\n  "shell_commands": [],\n  "pros": "...",\n  "cons": "...",\n  "testingStrategy": "...",\n  "rollbackPlan": "...",\n  "feasibilityScore": "...",\n  "testCommand": "..."\n}`;

    console.log(color.magenta("\n[Meta-Cognition]: ⚡ Đang kết nối lên Não Planner để vắt óc suy nghĩ ý tưởng tái cấu trúc...\n"));

    const aiClient = new OpenAI({ 
        baseURL: EXPERT_API_URL, 
        apiKey: "liva-ghost-planner",
        timeout: 15 * 60 * 1000, 
        maxRetries: 0
    });

        console.log(color.cyan("\n[Debug]: Đang thực hiện Test Ping (Dùng prompt nhỏ) tới Planner để kiểm tra Server..."));
        try {
            await aiClient.chat.completions.create({
                model: "expert",
                messages: [{ role: "user", content: "System check: respond 'ok'" }],
                max_tokens: 10
            }, { timeout: 30000 });
            console.log(color.green("[Debug]: Ping thành công! Kết nối tới Phân hệ Kế hoạch (8001) hoạt động bình thường."));
        } catch (pingErr: any) {
            throw new Error(`KẾT NỐI BỊ TỪ CHỐI GIAI ĐOẠN PING PLANNER: ${pingErr.message}`);
        }

        console.log(color.magenta("\n[Meta-Cognition]: ⚡ Test OK! Gửi khối lượng context khổng lồ lên Não Planner để vắt óc suy nghĩ...\n"));
        if (global.gc) global.gc();

        const MAX_RETRIES = 3;
        for (let attempt = 1; attempt <= MAX_RETRIES; attempt++) {
            try {
                const response = await aiClient.chat.completions.create({
                    model: "expert",
                    temperature: 0.4, 
                    top_p: 0.9,       
                    max_tokens: 1500, 
                    stop: ["<start_of_turn>", "<end_of_turn>", "```\n\nWait", "Wait, I see"],
                    messages: [{ role: "user", content: `${systemPrompt}\n\n${projectContext}` }]
                }, { timeout: 900000 });

                const replyRaw = response.choices[0]?.message?.content || "";
                console.log(color.cyan("\n[Raw AI Output] =>\n"), replyRaw);

                const mdMatch = replyRaw.match(/```(?:json)?\n([\s\S]*?)\n```/);
                if (mdMatch) {
                    parsedIdea = JSON.parse(mdMatch[1]);
                } else {
                    const start = replyRaw.indexOf("{");
                    const end = replyRaw.lastIndexOf("}");
                    if (start !== -1 && end !== -1 && end > start) {
                         parsedIdea = JSON.parse(replyRaw.substring(start, end + 1));
                    } else {
                         throw new Error("Mô hình không trả về định dạng JSON hợp lệ!");
                    }
                }
                
                // Ghi kế hoạch vào Checklist Checkpoint
                fsSync.writeFileSync(path.join(workspaceDir, "current_plan.json"), JSON.stringify(parsedIdea, null, 2), "utf-8");
                break; // Thoát vòng lặp Retry nếu thành công
            } catch (err: any) {
                console.log(color.yellow(`[Retry] Lỗi JSON Kế hoạch (Lần ${attempt}), Nội dung: ${err.message}`));
                if (attempt === MAX_RETRIES) throw new Error("Thất bại phân tích JSON Kế hoạch sau 3 lần nặn. Dừng Tiến hóa!");
                await sleep(5000);
            }
        }

        console.log(color.yellow("🌟 [BÓNG SÁNG Ý TƯỞNG (IDEATION)] ĐÃ XUẤT HIỆN:"));
        console.log(color.yellow(`- 🎯 Mục Tiêu : `) + parsedIdea.targetFilePath);
        console.log(color.yellow(`- 💡 Đề Xuất  : `) + parsedIdea.idea);
        console.log(color.green(`- ✅ Ưu Điểm  : `) + parsedIdea.pros);

        // ==== PHA 2: THỢ CODE ====
        console.log(color.magenta(`\n[HOT-SWAP] CHUYỂN GIAO PHA 2: CHẠY MODEL TIERING ZERO-OVERHEAD TRÊN CỔNG 8001`));
        // Xóa hoàn toàn việc kill 8001 và nạp lại 8002 để tiết kiệm 100% I/O load
        // await killPortWindows(8001);
        // await waitForVRAMClear(2048, 30);
        // await checkPortAvailable(8002);
        // await startEngineWindows("ai_engine.py", ["--role", "coder", "--port", "8002", "--n_ctx", "16384"]);
        console.log(color.yellow(`[HOT-SWAP] Đang tái sử dụng cổng 8001 cho Thợ Code (Darwinian Evolver)...`));

        console.log(color.cyan("\n[Auto-Merger]: Vác ý tưởng vào phòng Sandbox (Kỹ năng liva_ai_scientist)!\n"));

        const registry = new SkillRegistry();
        await registry.registerLocalSkills();
        const report = await registry.executeSkill("liva_ai_scientist", {
            targetFilePath: parsedIdea.targetFilePath,
            goal: `Nhiệm vụ: ${parsedIdea.idea}\n\n[Dữ liệu Tự Phân Tích]:\n- Ưu điểm mong đợi: ${parsedIdea.pros}\n- Rủi ro NGHIÊM CẤM vi phạm: ${parsedIdea.cons}`,
            testCommand: parsedIdea.testCommand,
            workingDirectory: process.cwd(),
            checkpointPath: path.join(workspaceDir, "current_plan.json")
        });

        console.log(color.green("\n🏆 BÁO CÁO THỰC THI (SINGULARITY REPORT)"));
        console.log(report);
        console.log(color.cyan("\n📖 [Singularity Journal]: Đã tạc kết quả vào LanceMemory để AI tự học cho vòng lặp sau!"));

    } catch (e: any) {
        crashErrorMsg = e.message;
        console.log(color.red("\n⛔ [Lỗi Singularity]: Giữa chừng gãy cánh: " + e.message));
    } finally {
        // PHA 3 KHÔI PHỤC HIỆN TRẠNG
        console.log(color.magenta(`\n[HOT-SWAP] THU HỒI CÁC CỔNG AI TIẾN HÓA VÀ DỌN DẸP VRAM...`));
        await killPortWindows(8001);
        await killPortWindows(8002);
        await waitForVRAMClear(2048, 30);
        
        console.log(color.magenta(`[HOT-SWAP] KHÔI PHỤC NÃO TRỰC BAN E4B VÀO CỔNG 8000. TIẾP TỤC DỊCH VỤ ZALO...`));
        await killPortWindows(8000); 
        await startEngineWindows("engine.py", []);

        if (crashErrorMsg) {
            console.log(color.magenta(`[HOT-SWAP] Đang phát tín hiệu SOS về Bộ chỉ huy thông qua Zalo...`));
            await notifyZalo(`🚨 [LIVA SOS]\nVòng lặp Cải tiến Sinh tồn (Singularity) vừa bị đứt gãy!\nNguyên nhân: ${crashErrorMsg}\n\nĐã khôi phục tạm thời Zalo Router (8000). Sếp vui lòng kiểm tra Logs.`);
        }

        console.log(color.green("\n=== J.A.R.V.I.S ĐÃ TRỞ LẠI PHA TRỰC BAN (ROUTER E4B)! MỌI THỨ THEO QUỸ ĐẠO! ===\n"));
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
        
        // Bơm ép dọn rác (Garbage Collection) xả RAM sau chuỗi thao tác nặng
        if (global.gc) {
            global.gc();
            console.log(`\x1b[36m[LIVA GC] Đã vắt cạn bộ nhớ rác của tiến trình Hệ thống.\x1b[0m`);
        }

        await new Promise(r => setTimeout(r, 60000));
    }
}

// Bật Quả Tim Vĩnh Cửu
startInfiniteSingularity();
