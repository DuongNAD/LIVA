import * as fs from "fs/promises";
import * as path from "path";
import OpenAI from "openai";
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


async function extractProjectSurface(dirPath: string, prefix = "", blacklist: string[] = []): Promise<string> {
    let result = "";
    try {
        const files = await fs.readdir(dirPath, { withFileTypes: true });
        for (const file of files) {
            if (file.name.startsWith("node_modules") || file.name.startsWith("dist") || file.name.startsWith(".git") || file.name.endsWith(".sandbox.ts") || file.name.endsWith(".bak")) {
                continue;
            }
            
            const relPath = path.relative(process.cwd(), path.join(dirPath, file.name)).replace(/\\/g, '/');
            if (blacklist.includes(relPath)) continue;

            if (file.isDirectory()) {
                const subContent = await extractProjectSurface(path.join(dirPath, file.name), prefix + "  ", blacklist);
                if (subContent.trim() !== "") {
                    result += `${prefix}📁 ${file.name}/\n` + subContent;
                }
            } else if (file.name.endsWith(".ts")) {
                result += `${prefix}📄 ${file.name}\n`;
                try {
                    const content = await fs.readFile(path.join(dirPath, file.name), "utf-8");
                    const matches = [...content.matchAll(/export\s+(class|interface|type|enum|function)\s+([A-Za-z0-9_]+)/g)];
                    if (matches.length > 0) {
                        result += `${prefix}   └─ API Surface: ${matches.map(m => m[2]).join(", ")}\n`;
                    }
                } catch(e) {}
            }
        }
    } catch (error) { }
    return result;
}

async function performWebResearch(topic: string): Promise<string> {
    try {
        console.log(color.cyan(`\n[Web Intelligence]: Đang sục sạo Google/DuckDuckGo tìm kiếm: "${topic}"...`));
        const res = await fetch("https://html.duckduckgo.com/html/", {
            method: "POST",
            headers: { "Content-Type": "application/x-www-form-urlencoded" },
            body: `q=${encodeURIComponent(topic)}`
        });
        const html = await res.text();
        const snippetMatches = [...html.matchAll(/<a class="result__snippet[^>]*>(.*?)<\/a>/gi)];
        let insights = "";
        snippetMatches.slice(0, 3).forEach((match, idx) => {
            insights += `- Ý Tưởng Web ${idx+1}: ${match[1].replace(/<\/?[^>]+(>|$)/g, "")}\n`;
        });
        return insights || "Không có dữ liệu Web khả dụng.";
    } catch (e) {
        return "Lỗi mạng khi lấy dữ liệu Web.";
    }
    }
}

async function distillKnowledge(journalPath: string, rawJournal: string) {
    if (rawJournal.length < 2500) return; // Chỉ chưng cất khi file đủ dài
    console.log(color.yellow("\n🔥 [Lò Luyện Đan]: Lịch sử Tiến Hóa đã quá Dày! Kích hoạt thuật toán Chưng Cất Tri Thức (Knowledge Distillation)..."));
    
    const axiomPath = path.join(process.cwd(), "data", "agents", "liva_core", "liva_core_axioms.md");
    
    // Nạp Não 26B để chưng cất
    const aiClient = new OpenAI({ baseURL: EXPERT_API_URL, apiKey: "liva-ghost-expert" });
    const prompt = `Từ kinh nghiệm lập trình sâu sắc sau đây mà hệ thống vừa học được, hãy chắt lọc ra ĐÚNG 3 Lệnh Thuật Toán Tối Ưu (Heuristic Rules) cốt lõi nhất. Trình bày dạng gạch đầu dòng ngắn gọn súc tích cực hạn (Mỗi luật 1 dòng). Không giải thích dài dòng.\n\nLịch sử:\n${rawJournal.slice(-4000)}`;
    
    try {
        const response = await aiClient.chat.completions.create({
            model: "expert",
            temperature: 0.1,
            max_tokens: 500,
            messages: [{ role: "system", content: "Bạn là AI Siêu Nén (Axiomatic Compressor) - Trí nhớ Tiên Đề." }, { role: "user", content: prompt }]
        });
        
        let newAxioms = response.choices[0]?.message?.content || "";
        
        const exist = await fs.readFile(axiomPath, "utf-8").catch(() => null);
        if (exist) {
            await fs.appendFile(axiomPath, "\n\n" + newAxioms, "utf-8");
        } else {
            await fs.writeFile(axiomPath, "# 🧬 LIVA CORE AXIOMS (Luật Vàng Tiến Hóa Bất Biến)\n\n" + newAxioms, "utf-8");
        }
        
        // ĐẬP NÁT LOG CŨ (Để lại Header) ĐỂ NGĂN DOOM LOOP LẶP LẠI
        await fs.writeFile(journalPath, "", "utf-8");
        console.log(color.green("✅ [Trí Nhớ Tiên Đề]: Chưng cất thành công! Rác Log đã bị tiêu hủy, File Axioms đã được bồi đắp."));
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

    console.log(color.cyan("[Code Sequencer]: Đang trinh sát Cấu trúc Lõi LIVA (Lọc Mù Blacklist)..."));
    const fullStructure = await extractProjectSurface(path.join(process.cwd(), "src"), "", blacklistFiles);

    const systemPrompt = `Bạn là J.A.R.V.I.S - Giám Đốc Kỹ Thuật Tối Cao.
Nhiệm vụ: Tìm tỉ mỉ 1 file TRUNG TÂM có tiềm năng TỐI ƯU HÓA cực cao dựa vào Bản đồ Kiến trúc (API Surface) được cung cấp. Phân tích các hàm, kiểu dữ liệu mà nó export để định hướng thay đổi.
MỤC TIÊU CỐT LÕI (CORE OBJECTIVE): Sếp Dương yêu cầu tập trung tuyệt đối vào: HIỆU NĂNG (Performance), TỐI ƯU HÓA BỘ NHỚ (O(1) Data Structures, Memory Leak Prevention), và ĐỘ CỨNG CÁP QUẢN LÝ LỖI (Stability & Robust Error Handling). Tạm dừng các nâng cấp bảo mật trừ khi nó giúp mã chạy mượt hơn.
LƯU Ý TỐI QUAN TRỌNG: BẮT BUỘC CHỈ XUẤT RA RAW JSON. Tuyệt đối không chèn thẻ <|channel>| hay suy nghĩ ngoài luồng.
LỆNH CẤM VƯỢT QUYỀN (ZERO-TOLERANCE BLACKLIST): CẤM TUYỆT ĐỐI không chọn lại các tập tin ĐÃ TỐI ƯU GẦN ĐÂY sau đây:
${blacklistFiles.length > 0 ? JSON.stringify(blacklistFiles) : "[Trống]"}
Hãy khám phá các tập tin MỚI CHƯA TỪNG chạm tới để nâng cấp đều đặn toàn diện hệ thống!

[NHỮNG LUẬT VÀNG TIẾN HÓA BẤT BIẾN] (Bắt buộc tuân thủ):
${axioms || "Chưa thiết lập"}

Trả về RAW JSON (Đúng syntax, không Markdown):
{
   "targetFilePath": "src/core/CoreKernel.ts hoặc src/memory/...",
   "idea": "Hướng nâng cấp hiệu năng bằng Garbage Collection, O(1) Map, hoặc Caching...",
   "pros": "Ưu điểm của quyết định này (Tốc độ xử lý tăng, rò rỉ RAM giảm)...",
   "cons": "Nhược điểm, rủi ro có thể gây hỏng hóc...",
   "feasibilityScore": "Độ khả thi thực tế (1-10)",
   "testCommand": "npx tsc --noEmit"
}`;

    const webContext = await performWebResearch("typescript enterprise advanced performance optimization robust error handling high load memory management 2026");
    const projectContext = `Cấu trúc Project LIVA Hiện Tại:\n${fullStructure}\n\n[Dữ Liệu Thu Thập Từ Google (Xu Hướng Hiện Tại)]:\n${webContext}\n\n[Kinh Nghiệm Tự Tối Ưu Lần Trước (TRÁNH LẶP LẠI)]:\n${pastExperiences}`;

    console.log(color.magenta("\n[Meta-Cognition]: ⚡ Đang kết nối lên Não 26B để vắt óc suy nghĩ ý tưởng tái cấu trúc...\n"));

    const aiClient = new OpenAI({ baseURL: EXPERT_API_URL, apiKey: "liva-ghost-expert" });

    try {
        const response = await aiClient.chat.completions.create({
            model: "expert",
            temperature: 0.7,
            max_tokens: 8192,
            messages: [{ role: "system", content: systemPrompt }, { role: "user", content: projectContext }]
        });

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
