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

  console.log(`\x1b[36m[Web Intelligence]: Đang tự động Search Mạng khảo sát kiến thức nền cho Mục tiêu Tối Ưu...\x1b[0m`);
  const initialWebHints = await robustWebSearch(`${args.goal.slice(0, 100)} typescript best practices tutorial`);
  console.log(`\x1b[36m[Web Intelligence]: Đã cập nhật xong Kiến thức Chuyên môn từ Internet.\x1b[0m`);

  const systemPrompt = `Bạn là J.A.R.V.I.S (Siêu Kỹ Sư Công Nghệ LIVA - Agentic AI 3.0). 
Mục tiêu Tự Tiến Hóa (Singularity Loop) lần này: ${args.goal}

[BẢN ĐỒ KIẾN TRÚC HIỆN TẠI]: Bạn BẮT BUỘC phải dùng đúng tên File và Export này khi Import để tránh lỗi Ảo Giác (Hallucination):
${projectSurface.slice(0, 8000)}

[NGHIÊN CỨU TỪ INTERNET]: Các bài viết/giải pháp từ mạng về chủ đề của bạn để tối ưu code:
${initialWebHints}

Quy trình Bắt buộc (Môi trường cách ly Sandbox):
1. Phân tích nguyên lý hoạt động của mã gốc, so chiếu với Bản Đồ Kiến Trúc và Kiến Thức Mạng.
2. Viết lại TOÀN BỘ MÃ NGUỒN TIẾN HÓA nằm BẮT BUỘC trong thẻ XML này:
<source_code file="đường_dẫn_tương_đối_hoặc_tuyệt_đối">
// Toàn bộ code hoàn chỉnh ở đây...
</source_code>
Lưu ý: Nếu có lỗi chéo (Cascading Error) ở file khác, bạn CÓ QUYỀN TRẢ VỀ NHIỀU THẺ <source_code file="..."> để sửa đồng thời nhiều file.
Lệnh Bắt Buộc: Cấm tự sáng tạo đường dẫn import. Mọi hàm import phải được trích từ Bản đồ API Surface phía trên. 
3. Nếu ở vòng lặp sau bạn nhận được thông báo Lỗi Biên Dịch (Feedback Edge Error), hãy sửa sai ngay lập tức. Đừng cãi lại Trình biên dịch.`;

  let conversation: any[] = [
    { role: "system", content: systemPrompt }
  ];

  if (originalCode) {
    conversation.push({
      role: "user",
      content: `Đây là mã nguồn HIỆN TẠI (Trước tiến hóa) của ${targetPath}:\n\n\`\`\`typescript\n${originalCode}\n\`\`\`\n\nHãy tiến hóa nó và xuất bản Hoàn Thiện bọc trong thẻ <source_code file="${targetPath}">.`
    });
  }

  let report = `# TIẾN TRÌNH J.A.R.V.I.S: KHỞI ĐỘNG SINGULARITY LOOP V3.0\n`;
  report += `[__start__] Định tuyến thành công Model Expert 26B qua cổng 8001.\n`;
  report += `[__start__] Bật Môi Trường Hộp Cát Bảo Mật: ${sandboxPath}\n`;
  console.log(`[J.A.R.V.I.S] Bật cơ chế Tự Tiến Hóa trên tệp: ${targetPath}`);

  const MAX_CYCLES = 10;
  let currentCycle = 1;
  let mergedSuccess = false;

  // LANG-GRAPH STATE MACHINE (Simulated Nodes)
  while (currentCycle <= MAX_CYCLES) {
    report += `\n>> [Nodes: generate] Đang quay vòng lặp thứ (#${currentCycle})...\n`;
    await notifyZalo(`🧠 [Singularity Loop]: Đang sử dụng 26B Expert để kiến trúc mã nguồn. Trạng thái vòng lặp (#${currentCycle}/${MAX_CYCLES}). Mã đang được giam trong hộp cát bảo vệ...`);

    try {
      console.log(`\x1b[36m\n>> [Nodes: generate] Đang quay vòng lặp thứ (#${currentCycle})...\x1b[0m`);
      console.log(`\x1b[33m[AI Scientist] NÃO 26B ĐANG BÓC TÁCH KIẾN TRÚC VÀ VIẾT CODE TRỰC TIẾP...\x1b[0m`);

      const streamRes = await aiClient.chat.completions.create({
        model: "expert",
        messages: conversation,
        temperature: 0.2, 
        max_tokens: 16384,
        stream: true
      }, { timeout: 600000 }); // Nâng timeout

      let replyContent = "";
      process.stdout.write("\x1b[90m"); // Xám cho dễ nhìn Matrix
      for await (const chunk of streamRes) {
         const token = chunk.choices[0]?.delta?.content || "";
         replyContent += token;
         process.stdout.write(token); // Văng token realtime
      }
      process.stdout.write("\x1b[0m\n"); // Reset màu

      conversation.push({ role: "assistant", content: replyContent });

      if (replyContent.trim() === "") {
        console.log(`\x1b[31m[LỖI CHÍ TỬ]: Não 26B ngưng thở và trả về rỗng (Có thể do kích thước File quá lớn vượt Context Length). Mạch tiến hóa Tự động hủy để bảo toàn VRAM!\x1b[0m`);
        report += `[Thảm họa Lượng tử]: Context Length Tràn bộ nhớ do Tệp mục tiêu quá nặng. Não bị ngắt. Tiến trình bỏ cụt.\n`;
        break; // Thoát ngay lập tức khối Vòng Lặp, kết thúc tác vụ ảo giác này
      }

      console.log(`\x1b[32m[AI Scientist] Không gặp vấn đề ngắt nối. Não 26B đã hoàn thành việc nhả Code!\x1b[0m`);

      // Trích xuất mã hộp cát ĐA TỆP
      const codeRegex = /<source_code\s+file="([^"]+)">([\s\S]*?)<\/source_code>/g;
      const matches = [...replyContent.matchAll(codeRegex)];
      
      if (matches.length === 0) {
         console.log(`\x1b[31m[LỖI]: Não 26B ảo giác bỏ quên thẻ <source_code file="...">. Bắt buộc cày lại vòng lặp!\x1b[0m`);
         report += `[Feedback Edge]: Ảo giác mã nguồn, thiếu thẻ <source_code file="...">. Cưỡng bức làm lại.\n`;
         conversation.push({ role: "user", content: "LỖI PARSER: Bạn Không sinh ra <source_code file=\"...\">. Hệ thống khước từ Output nãy giờ. Cẩn thận bọc code lại đi!"});
         currentCycle++;
         continue;
      }

      console.log(`\x1b[32m[AI Scientist] Trích xuất thành công ${matches.length} tệp mã nguồn. Đang ghi đè vào Sandbox...\x1b[0m`);
      
      const generatedSandboxes: { original: string, sandbox: string }[] = [];
      
      for (const match of matches) {
         let target = match[1].trim();
         if (!path.isAbsolute(target)) {
            target = path.join(workspace, target);
         }
         const dir = path.dirname(target);
         const ext = path.extname(target);
         const base = path.basename(target, ext);
         const sboxPath = path.join(dir, `${base}.sandbox${ext}`);
         
         const sboxCode = match[2].trim();
         fs.writeFileSync(sboxPath, sboxCode, "utf8");
         generatedSandboxes.push({ original: target, sandbox: sboxPath });
      }
      
      console.log(`\x1b[33m[Nodes: check_code] Bắt đầu Kiểm tra Biên dịch thực tế bằng lệnh:\x1b[0m ${args.testCommand}`);
      report += `[Nodes: check_code] Đã đúc tệp Sandbox. Tiến hành đo kiểm biên dịch gắt gao bằng lệnh: ${args.testCommand}\n`;
      
      // Khắc phục lỗi ESM: Đổi ts-node thành tsx hoặc tsc để tránh lỗi Unknown file extension
      let safeTestCmd = args.testCommand.replace("ts-node", "tsx");
      
      // Thực thi kiểm định hộp cát
      try {
        const testRes = await execAsync(safeTestCmd, { cwd: workspace, timeout: 60000 });
        
        // --- GREEN CONSOLE ACHIEVED --- 
        console.log(`\x1b[32m🟢 [Trạng Thái Hộp Cát]: GREEN CONSOLE (THÀNH CÔNG VÔ TỲ VẾT)! Đang Merge vào Lõi!\x1b[0m`);
        
        if (testRes.stdout && testRes.stdout.trim().length > 0) {
            console.log(`\x1b[90m--- [SANDBOX STDOUT LOGS] ---\n${testRes.stdout.trim()}\n-----------------------------\x1b[0m`);
        }

        report += `🟢 [Trạng Thái Hộp Cát]: GREEN CONSOLE (THÀNH CÔNG VÔ TỲ VẾT) \n`;
        report += `[Nodes: __end__] Hoàn hảo! Mã Tiến Hóa hoàn toàn đúng cú pháp logic. Bắt đầu Hợp Nhất (Merge) vào Project lõi!\n`;
        
        // ROLLOUT / MERGE ĐA TỆP
        for (const meta of generatedSandboxes) {
          if (fs.existsSync(meta.original)) {
              fs.copyFileSync(meta.original, `${meta.original}.bak`); 
          }
          fs.renameSync(meta.sandbox, meta.original);
        }
        
        // GIT SNAPSHOT AUTO-COMMIT
        try {
           await execAsync(`git add . && git commit -m "[LIVA Evolution] Tối ưu: ${args.goal.replace(/"/g, "'").slice(0, 50)}..."`, { cwd: workspace });
           console.log(`\x1b[35m🦊 [Git Versioning] Đã lưu mộc (Snapshot) để phòng ngừa hỏng hóc Runtime.\x1b[0m`);
        } catch (e) {
           console.log(`\x1b[90m[Git Info] Chưa cấu hình Git Repository hoặc không có thay đổi.\x1b[0m`);
        }
        
        await notifyZalo(`✅ [Singularity Loop THÀNH CÔNG]: Vượt ngục Hộp cát ở vòng lặp số ${currentCycle}. Trình biên dịch báo Green Console (Không một vệt lỗi). LIVA ĐÃ HOÀN TẤT TỰ TIẾN HÓA VÀ HỢP NHẤT MÃ NGUỒN VÀO HỆ THỐNG AN TOÀN TUYỆT ĐỐI! 💎`);
        mergedSuccess = true;
        break;

      } catch (err: any) {
        // --- RED CONSOLE FAIL ---
        const errMsg = err.stderr || err.message || err.stdout;
        console.log(`\x1b[31m🔴 [Hộp Cát FAILED]: Trình biên dịch bợp tai Code lỗi! Phản hồi lại cho 26B sửa gấp!\x1b[0m`);
        
        if (err.stdout && err.stdout.trim().length > 0) {
             console.log(`\x1b[90m--- [SANDBOX PARTIAL LOGS] ---\n${err.stdout.trim()}\n------------------------------\x1b[0m`);
        }
        
        console.log(`\x1b[31m+ Tóm tắt Lỗi: ${errMsg.slice(0, 500)}...\x1b[0m`);

        console.log(`🔴 [Trạng Thái Hộp Cát]: RED CONSOLE FAILED (LỖI BIÊN DỊCH)\nTrình Dịch báo cáo sai phạm: ${errMsg.slice(0, 400)}`);
        
        // TÍNH NĂNG MỚI: Web Research khi vấp lỗi (Robust duckduckgo lite)
        const shortError = errMsg.split('\n')[0].replace(/[^a-zA-Z0-9 ]/g, " ").slice(0, 60);
        console.log(`\x1b[36m[Web Intelligence]: Đang sục sạo mạng tìm phao cứu sinh cho rập: "${shortError}"...\x1b[0m`);
        const webHints = await robustWebSearch(`${shortError} typescript error fix how to`);
        console.log(`\x1b[36m[Web Intelligence]: Đã thu thập Gợi ý từ Internet!\x1b[0m`);

        // CASCADE ERROR RESOLVER: Quét tìm các file gây lỗi ngoài luồng
        const fileErrRegex = /([a-zA-Z0-9_\-\/\\]+\.ts)\(\d+,\d+\): error/g;
        const errMatches = [...errMsg.matchAll(fileErrRegex)];
        const additionalFilesContext: string[] = [];
        const seenFiles = new Set<string>();

        for (const em of errMatches) {
            let errFile = em[1];
            if (!path.isAbsolute(errFile)) {
                errFile = path.join(workspace, errFile);
            }
            if (!seenFiles.has(errFile) && fs.existsSync(errFile)) {
                seenFiles.add(errFile);
                const fileContent = fs.readFileSync(errFile, "utf-8");
                additionalFilesContext.push(`\n**NỘI DUNG TỆP BỊ LỖI DÂY CHUYỀN (${em[1]}):**\n\`\`\`typescript\n${fileContent}\n\`\`\`\n`);
            }
        }
        
        const cascadeContextStr = additionalFilesContext.length > 0 
           ? "\n\n[HỆ THỐNG ĐÃ TRÍCH XUẤT CÁC TỆP BỊ LỖI CHÉO CHO BẠN SỬA:]\n" + additionalFilesContext.join("") 
           : "";

        await notifyZalo(`💥 [Feedback Loop]: Trình biên dịch phát hiện sai phạm cú pháp (Báo Đỏ) ở Code Nháp Sandbox vòng ${currentCycle}. Expert Model đang tự phân tích lỗi để cày lại thuật toán...`);
        conversation.push({ 
          role: "user", 
          content: `LỖI TRÌNH BIÊN DỊCH BẮT ĐƯỢC TẠI SANBOX KHI CHẠY ${args.testCommand}:\n\n${errMsg}${cascadeContextStr}\n\n[GỢI Ý CÁCH SỬA LỖI TỪ INTERNET]:\n${webHints}\n\nDùng chuỗi tư duy (Chain of Thought), nhìn lại mã nãy, TỰ SỬA VÀ ĐÁP LẠI VÀO các thẻ <source_code file="...">. KHÔNG ĐƯỢC ĐẦU HÀNG!` 
        });
        currentCycle++;
      }
    } catch (apiError: any) {
        console.log(`[Lỗi Lõi Động Cơ 8001]: Model 26B ngủ quên hoặc máy chủ API đứt kết nối: ${apiError.message}`);
        break;
    }
  }

  if (!mergedSuccess) {
    report += `\n⛔ [THẢM HỌA SINGULARITY]: Thất bại kịch trần (Max ${MAX_CYCLES} Cycles). Hệ thống tự sửa chửa bất lực.\n`;
    report += `[Bảo Vệ Core]: Hệ thống Lõi vẫn an toàn tuyệt đối. Xóa tệp Sandbox nháp rác rưởi.\n`;
    if (fs.existsSync(sandboxPath)) {
        fs.unlinkSync(sandboxPath);
    }
    await notifyZalo(`🚨 [Thất Bại Kịch Trần]: 26B Cố gắng ${MAX_CYCLES} lần vẫn viết code hỏng biên dịch. Tính năng Bảo vệ Hộp Cát đã hoạt động (Không Merge Code). Source Lõi BẤT TỬ không chịu tổn thất!! Sếp hãy yên tâm.`);
  }

  return report;
};
