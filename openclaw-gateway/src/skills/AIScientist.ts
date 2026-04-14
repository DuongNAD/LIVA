import OpenAI from "openai";
import * as fs from "fs";
import * as path from "path";
import { exec } from "child_process";
import { promisify } from "util";
import axios from "axios";

const execAsync = promisify(exec);

async function notifyZalo(message: string) {
    try {
        await axios.post('http://127.0.0.1:8000/api/zalo/send', {
            phone: "0343388056",
            message: message
        });
    } catch(e) { }
}

async function buildProjectSurface(dirPath: string): Promise<string> {
    // Fake for now
    return "Project Surface";
}

async function robustWebSearch(query: string): Promise<string> {
    return "Web Hints";
}

function extractSlidingWindowWithLines(searchChunk: string, fileContent: string): string {
    if (!searchChunk) return "";
    const lines = fileContent.split('\n');
    let bestMatchIdx = -1;
    
    const firstSearchLine = searchChunk.trim().split('\n')[0]?.trim();
    if (firstSearchLine) {
        for (let i = 0; i < lines.length; i++) {
            if (lines[i].includes(firstSearchLine)) {
                bestMatchIdx = i;
                break;
            }
        }
    }
    
    if (bestMatchIdx === -1) bestMatchIdx = 20; 
    
    const start = Math.max(0, bestMatchIdx - 20);
    const end = Math.min(lines.length - 1, bestMatchIdx + 20);
    
    let result = "";
    for (let i = start; i <= end; i++) {
        result += `${i + 1}: ${lines[i]}\n`;
    }
    return result;
}

export const execute = async (args: any): Promise<string> => {
  const workspace = process.cwd();
  let targetPath = args.targetFilePath;
  if (!path.isAbsolute(targetPath)) {
    targetPath = path.join(workspace, targetPath);
  }

  const dirName = path.dirname(targetPath);
  const extName = path.extname(targetPath);
  const baseName = path.basename(targetPath, extName);

  let originalCode = "";
  if (fs.existsSync(targetPath)) {
    originalCode = fs.readFileSync(targetPath, "utf8");
  }

  const aiClient = new OpenAI({
    baseURL: "http://127.0.0.1:8001/v1",
    apiKey: "liva-ghost-expert",
    timeout: 10 * 60 * 1000,
    maxRetries: 0
  });

  const projectSurface = await buildProjectSurface(path.join(workspace, "src"));
  const initialWebHints = await robustWebSearch(`${args.goal.slice(0, 100)} typescript best practices tutorial`);

  const systemPrompt = `Bạn là J.A.R.V.I.S (Siêu Kỹ Sư Công Nghệ LIVA - Agentic AI 3.0). 
Mục tiêu Tự Tiến Hóa lần này: ${args.goal}

[BẢN ĐỒ KIẾN TRÚC HIỆN TẠI]: Bạn BẮT BUỘC dùng đúng tên File/Export khi Import để tránh Ảo Giác:
${projectSurface.slice(0, 8000)}

[NGHIÊN CỨU INTERNET (XU HƯỚNG MỚI)]:
${initialWebHints}

[QUY TRÌNH BẮT BUỘC - GIT MULTI-FILE SANDBOX]:
Để bẻ khóa lồng giam tệp đơn, bạn được cấp quyền thao tác trên một Nhánh Git song song. Bạn có thể sửa nhiều file cùng lúc để tạo ra các Kiến trúc lớn.

BƯỚC 1: Bắt buộc mở thẻ <thought> để phân tích hệ quả. Trả lời đúng 3 câu:
- Dependencies: Code mới này có cần import thêm thư viện nào không?
- Cascade Impact: Sửa logic ở đây có làm vỡ Interface của file khác gọi tới nó không?
- Test Strategy: Unit Test sẽ phủ (cover) các trường hợp Edge-case nào?

BƯỚC 2: Cấp MỘT HOẶC NHIỀU thẻ <file_action path="..."> để chỉnh sửa mã đa tệp.
- Bắt buộc khai báo đường dẫn tương đối (Ví dụ: path="src/core/main.ts")
- Bạn dùng cơ chế TÌM-VÀ-THAY-THẾ (Search/Replace). Bắt buộc copy DƯ SẴN 3-4 dòng nguyên bản tĩnh làm Mỏ Neo.
Ví dụ:
<file_action path="src/math.ts">
  <search>
  function oldMethod() {
      doSomething();
  }
  </search>
  <replace>
  export function newMethod() {
      doSomethingFaster();
  }
  </replace>
</file_action>

BƯỚC 3: Bắt buộc viết một đoạn Unit Test kiểm định độc lập đưa vào thẻ <test_case>. Trong test BẮT BUỘC chèn Broken Tool Simulation (Giả lập rớt mạng, nullish).
TRONG KHỐI <test_case>, BẠN BẮT BUỘC PHẢI CHỨNG MINH ĐỘT PHÁ CỦA BẠN LÀ CÓ THẬT. Hãy dùng performance.now() để chạy Hàm Gốc 1000 lần và Hàm Mới 1000 lần. Dùng assert để đánh rớt bài Test nếu mã mới của bạn chậm hơn! BẠN PHẢI SINH TỒN BẰNG TỐC ĐỘ!`;

  let conversation: any[] = [
    { role: "system", content: systemPrompt }
  ];

  if (originalCode) {
    conversation.push({
      role: "user",
      content: `Đây là mã nguồn HIỆN TẠI của ${targetPath}:\n\n\`\`\`typescript\n${originalCode}\n\`\`\`\n\nHãy tiến hóa nó theo đúng 3 Bước (Thought -> file_action -> test_case).`
    });
  }

  const testPath = path.join(workspace, "liva_evolution_sandbox.test.ts");

  let report = `# TIẾN TRÌNH J.A.R.V.I.S: KHỞI ĐỘNG MACRO-EVOLUTION V5\n`;
  report += `[__start__] Bật Môi Trường Đa Tệp Song Song (Git Branch)\n`;
  console.log(`[J.A.R.V.I.S] Bật cơ chế Tiến Hóa Vĩ Mô qua Git Checkout!`);

  const MAX_CYCLES = 3;
  let currentCycle = 1;
  let mergedSuccess = false;
  let errorFingerprints: string[] = []; 

  const branchName = `liva_evo_${Date.now()}`;
  let isGitSwapped = false;
  try {
      await execAsync(`git stash && git checkout -b ${branchName}`, { cwd: workspace });
      isGitSwapped = true;
  } catch(e) { }

  while (currentCycle <= MAX_CYCLES) {
    report += `\n>> [Nodes: generate] Vòng lặp thứ (#${currentCycle})...\n`;
    let execErr: any = null;

    try {
      await notifyZalo(`🧠 [Singularity V5]: Không Gian Tác Giả (Git-Branch Sandbox). Vòng lặp (#${currentCycle}/${MAX_CYCLES})...`);
      console.log(`\x1b[36m\n>> [Nodes: generate] Đang tương tác Nhánh Vô Hình: ${branchName} (Vòng ${currentCycle})...\x1b[0m`);

      const streamRes = await aiClient.chat.completions.create({
        model: "expert",
        messages: conversation,
        temperature: 0.2, 
        frequency_penalty: 0.3,
        presence_penalty: 0.2,
        max_tokens: 16384,
        stream: true
      }, { timeout: 600000 });

      let replyContent = "";
      process.stdout.write("\x1b[90m"); 
      for await (const chunk of streamRes) {
         const token = chunk.choices[0]?.delta?.content || "";
         replyContent += token;
         process.stdout.write(token); 
      }
      process.stdout.write("\x1b[0m\n"); 

      conversation.push({ role: "assistant", content: replyContent });

      if (replyContent.includes("[ABORT_AND_ROLLBACK]")) {
          console.log(`\x1b[31m[Sandbox Rollback]: Nhận lệnh ABORT_AND_ROLLBACK từ 26B. Đang dọn dẹp file...\x1b[0m`);
          await execAsync(`git reset --hard HEAD`, { cwd: workspace });
          conversation.push({ role: "user", content: `Đã Reset File gốc thành công. Hãy bắt đầu lại Tư duy từ con số 0.` });
          currentCycle++;
          continue;
      }

      if (!/<\|?thought>/.test(replyContent)) {
         throw new Error("LỖI: 26B đã bỏ quên Thinking Protocol! Cấm code bừa bãi!");
      }

      const editRegex = /<file_action(?:\s+path="([^"]+)")?>\s*<search>([\s\S]*?)<\/search>\s*<replace>([\s\S]*?)<\/replace>\s*<\/file_action>/g;
      const edits = [...replyContent.matchAll(editRegex)];
      
      if (edits.length === 0) {
         throw new Error("LỖI CÚ PHÁP AI: Bạn chưa cung cấp khối <file_action path='...'><search>...</search><replace>...</replace></file_action>.");
      }

      let patchErrors = [];
      let touchedPaths: string[] = []; 
      
      for (const ed of edits) {
          const filePath = ed[1] ? path.join(workspace, ed[1]) : targetPath;
          touchedPaths.push(filePath);
          const searchChunk = ed[2].trim();
          const replaceChunk = ed[3].trim();
          
          let fileCode = "";
          if (fs.existsSync(filePath)) {
              fileCode = fs.readFileSync(filePath, "utf-8");
          } else {
              if (searchChunk === "") { fileCode = replaceChunk; }
          }
          
          if (searchChunk !== "") {
              if (fileCode.includes(searchChunk)) {
                  fileCode = fileCode.replace(searchChunk, replaceChunk);
              } else {
                  const escapedSearch = searchChunk.replace(/[.*+?^$\{}()|[\]\\]/g, '\\$&').replace(/\s+/g, '\\s*');
                  try {
                      const fuzzyRegex = new RegExp(escapedSearch, 'g');
                      if (fuzzyRegex.test(fileCode)) {
                          fileCode = fileCode.replace(fuzzyRegex, replaceChunk);
                      } else throw new Error();
                  } catch(e) {
                      patchErrors.push(`[FilePath: ${ed[1] || 'TargetFile'}] Code gốc không tồn tại mỏ neo:\n${searchChunk.substring(0, 50)}...\n\n[BỐI CẢNH 40 DÒNG (Sliding Window)]:\n${extractSlidingWindowWithLines(searchChunk, fileCode)}`);
                  }
              }
          }
          
          fs.writeFileSync(filePath, fileCode, "utf8");
      }

      if (patchErrors.length > 0) {
          throw new Error(`LỖI TÌM/THAY THẾ (Line-Shift Error):\n${patchErrors.join('\n')}\nHãy copy chính xác đoạn cũ làm <search>!`);
      }

      const testRegex = /<test_case>([\s\S]*?)<\/test_case>/;
      const testMatch = replyContent.match(testRegex);
      if (!testMatch) {
         throw new Error("LỖI TỬ HUYỆT: Bạn BẮT BUỘC phải viết mã kiểm thử tốc độ bằng performance.now() vào thẻ <test_case>.");
      }
      
      fs.writeFileSync(testPath, testMatch[1].trim(), "utf8");

      console.log(`\x1b[33m[Nodes: check_code] Bắt đầu Kiểm thử Đấu trường sinh tồn trên Nhánh Song song...\x1b[0m`);
      
      await execAsync(`npx tsx ${testPath}`, { 
          cwd: workspace, 
          timeout: 10000,
          env: { ...process.env, NODE_OPTIONS: "--max-old-space-size=1024" }
      });
      
      console.log(`\x1b[33m🟢 Local PASS. Chạy Global Type Check chống vỡ dây chuyền Đa Tệp...\x1b[0m`);
      try {
          await execAsync(`npx tsc --noEmit`, { cwd: workspace });
      } catch (tscErr) {
          throw new Error(`[LỖI SẬP DÂY CHUYỀN]: Mã bạn sửa tệp này đã làm hỏng Cú pháp của File khác. Lỗi TSC:\n${tscErr}`);
      }

      console.log(`\x1b[32m🟢 [Trạng Thái Hộp Cát Vĩ Mô]: SUCCESS & TYPECHECK PASS! Hợp nhất (Merge) Lõi!\x1b[0m`);
      report += `🟢 [Hộp Cát Git]: XANH (PASS UNIT TEST & TYPECHECK)\n`;
      
      try {
         await execAsync(`git add . && git commit -m "[LIVA Macro-Evolution] ${args.goal.replace(/"/g, "'").slice(0, 50)}..."`, { cwd: workspace });
         await execAsync(`git checkout main`, { cwd: workspace });
         await execAsync(`git merge ${branchName}`, { cwd: workspace });
         console.log(`\x1b[35m🦊 [Git Versioning] Đã dán mác Nhánh Tiến Hóa vào Main Lõi!\x1b[0m`);
      } catch (e) {}
      
      mergedSuccess = true;
      break; 

    } catch (err: any) {
      execErr = err;

      let errMsg = execErr.stderr || execErr.message || execErr.stdout || String(execErr);
      
      if (execErr.killed || errMsg.includes("timeout")) {
          errMsg = `Error: Test execution timed out (>10000ms). Mã chạy chậm hoặc vướng Infinite loop!`;
      }
      
      console.log(`\x1b[31m🔴 [Hộp Cát FAILED]: Sinh tồn thất bại! Kích hoạt Khâu Vá (Patching)!\x1b[0m`);
      console.log(`\x1b[31mLỗi: ${errMsg.slice(0, 400)}\x1b[0m`);

      const shortError = errMsg.split('\n')[0].replace(/[^a-zA-Z0-9 ]/g, " ").slice(0, 60);
      errorFingerprints.push(shortError);
      
      if (currentCycle >= MAX_CYCLES) {
          if (isGitSwapped) {
              try {
                  await execAsync(`git reset --hard && git checkout main && git branch -D ${branchName}`, { cwd: workspace });
                  console.log(`\x1b[31m[Sandbox Rollback]: Đã thử khâu vá 3 lần không qua. Khôi phục về Lõi An Toàn.\x1b[0m`);
              } catch(e) {}
          }
          break;
      }
      
      let gitDiff = "";
      try {
          const { stdout } = await execAsync(`git diff HEAD`, { cwd: workspace });
          gitDiff = stdout;
      } catch(e) {}

      let feedbackPrompt = `[VÁ LỖI LẦN ${currentCycle}/${MAX_CYCLES}]\nBiên dịch/Test thất bại:\n${errMsg}\n`;
      if (gitDiff.trim().length > 0) {
          feedbackPrompt += `\nMã nguồn CỦA BẠN đã được GIỮ NGUYÊN trên file. Đây là phần bạn vừa sửa làm hỏng (Git Diff):\n${gitDiff.slice(0, 1500)}\n`;
      }
      
      feedbackPrompt += `\nNhiệm vụ: Dùng công cụ sửa trực tiếp trên file đang hỏng này. Bắn chính xác số dòng để vá lỗi.\n(Hoặc trả về [ABORT_AND_ROLLBACK] nếu logic hiện tại đã vỡ nát không thể vá và bạn muốn đập đi làm lại).`;
      
      conversation.push({ role: "user", content: feedbackPrompt });
      currentCycle++;
    }
  }

  if (fs.existsSync(testPath)) fs.unlinkSync(testPath);

  if (!mergedSuccess) {
      report += `\n[Hệ Thống] Kích Hoạt Hình Phạt: Tiến hóa Thất Bại!`;
  }
  return report;
}

export const metadata = {
    name: "liva_ai_scientist",
    search_keywords: ["liva_ai_scientist","liva ai scientist", "tự tối ưu", "tự tiến hóa", "jarvis loop"],
    description: "Kỹ năng Đặc Vụ Kỹ Sư J.A.R.V.I.S 3.0 (Expert 26B). Khả năng tự thiết kế, sửa đa tệp nhánh Git và tối ưu hệ thống triệt để sinh tồn vĩ mô.",
    parameters: {
      type: "object",
      properties: {
        goal: { type: "string" },
        targetFilePath: { type: "string" },
        testCommand: { type: "string" }
      },
      required: ["goal", "targetFilePath", "testCommand"],
    },
};
