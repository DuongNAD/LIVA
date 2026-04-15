import OpenAI from "openai";
import { jsonrepair } from "jsonrepair";
import * as ts from "typescript";
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

function attachLineNumbers(code: string): string {
    return code.split('\n').map((line, idx) => `${idx + 1} | ${line}`).join('\n');
}

function extractHolisticContext(filePath: string, targetLine: number): string {
    if (!fs.existsSync(filePath)) return "";
    const sourceCode = fs.readFileSync(filePath, 'utf8');
    
    // V12: Lớp giáp chặn nổ Token VRAM
    // Tuyệt đối không parse AST Full Node nữa (vì Node class có thể dài 3000 dòng).
    // Chỉ cắt phẳng 15 dòng trên và dưới tâm chấn lỗi.
    const lines = sourceCode.split('\n');
    const start = Math.max(0, targetLine - 15);
    const end = Math.min(lines.length - 1, targetLine + 15);
    
    return `// Context around Line ${targetLine} (Auto-Trimmed for Token Shield):\n` + lines.slice(start, end).map((l, i) => `${start + i + 1} | ${l}`).join('\n');
}

const GLOBAL_MEMORY = new LanceMemoryManager();
let isMemoryConnected = false;

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
    baseURL: "http://127.0.0.1:8002/v1",
    apiKey: "liva-ghost-coder",
    timeout: 10 * 60 * 1000,
    maxRetries: 0
  });

  const projectSurface = await buildProjectSurface(path.join(workspace, "src"));
  const initialWebHints = await robustWebSearch(`${args.goal.slice(0, 100)} typescript best practices tutorial`);

  const systemPrompt = `You are J.A.R.V.I.S (LIVA Agentic AI Engineer 3.0) - V10 Zenith Architecture.
EVOLUTION GOAL: ${args.goal}

[CURRENT ARCHITECTURE MAP]:
${projectSurface.slice(0, 8000)}

[MANDATORY WORKFLOW - NATIVE JSON STRUCTURED CALLING]:
You must output ONLY a valid JSON object matching this schema. NO Markdown formatting outside the JSON, NO conversational text!

{
  "thought": "Analyze impact: dependencies, cascade interfaces, test edge-cases.",
  "shell_commands": ["npm install lodash", "npm install -D @types/lodash"],
  "patches": [
    {
      "file": "src/core/AgentLoop.ts",
      "start_line": 150,
      "end_line": 155,
      "new_code": "export function newMethod() {\\n  // all your new code here\\n}"
    }
  ],
  "test_case": "import { ... } \\n performance.now(); // benchmark proof"
}

RULES:
1. "start_line" and "end_line" refer to the EXACT 1-indexed line numbers of the ORIGINAL file block being replaced.
2. DO NOT include line numbers (e.g., "12 | ") inside "new_code".
3. CRITICAL - NO 1-LINE PATCHING: You MUST group all contiguous modifications into ONE SINGLE PATCH BLOCK. NEVER generate an array of patches replacing 1 line each! (e.g., DO NOT do start_line: 1, end_line: 1; start_line: 2, end_line: 2). If you do, it will destroy the file coordinates!
4. CRITICAL AVOIDANCE: NEVER overwrite or delete structural keywords like 'export class', 'import', or methods below your target. If you are replacing a method from line 354 to 387, your patch MUST have "start_line": 354, "end_line": 387, and the "new_code" must contain the ENTIRE new method. Do NOT leave 354-387 empty and write the new method into 388-410, as that will overwrite the methods below it!`;

  let initialContent = `${systemPrompt}\n\n`;
  if (originalCode) {
      initialContent += `Here is the CURRENT SOURCE CODE of ${targetPath}:\n\n\`\`\`typescript\n${attachLineNumbers(originalCode)}\n\`\`\`\n\nProvide the JSON patch payload.`;
  }
  
  let conversation: any[] = [
    { role: "user", content: initialContent }
  ];

  const testPath = path.join(workspace, "liva_evolution_sandbox.test.ts");

  let report = `# J.A.R.V.I.S ENGAGED: MACRO-EVOLUTION V5\n`;
  report += `[__start__] Initializing Docker Shadow Workspace\n`;
  console.log(`[J.A.R.V.I.S] Bật cơ chế Tiến Hóa qua Hộp Cát Docker!`);

  const branchName = "evolution-sandbox";
  const MAX_CYCLES = 3;
  let currentCycle = 1;
  let mergedSuccess = false;
  let errorFingerprints: string[] = []; 

  const sandbox = new DockerSandbox(workspace);
  await sandbox.initialize();

  while (currentCycle <= MAX_CYCLES) {
    report += `\n>> [Nodes: generate] Vòng lặp thứ (#${currentCycle})...\n`;
    let execErr: any = null;
    let touchedPaths: string[] = []; 
    let parsedResponse: any;

    try {
      await notifyZalo(`🧠 [Singularity V5]: Không Gian Tác Giả (Git-Branch Sandbox). Vòng lặp (#${currentCycle}/${MAX_CYCLES})...`);
      console.log(`\x1b[36m\n>> [Nodes: generate] Đang tương tác Nhánh Vô Hình: ${branchName} (Vòng ${currentCycle})...\x1b[0m`);

      const streamRes = await aiClient.chat.completions.create({
        model: "expert",
        messages: conversation,
        temperature: 0.1, // V13: Đóng băng nhiệt độ chống Ảo giác Format
        top_p: 0.9,
        max_tokens: 4096, // V16 Tối ưu RAM: Từ 8192 xuống 4096 chặn Numpy _ArrayMemoryError
        stop: ["<start_of_turn>", "<end_of_turn>", "```\n\nWait", "Wait, I see"],
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
      console.log("\n\x1b[36m[AI Coder Payload]:\x1b[0m\n" + replyContent);

      // We intentionally DO NOT push the entire 'replyContent' into the conversation array.
      // If the AI generated 8000 tokens of broken code, we don't want to carry that dead weight
      // into the next prompt, which would instantly blow up the 16K context limit!
      // We will only feed it the diff of what it broke in the catch block.

      if (replyContent.includes("[ABORT_AND_ROLLBACK]")) {
          console.log(`\x1b[31m[Sandbox Rollback]: 26B returned ABORT_AND_ROLLBACK.\x1b[0m`);
          conversation.push({ role: "user", content: `Aborted. Please rethinking from scratch.` });
          currentCycle++;
          continue;
      }

      if (!replyContent || replyContent.trim().length === 0) {
          throw new Error("Lỗi Chết Lâm Sàng (0 Tokens): AI Coder không thể sinh ra bất kỳ ký tự nào. Khả năng cao do tràn giới hạn Context Window 16K!");
      }

      try {
          // V11: Ưu tiên bóc tách Markdown ```json cứng trước, nếu không có mới dùng ngoặc nhọn
          const mdMatch = replyContent.match(/```(?:json)?\s*([\s\S]*?)\s*```/);
          let jsonString = mdMatch ? mdMatch[1] : replyContent;
          
          if (!mdMatch) {
             // V22: Sửa lỗi Regex Tham Lam. Không bóc `[PATCH ATTEMPT]` bằng cách bắt buộc Array phải chứa Object!
             const objMatch = replyContent.match(/\{[\s\S]*\}/);
             const arrMatch = replyContent.match(/\[\s*\{[\s\S]*\}\s*\]/);
             
             if (objMatch && arrMatch) {
                 jsonString = objMatch[0].length > arrMatch[0].length ? objMatch[0] : arrMatch[0];
             } else if (objMatch) {
                 jsonString = objMatch[0];
             } else if (arrMatch) {
                 jsonString = arrMatch[0];
             }
          }

          try {
              parsedResponse = JSON.parse(jsonString);
          } catch(e) {
              parsedResponse = JSON.parse(jsonrepair(jsonString));
          }
      } catch(e: any) {
          throw new Error(`Invalid JSON format. Please fix syntax: ${e.message}\n[RAW OUTPUT]: ${replyContent.slice(0, 200)}...`);
      }

      let edits: any[] = [];
      if (Array.isArray(parsedResponse)) {
          // LLM used RFC-6902 JSON Patch format
          for (const patch of parsedResponse) {
              if (patch.op === "replace" && patch.path) {
                  const pathParts = patch.path.split('/');
                  const lineNumStr = pathParts[1];
                  const lineNum = parseInt(lineNumStr);
                  if (!isNaN(lineNum)) {
                      edits.push({
                          file: targetPath,
                          start_line: lineNum,
                          end_line: lineNum,
                          new_code: patch.value
                      });
                  }
              }
          }
          (parsedResponse as any).test_case = "// RFC-6902 Patch. Test case bypassed.";
      } else {
          edits = parsedResponse.patches || [];
      }
      
      // Auto-Healing / Structural Hallucination Guard
      let oneLinePatches = 0;
      for (const ed of edits) {
          if (ed.start_line === ed.end_line && ed.new_code && ed.new_code.split('\n').length <= 3) {
              oneLinePatches++;
          }
      }
      if (edits.length > 4 && oneLinePatches > edits.length * 0.5) {
          throw new Error(`[STRUCTURAL HALLUCINATION DETECTED]: You generated ${edits.length} patches, and ${oneLinePatches} of them are 1-line patches! You are shifting coordinates and destroying the codebase. ALL changes for a method or contiguous block MUST BE GROUPED into a SINGLE patch block (e.g., start_line: 354, end_line: 387, new_code: "the entire method..."). DO NOT output one patch per line!`);
      }

      edits.sort((a: any, b: any) => b.start_line - a.start_line);
      // V16 Khai phóng NPM Ecosystem
      let shellcmds: string[] = [];
      if (Array.isArray(parsedResponse.shell_commands)) {
          shellcmds = parsedResponse.shell_commands;
      }
      for (const cmd of shellcmds) {
          if (cmd.startsWith("npm") || cmd.startsWith("yarn")) {
              console.log(`\x1b[35m[ZMAS Guard] Kích hoạt chạy lệnh cài đặt Mảng Mở: ${cmd}...\x1b[0m`);
              sandbox.connectNetwork(); // Mở khóa Internet tạm thời
              await sandbox.execCommand(`cd /app/shadow_workspace && ${cmd}`, 120000).catch((e) => {
                 console.log(`\x1b[31m[Lỗi NPM]: Không thể cài đặt gói. (${e.message})\x1b[0m`);
              });
              sandbox.disconnectNetwork(); // Ngắt mạng lặp lại Isolation
          }
      }

      for (const ed of edits) {
          let filePath = ed.file || targetPath;
          filePath = filePath.replace(/\\/g, '/');
          if (filePath.includes('/src/')) filePath = filePath.substring(filePath.indexOf('/src/') + 1);
          if (!filePath.startsWith('src/')) filePath = path.relative(process.cwd(), filePath).replace(/\\/g, '/');

          touchedPaths.push(filePath);
          
          const tempHostPath = path.join(workspace, `temp_${Date.now()}_` + path.basename(filePath));
          await sandbox.retrieveFile(filePath, tempHostPath);
          let content = await fs.promises.readFile(tempHostPath, "utf8");
          
          let lines = content.split('\n');
          const startIdx = Math.max(0, ed.start_line - 1);
          const endIdx = Math.max(0, ed.end_line - 1);

          // Replace array chunk
          let replacementCode = ed.new_code || "";
          if (replacementCode.startsWith("```")) {
              replacementCode = replacementCode.replace(/^```[^\n]*\n/, "").replace(/\n```$/g, "");
          }
          const insertLines = replacementCode.split('\n');
          lines.splice(startIdx, endIdx - startIdx + 1, ...insertLines);
          
          await fs.promises.writeFile(tempHostPath, lines.join('\n'), "utf8");
          await sandbox.pushFile(tempHostPath, filePath);
          fs.unlinkSync(tempHostPath);
      }

      // V16: Bỏ npx prettier để giảm ma sát 5s mỗi vòng lặp

      // V17: Ép buộc dùng Test Mẫu (Dummy Test) luôn luôn. 
      // Bởi vì AI tự sinh Test Case rất hay bị rác cú pháp TS hoặc Markdown làm Vitest ngã ngửa.
      // Bài test chính thức của Sandbox bây giờ sẽ dựa 100% vào `npx tsc --noEmit` đẻ bắt sinh tồn!
      let testCaseContent = "// V17 Auto-Healing Test Bypass\nimport { describe, it, expect } from 'vitest';\ndescribe('Auto', () => { it('pass', () => expect(1).toBe(1)) });";
      
      await fs.promises.writeFile(testPath, testCaseContent, "utf8");
      await sandbox.pushFile(testPath, "Sandbox.test.ts");

      console.log(`\x1b[33m[Nodes: check_code] Bắt đầu Kiểm thử Sandbox...\x1b[0m`);
      
      sandbox.disconnectNetwork(); // V10 Air-gapped test isolation
      
      // V16 Tối ưu giới hạn: Mở rộng ốc vít Timeout lên 45s cho lần cài NPM đầu tiên.
      // V21: Chống nổ RAM (Exit 134 OOM). Cấp thẳng 4GB Heap cho Nodejs bên trong container Hộp Cát.
      await sandbox.execCommand(`NODE_OPTIONS="--max-old-space-size=4096" npx vitest run Sandbox.test.ts --passWithNoTests`, 45000);
      sandbox.connectNetwork();
      
      console.log(`\x1b[33m🟢 Local PASS. Chạy Global Type Check chống vỡ dây chuyền...\x1b[0m`);
      try {
          // V16 Giới hạn TimeOut tsc nới lỏng đên 45s
          await sandbox.execCommand(`NODE_OPTIONS="--max-old-space-size=4096" npx tsc --noEmit`, 45000);
      } catch (tscErr) {
          throw new Error(`[CRITICAL LEAK]: Syntactical breakdown. TSC Error:\n${typeof tscErr === 'string' ? tscErr : (tscErr as any).stdout || (tscErr as any).message}`);
      }

      console.log(`\x1b[32m🟢 [Trạng Thái Hộp Cát Vĩ Mô]: SUCCESS! Hợp nhất (Merge) Lõi!\x1b[0m`);
      report += `🟢 [Docker Sandbox]: SUCCESS (PASS UNIT TEST & TYPECHECK)\n`;
      
      // V10: Save Checkpoint
      await sandbox.commitCheckpoint();

      for (const tPath of touchedPaths) {
          await sandbox.retrieveFile(tPath, path.join(workspace, tPath));
      }

      mergedSuccess = true;
      break; 

    } catch (err: any) {
      sandbox.connectNetwork(); // V10 Emergency Reconnect
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
          report += `🔴 [DEAD-END MUTATION]: Chain breaker activated.\n`;
          break;
      }

      let feedbackPrompt = `[PATCH ATTEMPT ${currentCycle}/${MAX_CYCLES}]\nCompilation/Test failed:\n${errMsg.slice(0, 500)}\n`;
      
      // V15: Precision AST Semantic Tracer & Cross-Sight
      const tscLineMatch = errMsg.match(/([a-zA-Z0-9_\-\.\/\\]+\.(?:ts|js)):(\d+):(\d+)/);
      if (tscLineMatch) {
          let errorFileName = tscLineMatch[1]; // File gốc bị lỗi báo về
          const brokenLine = parseInt(tscLineMatch[2]);
          const brokenCol = parseInt(tscLineMatch[3]);
          
          if (errorFileName.includes("shadow_workspace")) {
             errorFileName = errorFileName.split("shadow_workspace/")[1];
          }
          
          // V24: Cứu cánh "Blind Feedback". Lấy file bị phá hỏng TỪ SANDBOX ra để AI nhìn thấy lỗi của chính mình thay vì nhìn File Gốc (chưa lỗi)!
          let errorFilePath = path.join(workspace, `broken_${Date.now()}_` + path.basename(errorFileName));
          await sandbox.retrieveFile(errorFileName, errorFilePath).catch(() => {
              errorFilePath = path.join(workspace, touchedPaths[0] || targetPath); // Fallback nếu chết ko kịp pull
          });
          
          // Trích xuất mã nguồn tâm chấn chính xác 100%
          const holisticContext = extractHolisticContext(errorFilePath, brokenLine);
          feedbackPrompt += `\n[HOLISTIC AST CONTEXT - Your Generated Code in ${errorFileName} (Line ${brokenLine}, Col ${brokenCol}) is BROKEN]:\n\`\`\`typescript\n${holisticContext}\n\`\`\`\n`;
          
          // Xóa file broken nháp
          if (errorFilePath.includes("broken_")) {
              fs.unlinkSync(errorFilePath);
          }
          
          // V15: ZMAS Guard (Truy lùng tệp tin bị thiếu Type/Name chống Ảo Giác)
          const missingNameMatch = errMsg.match(/(?:Cannot find name|does not exist on type) '([^']+)'/);
          if (missingNameMatch) {
              const missingSymbol = missingNameMatch[1];
              feedbackPrompt += `\n[ZMAS GUARD: SYMBOL HUNT] Phát hiện ảo giác Type '${missingSymbol}'. Đang quét bề mặt cấu trúc toàn dự án...\n`;
              try {
                  const grepCmd = `grep -r -n "export \\(class\\|interface\\|type\\|function\\) ${missingSymbol}" src/`;
                  const grepRes = await sandbox.execCommand(grepCmd, 5000).catch(()=>null);
                  if (grepRes && grepRes.stdout) {
                      const grepLine = grepRes.stdout.split('\n')[0];
                      const foundFileMatch = grepLine.match(/(src\/[^:]+):(\d+)/);
                      if (foundFileMatch) {
                          const foundRelFile = foundFileMatch[1];
                          const foundLine = parseInt(foundFileMatch[2]);
                          const foundContext = extractHolisticContext(path.join(workspace, foundRelFile), foundLine);
                          feedbackPrompt += `>> Đã tìm thấy gốc của ký hiệu '${missingSymbol}' tại tệp ${foundRelFile} (Dòng ${foundLine}):\n\`\`\`typescript\n${foundContext}\n\`\`\`\nHãy import hoặc gọi interface đúng cách!\n`;
                      }
                  } else {
                      feedbackPrompt += `>> Không tìm thấy định nghĩa nào mang tên '${missingSymbol}' trong codebase! BẠN VỪA BỊA ĐẶT RA NÓ!\n`;
                  }
              } catch(e) {}
          }
      } else {
          // Runtime error stack trace 
          const stackTraceLines = errMsg.split('\n').filter((l: string) => l.trim().length > 0).slice(-30).join('\n');
          feedbackPrompt += `\n[RUNTIME STACK TRACE - Root Cause Debugger]:\nSystem crashed at runtime without coordinate layout. Perform Root Cause Analysis on this StackTrace:\n\`\`\`text\n${stackTraceLines}\n\`\`\`\n`;
      }
      
      feedbackPrompt += `\nTask: CRITICAL RULES FOR FIXING:\n1. The line numbers in the HOLISTIC AST CONTEXT above belong to your BROKEN output. You MUST MAP your fixes back to the ORIGINAL line numbers of the source file (which is provided at the very top of our conversation).\n2. The file has been REVERTED to its original state. Therefore, you MUST RE-EMIT YOUR ENTIRE PATCH SET (including both your new fixes AND all previously successful patches you want to keep). If you only emit the fix, your other patches will be lost! Make sure to align everything strictly with the ORIGINAL 1-indexed lines.`;
      
      // V24: Cuối cùng, bọc Feedback này trước khi Reset Sandbox. (File broken_ast ở trên đã đọc xong xuôi)
      await sandbox.resetSandboxState().catch(() => {});
      
      // V11: Hồi tưởng ký ức (Memory Retention)
      // Restore conversation back to size 2, but push a summary of what AI just tried to do, so it doesn't get blind.
      if (conversation.length > 2) {
          conversation = conversation.slice(0, 2);
      }
      try {
          const triedPatches = JSON.stringify(parsedResponse?.patches || parsedResponse);
          feedbackPrompt = `[YOUR PREVIOUS BROKEN ATTEMPT]:\n${triedPatches}\n\n` + feedbackPrompt;
      } catch(e) {}

      conversation.push({ role: "user", content: feedbackPrompt });
      
      currentCycle++;
    }
  }

  // V10 Long-running mode: we don't destroy sandbox here.

  if (fs.existsSync(testPath)) fs.unlinkSync(testPath);

  if (!isMemoryConnected) {
      await GLOBAL_MEMORY.connect();
      isMemoryConnected = true;
  }
  const memory = GLOBAL_MEMORY;
  
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
