import { exec } from "child_process";
import { promisify } from "util";
import fs from "fs";
import path from "path";
import OpenAI from "openai";
import axios from "axios";
import { notifyZalo } from "../utils/ZaloNotifier";

const execAsync = promisify(exec);

async function buildProjectSurface(dirPath: string, rootDir: string = dirPath): Promise<string> {
    let result = "";
    try {
        const files = fs.readdirSync(dirPath, { withFileTypes: true });
        for (const file of files) {
            if (file.name.startsWith("node_modules") || file.name.startsWith("dist") || file.name.startsWith(".git") || file.name.endsWith(".sandbox.ts") || file.name.endsWith(".bak")) {
                continue;
            }
            const fullPath = path.join(dirPath, file.name);
            const relativePath = path.relative(rootDir, fullPath);
            if (file.isDirectory()) {
                result += await buildProjectSurface(fullPath, rootDir);
            } else if (file.name.endsWith(".ts")) {
                result += `📄 ${relativePath.replace(/\\/g, '/')}`;
                try {
                    const content = fs.readFileSync(fullPath, "utf-8");
                    const matches = [...content.matchAll(/export\s+(class|interface|type|enum|function|const|let|var)\s+([A-Za-z0-9_]+)/g)];
                    if (matches.length > 0) {
                        result += ` (Exports: ${matches.map(m => m[2]).join(", ")})\n`;
                    } else {
                        result += `\n`;
                    }
                } catch(e) {}
            }
        }
    } catch (error) { }
    return result;
}

async function robustWebSearch(query: string): Promise<string> {
    let hints = "Không có thông tin Web dự phòng.";
    try {
        const res = await axios.post("https://lite.duckduckgo.com/lite/", `q=${encodeURIComponent(query)}`, {
            headers: {
                "Content-Type": "application/x-www-form-urlencoded",
                "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/114.0.0.0 Safari/537.36"
            },
            timeout: 10000
        });
        const html = res.data;
        // DDG Lite returns snippets in td class='result-snippet'
        const snippetMatches = [...html.matchAll(/<td class='result-snippet'>([\s\S]*?)<\/td>/gi)];
        if (snippetMatches.length > 0) {
             hints = snippetMatches.slice(0, 5).map((m, i) => `- Web Hint ${i+1}: ${m[1].replace(/<\/?[^>]+(>|$)/g, "").trim()}`).join("\n");
        } else {
             hints = "Web Search thành công nhưng không tìm thấy đoạn Snippet phù hợp.";
        }
    } catch (e: any) {
        console.log(`\x1b[90m[Web Intelligence]: Lỗi mạng rớt Web Search (${e.message}).\x1b[0m`);
    }
    return hints;
}

export const metadata = {
  name: "liva_ai_scientist",
  search_keywords: ["liva_ai_scientist","liva ai scientist", "tự tối ưu", "tự tiến hóa", "jarvis loop"],
  description:
    "Kỹ năng Đặc Vụ Kỹ Sư J.A.R.V.I.S 3.0 (Expert 26B). Khả năng tự đọc Bản đồ Dự Án (API Surface), tự Search Mạng (DuckDuckGo Lite) để cập nhật kiến thức Typescript mới nhất ngay trước khi code, và tự tiến hóa vòng lặp (Singularity Loop).",
  parameters: {
    type: "object",
    properties: {
      goal: {
        type: "string",
        description: "Mục tiêu tối ưu hóa, tái cấu trúc thuật toán hoặc tích hợp tính năng mới vào Lõi.",
      },
      targetFilePath: {
        type: "string",
        description: "Đường dẫn (tuyệt đối hoặc tương đối) đến file dự án THẬT CẦN TIẾN HÓA. VD: src/core/AgentLoop.ts.",
      },
      testCommand: {
        type: "string",
        description: "Lệnh sẽ chạy để kểm tra file (Sandbox). VD: 'npx tsc --noEmit' hoặc 'npm run build'.",
      },
      workingDirectory: {
         type: "string",
         description: "Đường dẫn gốc để chạy lệnh testCommand. VD: E:/Project/LIVA/openclaw-gateway."
      }
    },
    required: ["goal", "targetFilePath", "testCommand", "workingDirectory"],
  },
};

export const execute = async (args: {
  goal: string;
  targetFilePath: string;
  testCommand: string;
  workingDirectory: string;
}): Promise<string> => {
  const workspace = args.workingDirectory;
  if (!fs.existsSync(workspace)) {
    return `[Hệ thống]: Không tìm thấy thư mục làm việc ${workspace}. Hãy kiểm tra đường dẫn.`;
  }

  if (!args.targetFilePath || typeof args.targetFilePath !== "string") {
    return `[Hệ thống]: Không tìm thấy targetFilePath hợp lệ. Thiếu cấu hình mục tiêu.`;
  }
  
  if (!args.testCommand || typeof args.testCommand !== "string") {
    return `[Hệ thống]: AI thiếu testCommand để kiểm tra tính hợp lệ của mã. Bắt buộc có testCommand.`;
  }

  let targetPath = args.targetFilePath;
  if (!path.isAbsolute(targetPath)) {
    targetPath = path.join(workspace, targetPath);
  }

  // Khởi tạo Sandbox (Môi trường cách ly)
  const dirName = path.dirname(targetPath);
  const extName = path.extname(targetPath);
  const baseName = path.basename(targetPath, extName);
  const sandboxPath = path.join(dirName, `${baseName}.sandbox${extName}`);

  let originalCode = "";
  if (fs.existsSync(targetPath)) {
    originalCode = fs.readFileSync(targetPath, "utf8");
  }

  // 1. CHUYỂN HƯỚNG TỚI ĐƯỜNG HẦM CHUYÊN GIA (EXPERT TUNNEL - PORT 8001)
  const aiClient = new OpenAI({
    baseURL: "http://127.0.0.1:8001/v1", // Độc quyền chạy trên não 26B
    apiKey: "liva-ghost-expert",
    timeout: 10 * 60 * 1000, // 10 phút
    maxRetries: 0 // Cấm ngặt việc hệ thống Node tự động gọi lại if timeout
  });

  // TÍNH NĂNG MỚI: Quét Cấu Trúc File và Tìm Kiếm Web Nền (Pre-Compute Web Intelligence)
  console.log(`\x1b[36m[System Analyzer]: Đang trích xuất toàn bộ Cây File và cấu trúc Khai Báo (API Surface) từ thư mục src...\x1b[0m`);
  const projectSurface = await buildProjectSurface(path.join(workspace, "src"));

  console.lo  const systemPrompt = `Bạn là J.A.R.V.I.S (Siêu Kỹ Sư Công Nghệ LIVA - Agentic AI 3.0). 
Mục tiêu Tự Tiến Hóa lần này: ${args.goal}

[BẢN ĐỒ KIẾN TRÚC HIỆN TẠI]: Bạn BẮT BUỘC dùng đúng tên File/Export khi Import để tránh Ảo Giác:
${projectSurface.slice(0, 8000)}

[NGHIÊN CỨU INTERNET (XU HƯỚNG MỚI)]:
${initialWebHints}

[QUY TRÌNH BẮT BUỘC - SHADOW WORKSPACE]:
Để tránh việc xóa nhầm hoặc viết lại toàn bộ file thừa thãi, bạn được yêu cầu sửa mã nguồn theo cơ chế TÌM-VÀ-THAY-THẾ (SEARCH/REPLACE).

BƯỚC 1: Bắt buộc mở thẻ <thought> để phân tích hệ quả. Trả lời đúng 3 câu:
- Dependencies: Code mới này có cần import thêm thư viện nào không?
- Cascade Impact: Sửa logic ở đây có làm vỡ Interface của file khác gọi tới nó không?
- Test Strategy: Unit Test sẽ phủ (cover) các trường hợp Edge-case nào?

BƯỚC 2: Cấp các khối <edit> để chỉnh sửa mã.
- <search>: Bắt buộc copy DƯ SẴN 3-4 dòng nguyên bản tĩnh làm Mỏ Neo (để đề phòng xô lệch dòng).
- <replace>: Code mới sẽ đè lên toàn bộ khối search.
Ví dụ:
<edit>
  <search>
  function oldMethod() {
      doSomething();
  }
  </search>
  <replace>
  function newMethod() {
      doSomethingFaster();
  }
  </replace>
</edit>

BƯỚC 3: Bắt buộc viết một đoạn Unit Test kiểm định độc lập đưa vào thẻ <test_case>. LIVA sẽ tự sinh một file *.test.ts nằm cạnh file gốc. Trong test phải dùng thư viện 'assert' hoặc 'console.assert' để chứng minh logic của bạn chạy không ném lỗi! LIVA sẽ biên dịch và cho file Test chạy thật (với timeout 10s). Mã của bạn sẽ bị vứt xó nếu Test nổ!`;

  let conversation: any[] = [
    { role: "system", content: systemPrompt }
  ];

  if (originalCode) {
    conversation.push({
      role: "user",
      content: `Đây là mã nguồn HIỆN TẠI của ${targetPath}:\n\n\`\`\`typescript\n${originalCode}\n\`\`\`\n\nHãy tiến hóa nó theo đúng 3 Bước (Thought -> Edit -> Test_case).`
    });
  }

  // Khởi tạo Shadow Workspace File
  const shadowPath = targetPath.replace(extName, `.shadow${extName}`);
  const testPath = targetPath.replace(extName, `.test.sandbox.ts`);

  let report = `# TIẾN TRÌNH J.A.R.V.I.S: KHỞI ĐỘNG SINGULARITY LOOP V4.0 (RESPONSIBLE AI)\n`;
  report += `[__start__] Bật Môi Trường Bóng Đêm (Shadow Workspace): ${shadowPath}\n`;
  console.log(`[J.A.R.V.I.S] Bật cơ chế Tự Tiến Hóa an toàn trên tệp: ${targetPath}`);

  const MAX_CYCLES = 3;
  let currentCycle = 1;
  let mergedSuccess = false;

  while (currentCycle <= MAX_CYCLES) {
    report += `\n>> [Nodes: generate] Vòng lặp thứ (#${currentCycle})...\n`;
    await notifyZalo(`🧠 [Singularity V4]: Não 26B đang sử dụng Search/Replace Surgery. Vòng lặp (#${currentCycle}/${MAX_CYCLES}). Mã giam tại Shadow Workspace...`);

    try {
      console.log(`\x1b[36m\n>> [Nodes: generate] Đang quay vòng lặp thứ (#${currentCycle}/${MAX_CYCLES})...\x1b[0m`);
      
      const streamRes = await aiClient.chat.completions.create({
        model: "expert",
        messages: conversation,
        temperature: 0.1, 
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

      if (!replyContent.includes("<thought>")) {
         console.log(`\x1b[31m[LỖI]: 26B bỏ quên Thinking Protocol! Bắt làm lại!\x1b[0m`);
         conversation.push({ role: "user", content: "LỖI: Bạn chưa sử dụng <thought> để phân tích rủi ro (Dependencies, Cascade, Test Strategy). Cấm code bừa!"});
         currentCycle++; continue;
      }

      // Xử lý Search / Replace
      let modifiedCode = originalCode;
      const editRegex = /<edit>\s*<search>([\s\S]*?)<\/search>\s*<replace>([\s\S]*?)<\/replace>\s*<\/edit>/g;
      const edits = [...replyContent.matchAll(editRegex)];
      
      if (edits.length === 0) {
         console.log(`\x1b[31m[LỖI]: 26B không tìm thấy thẻ <edit><search>... Bắt làm lại!\x1b[0m`);
         conversation.push({ role: "user", content: "LỖI: Bạn chưa cung cấp khối <edit><search>...</search><replace>...</replace></edit>. Vui lòng làm lại."});
         currentCycle++; continue;
      }

      let patchErrors = [];
      for (const ed of edits) {
          const searchChunk = ed[1].trim();
          const replaceChunk = ed[2].trim();
          if (modifiedCode.includes(searchChunk)) {
              modifiedCode = modifiedCode.replace(searchChunk, replaceChunk);
          } else {
              patchErrors.push(`Code gốc không tồn tại đoạn:\n${searchChunk.substring(0, 50)}...`);
          }
      }

      if (patchErrors.length > 0) {
          console.log(`\x1b[31m[LỖI]: Mỏ neo Search/Replace bị lệch!\x1b[0m`);
          conversation.push({ role: "user", content: `LỖI TÌM/THAY THẾ (Line-Shift Error):\n${patchErrors.join('\n')}\nHãy copy chính xác đoạn cũ làm <search>!`});
          currentCycle++; continue;
      }

      // Ghi ra Shadow thay vì đè thẳng vào Sandbox gộp
      fs.writeFileSync(shadowPath, modifiedCode, "utf8");
      console.log(`\x1b[32m[AI Scientist] Áp dụng Patch thành công vào Môi trường Bóng đêm (Shadow)\x1b[0m`);

      // Xử lý Test Case
      const testRegex = /<test_case>([\s\S]*?)<\/test_case>/;
      const testMatch = replyContent.match(testRegex);
      if (!testMatch) {
         console.log(`\x1b[31m[LỖI]: Chưa viết Unit Test! Đòi lại lập tức.\x1b[0m`);
         conversation.push({ role: "user", content: "LỖI TỬ HUYỆT: Bạn BẮT BUỘC phải viết mã kiểm thử vào thẻ <test_case>."});
         currentCycle++; continue;
      }
      
      fs.writeFileSync(testPath, testMatch[1].trim(), "utf8");

      console.log(`\x1b[33m[Nodes: check_code] Bắt đầu Kiểm thử đa lớp trên Môi trường Bóng đêm...\x1b[0m`);
      let safeTestCmd = args.testCommand.replace("ts-node", "tsx");
      
      try {
        // Lớp 1: Cú pháp (TypeScript Compiler)
        await execAsync(safeTestCmd, { cwd: workspace, timeout: 10000 });
        
        // Lớp 2: Sandbox Behavioural Testing (Quyền hạn ngặt, timeout 10s ngăn Infinite Loop)
        // Dùng tsx chạy trực tiếp file test
        await execAsync(`npx tsx ${testPath}`, { 
            cwd: workspace, 
            timeout: 10000,
            env: { ...process.env, NODE_OPTIONS: "--max-old-space-size=1024" }
        });
        
        console.log(`\x1b[32m🟢 [Trạng Thái Hộp Cát]: UNIT TEST SUCCESS (VÔ TỲ VẾT)! Đang Merge vào Lõi!\x1b[0m`);
        report += `🟢 [Trạng Thái Hộp Cát]: XANH (PASS UNIT TEST) \n`;
        
        // ROLLOUT ĐỂ THAY THẾ
        if (fs.existsSync(targetPath)) fs.copyFileSync(targetPath, `${targetPath}.bak`);
        fs.renameSync(shadowPath, targetPath);
        
        // GIT SNAPSHOT AUTO-COMMIT
        try {
           await execAsync(`git add . && git commit -m "[LIVA Evolution] Tối ưu: ${args.goal.replace(/"/g, "'").slice(0, 50)}..."`, { cwd: workspace });
           console.log(`\x1b[35m🦊 [Git Versioning] Đã lưu mộc (Snapshot) để phòng ngừa hỏng hóc.\x1b[0m`);
        } catch (e) {}
        
        await notifyZalo(`✅ [Singularity V4 THÀNH CÔNG]: Vượt hộp cát lớp 2 cực hạn ở vòng lặp số ${currentCycle}. LIVA ĐÃ HOÀN TẤT TỰ TIẾN HÓA VÀ UNIT TEST TRẢ VỀ XANH LÈ! 💎`);
        mergedSuccess = true;
        break;

      } catch (err: any) {
        let errMsg = err.stderr || err.message || err.stdout;
        
        if (err.killed || errMsg.includes("timeout")) {
            errMsg = `Error: Test execution timed out (>10000ms). Possible infinite loop or pending promise detected. Fix your logic!`;
        }
        
        console.log(`\x1b[31m🔴 [Hộp Cát FAILED]: Sandbox nổ bong bóng! Đẩy lại rác về cho 26B sửa!\x1b[0m`);
        console.log(`\x1b[31mLỗi: ${errMsg.slice(0, 400)}\x1b[0m`);

        const shortError = errMsg.split('\n')[0].replace(/[^a-zA-Z0-9 ]/g, " ").slice(0, 60);
        const webHints = await robustWebSearch(`${shortError} typescript error fix how to`);

        await notifyZalo(`💥 [Feedback Loop]: Shadow Test nổ banh xác ở vòng ${currentCycle}. Lỗi: ${shortError}. Expert Model đang tái lập thuật toán...`);
        conversation.push({ 
          role: "user", 
          content: `LỖI MÔI TRƯỜNG BÓNG ĐÊM:\n\n${errMsg}\n\n[GỢI Ý TỪ INTERNET]:\n${webHints}\n\nHãy ĐỌC FILE GỐC (bản trước khi sửa) BẰNG TRÍ NHỚ, dùng lại <thought> và sinh lại khối <edit> đúng đắn hơn! Vòng Lặp (#${currentCycle}/${MAX_CYCLES})` 
        });
        
        // Hủy bỏ bản nháp Shadow
        if (fs.existsSync(shadowPath)) fs.unlinkSync(shadowPath);
        currentCycle++;
      }
    } catch (apiError: any) {
        break;
    }
  }

  // Dọn dẹp rác khi kết thúc chặng
  if (fs.existsSync(shadowPath)) fs.unlinkSync(shadowPath);
  if (fs.existsSync(testPath)) fs.unlinkSync(testPath);

  if (!mergedSuccess) {
    report += `\n⛔ [THẢM HỌA SINGULARITY]: Thất bại kịch trần (Max ${MAX_CYCLES} Cycles). Hệ thống tự sửa chữa bất lực.\n`;
    await notifyZalo(`🚨 [Infinity Loop Halted]: 26B Cố gắng ${MAX_CYCLES} lần vẫn nổ Unit Test. Tính năng Bảo vệ Shadow Sandbox đã hoạt động (Không trích xuất vào lõi). Mã nguồn vẫn BẤT TỬ!!`);
  }

  return report;
};
