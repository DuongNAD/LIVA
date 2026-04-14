import OpenAI from "openai";
import * as fs from "fs";
import * as path from "path";
import { exec } from "child_process";
import { promisify } from "util";
import axios from "axios";
import { DockerSandbox } from "../utils/DockerSandbox.js";
import { LanceMemoryManager } from "../memory/LanceMemory.js";

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

  let report = `# J.A.R.V.I.S ENGAGED: MACRO-EVOLUTION V5\n`;
  report += `[__start__] Initializing Docker Shadow Workspace\n`;
  console.log(`[J.A.R.V.I.S] Bật cơ chế Tiến Hóa qua Hộp Cát Docker!`);

  const MAX_CYCLES = 3;
  let currentCycle = 1;
  let mergedSuccess = false;
  let errorFingerprints: string[] = []; 

  const sandbox = new DockerSandbox(workspace);
  await sandbox.initialize();

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
          console.log(`\x1b[31m[Sandbox Rollback]: 26B returned ABORT_AND_ROLLBACK.\x1b[0m`);
          conversation.push({ role: "user", content: `Aborted. Please rethinking from scratch.` });
          currentCycle++;
          continue;
      }

      const patternRegex = /<file_action(?:\s+path="([^"]+)")?\s+engine="([^"]+)">\s*<pattern>\s*([\s\S]*?)\s*<\/pattern>\s*<rewrite>\s*([\s\S]*?)\s*<\/rewrite>\s*<\/file_action>/gi;
      const edits = [...replyContent.matchAll(patternRegex)];
      
      if (edits.length === 0) {
         throw new Error("SYNTAX ERROR: You must provide a <file_action path='...' engine='ast-grep'><pattern>...</pattern><rewrite>...</rewrite></file_action> block.");
      }

      let touchedPaths: string[] = []; 
      
      for (const ed of edits) {
          const filePath = ed[1] || targetPath;
          const engine = ed[2];
          const patternChunk = ed[3];
          const rewriteChunk = ed[4];
          touchedPaths.push(filePath);
          
          if (engine === "ast-grep") {
              const rule = {
                  id: "liva-patch",
                  language: "typescript",
                  rule: { pattern: patternChunk },
                  fix: rewriteChunk
              };
              await sandbox.execCommand(`echo '${Buffer.from(JSON.stringify(rule)).toString('base64')}' | base64 -d > /tmp/sg.yml`);
              await sandbox.execCommand(`npx ast-grep run -config /tmp/sg.yml -U --update-all /app/shadow_workspace/${filePath}`).catch(()=>true);
          }
      }

      const testRegex = /<test_case>\s*([\s\S]*?)\s*<\/test_case>/i;
      const testMatch = replyContent.match(testRegex);
      if (!testMatch) {
         throw new Error("CRITICAL FATAL: You MUST write the performance benchmark in <test_case>.");
      }
      
      await fs.promises.writeFile(testPath, testMatch[1].trim(), "utf8");
      await sandbox.execCommand(`docker cp ${testPath} ${sandbox.getContainerId()}:/app/shadow_workspace/Sandbox.test.ts`);

      console.log(`\x1b[33m[Nodes: check_code] Bắt đầu Kiểm thử Sandbox...\x1b[0m`);
      
      await sandbox.execCommand(`npx vitest run Sandbox.test.ts --passWithNoTests`, 10000);
      
      console.log(`\x1b[33m🟢 Local PASS. Chạy Global Type Check chống vỡ dây chuyền...\x1b[0m`);
      try {
          await sandbox.execCommand(`npx tsc --noEmit`);
      } catch (tscErr) {
          throw new Error(`[CRITICAL LEAK]: Syntactical breakdown in other files. TSC Error:\n${tscErr}`);
      }

      console.log(`\x1b[32m🟢 [Trạng Thái Hộp Cát Vĩ Mô]: SUCCESS! Hợp nhất (Merge) Lõi!\x1b[0m`);
      report += `🟢 [Docker Sandbox]: SUCCESS (PASS UNIT TEST & TYPECHECK)\n`;
      
      for (const tPath of touchedPaths) {
          await sandbox.retrieveFile(tPath, path.join(workspace, tPath));
      }

      mergedSuccess = true;
      break; 

    } catch (err: any) {
      execErr = err;

      let errMsg = execErr.stderr || execErr.message || execErr.stdout || String(execErr);
      
      if (execErr.killed || errMsg.includes("timeout")) {
          errMsg = `Error: Test execution timed out (>10000ms). Infinite loop detected!`;
      }
      
      console.log(`\x1b[31m🔴 [Hộp Cát FAILED]: Sinh tồn thất bại! Kích hoạt Khâu Vá (Patching)!\x1b[0m`);
      console.log(`\x1b[31mLỗi: ${errMsg.slice(0, 400)}\x1b[0m`);

      const shortError = errMsg.split('\n')[0].replace(/[^a-zA-Z0-9 ]/g, " ").slice(0, 60);
      errorFingerprints.push(shortError);
      
      if (currentCycle >= MAX_CYCLES) {
          report += `🔴 [DEAD-END MUTATION]: Chain breaker activated. Destroying Container!\n`;
          break;
      }
      
      let gitDiff = "";
      try {
          const { stdout } = await sandbox.execCommand(`cd /app/shadow_workspace && git diff HEAD`);
          gitDiff = stdout;
      } catch(e) {}

      let feedbackPrompt = `[PATCH ATTEMPT ${currentCycle}/${MAX_CYCLES}]\nCompilation/Test failed:\n${errMsg}\n`;
      if (gitDiff.trim().length > 0) {
          feedbackPrompt += `\nYOUR CODE HAS BEEN RETAINED. This is what you modified that caused the breakage (Git Diff):\n${gitDiff.slice(0, 1500)}\n`;
      }
      
      feedbackPrompt += `\nTask: Target the exact broken lines using ast-grep and fix them. Do NOT return old search/replace blocks!`;
      
      conversation.push({ role: "user", content: feedbackPrompt });
      currentCycle++;
    }
  }

  await sandbox.destroy();
  if (fs.existsSync(testPath)) fs.unlinkSync(testPath);

  const memory = new LanceMemoryManager();
  
  if (!mergedSuccess) {
      report += `\n[Hệ Thống] Kích Hoạt Hình Phạt: Tiến hóa Thất Bại!`;
      await memory.addMemory("DEAD-END", `Evolution failed on ${targetPath}.\nReport: ${report.slice(-1000)}`, targetPath);
  } else {
      await memory.addMemory("SUCCESS", `Evolution successful on ${targetPath}.\nReport: ${report.slice(-1000)}`, targetPath);
  }
  return report;
}

export const metadata = {
    name: "liva_ai_scientist",
    search_keywords: ["liva_ai_scientist","liva ai scientist", "tự tối ưu", "tự tiến hóa", "jarvis loop"],
    description: "Kỹ năng Đặc Vụ Kỹ Sư Jarvis 3.0 (Expert 26B). Khả năng tự thiết kế, sửa đa tệp nhánh Git và tối ưu hệ thống triệt để sinh tồn vĩ mô.",
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
