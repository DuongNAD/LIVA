import { exec } from "child_process";
import { promisify } from "util";
import fs from "fs";
import path from "path";
import OpenAI from "openai";
import axios from "axios";

const execAsync = promisify(exec);

// Helper báo Zalo bí mật để đẩy thông báo Mid-Flight (Tiến độ) xuống điện thoại Sếp
async function notifyZalo(msg: string) {
  const token = process.env.ZALO_OA_ACCESS_TOKEN;
  let userId = process.env.ZALO_USER_ID;
  if (!token || !userId) return;

  try {
     const isBotToken = token.includes(":");
     const endpoint = isBotToken 
         ? `https://bot-api.zaloplatforms.com/bot${token}/sendMessage`
         : "https://openapi.zalo.me/v3.0/oa/message/cs";
     
     if (isBotToken) {
         await axios.post(endpoint, { chat_id: userId, text: msg }).catch(() => {});
     } else {
         await axios.post(endpoint, {
            recipient: { user_id: userId },
            message: { text: msg }
         }, { headers: { access_token: token } }).catch(() => {});
     }
  } catch(e) {}
}

export const metadata = {
  name: "liva_ai_scientist",
  description:
    "Kỹ năng Trợ lý Dev Tự Động (Auto-Coder). Điểm mạnh là đọc trực tiếp File dự án (TS, JS, Vue, Py), sửa code theo ý tưởng, chạy lệnh Test để bắt lỗi (stderr đỏ) và TỰ ĐỘNG lặp lạị việc Sửa-Chạy cho tới khi Test xanh (success). Sẽ tự động Backup file (.bak) và Rollback nếu sửa hỏng.",
  parameters: {
    type: "object",
    properties: {
      goal: {
        type: "string",
        description: "Mục tiêu cần tính toán, thiết kế lại thuật toán hoặc tính năng mới cần code thêm vào file.",
      },
      targetFilePath: {
        type: "string",
        description: "Đường dẫn (tuyệt đối hoặc tương đối) đến file dự án THẬT cần sửa. VD: src/core/AgentLoop.ts hoặc E:/Project/LIVA/liva-ui/src/App.vue.",
      },
      testCommand: {
        type: "string",
        description: "Lệnh sẽ chạy để kểm tra file vừa sửa. VD: 'npx tsc' (để kiểm tra lỗi syntax TS), 'npm run build' (với Vue), hoặc 'python xxx' để chạy unit test.",
      },
      workingDirectory: {
         type: "string",
         description: "Đường dẫn gốc để chạy lệnh testCommand. Nên để là đường dẫn gốc của project đó. VD: E:/Project/LIVA/openclaw-gateway."
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
    return `[Hệ thống]: Không tìm thấy thư mục làm việc ${workspace}. Hãy tự tạo hoặc điền cho đúng.`;
  }

  // Xác định Target Path
  let targetPath = args.targetFilePath;
  if (!path.isAbsolute(targetPath)) {
    targetPath = path.join(workspace, targetPath);
  }

  const MAX_ITERS = 5;
  let currentIter = 1;

  let report = `# BÁO CÁO NHÀ KHOA HỌC SAKANA (LIVA AUTO-CODER)\n`;
  report += `- Mục tiêu Fix/Build: ${args.goal}\n`;
  report += `- Đang can thiệp File: ${targetPath}\n\n`;

  // BACKUP LOGIC KHI FILE ĐÃ TỒN TẠI
  let originalCode = "";
  let backupPath = `${targetPath}.bak`;
  let hasBackup = false;

  if (fs.existsSync(targetPath)) {
    originalCode = fs.readFileSync(targetPath, "utf8");
    fs.copyFileSync(targetPath, backupPath);
    hasBackup = true;
    report += `🛡️ Đã tìm thấy file dự án gốc. Hệ thống Đã sao lưu (Backup) an toàn tại: ${backupPath}\n`;
  } else {
    report += `🛡️ File chưa tồn tại, hệ thống sẽ tạo mới từ đầu.\n`;
  }

  // Khởi tạo API đến Expert 26B
  const aiClient = new OpenAI({
    baseURL: "http://127.0.0.1:8000/v1",
    apiKey: "local-ghost-layer",
  });

  const systemPrompt = `Bạn là Siêu Kỹ Sư Công Nghệ LIVA (AI Auto-Coder). Khả năng của bạn là nhào nặn Kiến trúc hệ thống và Tự Chữa Lành Lỗi (Self-Healing Code).
Nhiệm vụ lần này: ${args.goal}

Quy trình Bắt buộc:
1. Đọc hiểu mã nguồn nguyên thủy (Nếu có). 
2. Viết lại MÃ NGUỒN HOÀN CHỈNH ĐÃ KÈM CẢ SỬA ĐỔI nằm gọn trong cặp thẻ XML này:
<source_code>
// Toàn bộ code hoàn chỉnh ở đây...
</source_code>
KHÔNG ĐƯỢC CHỈ VIẾT NHỮNG DÒNG THAY ĐỔI, PHẢI XUẤT RA TOÀN BỘ FILE. Code ở trong thẻ <source_code> sẽ được sao chép đè thẳng vào file project bằng phần mềm ngoài.
3. Nếu ở lượt sau bạn nhận được báo lỗi Đỏ (Terminal Stderr), hãy bớt nói dài dòng, tập trung nhẩm tính nguyên nhân lỗi logic và đẻ lại code bọc trong <source_code>.`;

  let conversation: any[] = [
    { role: "system", content: systemPrompt }
  ];

  if (originalCode) {
    conversation.push({
      role: "user",
      content: `Đây là mã nguồn HIỆN TẠI của file gốc (chưa sửa):\n\n\`\`\`\n${originalCode}\n\`\`\`\n\nHãy sửa nó theo yêu cầu và xuất ra bản Hoàn Thiện bọc trong thẻ <source_code>.`
    });
  } else {
    conversation.push({ role: "user", content: `Hãy viết luồng code hoàn chỉnh từ con số 0 cho mục tiêu trên bọc trong thẻ <source_code>.` });
  }

  let finalSuccess = false;

  while (currentIter <= MAX_ITERS) {
    report += `### Vòng chạy #${currentIter}:\n`;
    console.log(`[AI Scientist Coder] Gọi LLM nhào nặn mã nguồn (Vòng ${currentIter}/${MAX_ITERS})....`);
    await notifyZalo(`⏳ [Tiến trình Auto-Coder]: Đang tập trung đẻ Code (Vòng ${currentIter}/${MAX_ITERS}). Mã nguồn của sếp khá phức tạp, xin đợi xíu khoảng 1-2 phút...`);
    
    try {
      // Gọi Expert LLM
      const res = await aiClient.chat.completions.create({
        model: "expert",
        messages: conversation,
        temperature: 0.1, // Nên để thấp cho code logic
      });

      const replyContent = res.choices[0]?.message?.content || "";
      conversation.push({ role: "assistant", content: replyContent });

      // Trích xuất Source Code
      const codeMatch = replyContent.match(/<source_code>([\s\S]*?)<\/source_code>/);
      if (!codeMatch || !codeMatch[1]) {
        report += `⚠️ Vòng ${currentIter}: Bạn xuất sai cú pháp, quên bọc <source_code>. Đã báo ép làm lại.\n`;
        conversation.push({ role: "user", content: "LỖI PARSER: Tôi không tìm thấy thẻ <source_code>. Vui lòng bọc TOÀN BỘ file code vào thẻ này!"});
        currentIter++;
        continue;
      }

      const exactCode = codeMatch[1].trim();
      fs.writeFileSync(targetPath, exactCode, "utf8");
      
      report += `✅ Đã lưu đè mã nguồn lên File dự án. Bắt đầu kích hoạt Lệnh Test: ${args.testCommand}\n`;
      console.log(`[Auto-coder] Executing Test: ${args.testCommand} at ${workspace}`);
      await notifyZalo(`✅ [Tiến trình Auto-Coder]: Viết code xong ở Vòng ${currentIter}! Đã đè nguyên khối Code chui vào Dự án. Đang tự động chạy lệnh kiểm thử: "${args.testCommand}" ...`);

      // Thực thi Test (Compile, Build, Run...)
      try {
        const timeoutMs = 90000; // Cho 90 giây chạy Compile Project
        // execAsync sẽ throw Error nếu có return code khác 0
        const { stdout, stderr } = await execAsync(args.testCommand, { cwd: workspace, timeout: timeoutMs });
        
        // Cần lưu ý một số lệnh ghi warning qua cổng stderr nhưng return = 0
        if (stderr && stderr.toLowerCase().includes("error:")) {
           // Có chữ error trong stderr, coi như xịt
           throw new Error(stderr); 
        }

        // Nếu mọi thứ ổn xuôi, nghĩa là Compile / Test thành công
        report += `🎯 Kiểm thử (Testing Command): PASS (THÀNH CÔNG) NGAY!\n`;
        report += `> Output: ${stdout.trim().slice(0, 300)}\n\n`;
        report += `Quá trình refactor Dự án đã khép kín hoàn tất!`;
        console.log(`[AI Scientist Coder] Vòng lặp Khai sáng Code Thành Công ở Vòng ${currentIter}!`);
        finalSuccess = true;
        break; // Thoát vòng

      } catch (err: any) {
        // Crash Test (Syntax error, Build fail)
        report += `💥 Kiểm thử (Testing Command): FAILED (LỖI THỰC THI).\n`;
        const errMsg = err.stderr || err.message || err.stdout;
        report += `> Lỗi Dịch/Biên Dịch: ${errMsg.slice(0, 500)}\n`;
        
        console.warn(`[Auto-Coder] Code hỏng ở vòng ${currentIter}! Ép AI Review lại lỗi...`);
        await notifyZalo(`💥 [Cảnh báo Auto-Coder]: Lỗi nặng!! Lệnh test Terminal vừa phun ra 1 đống chữ Đỏ (Syntax error). Tiến trình Vòng ${currentIter} đã thất bại. Em đang tự ngậm ngùi Bóc Lỗi để đẻ lại code từ đầu!`);
        conversation.push({ 
          role: "user", 
          content: `CRASH/ERROR KHI TEST MÃ NGUỒN CỦA BẠN BẰNG LỆNH (${args.testCommand}):\nLog:\n${errMsg}\n\nHãy cẩn thận tự review lại xem biến nào nhầm, logic nào gãy gập, sửa mã và bọc TẤT CẢ lại trong <source_code> lần nữa!` 
        });
        currentIter++;
      }

    } catch (apiError: any) {
      report += `[Lỗi Lõi]: Cổng kết nối Internal API (Local) bị đứt: ${apiError.message}\n`;
      break;
    }
  }

  // LOGIC ROLLBACK BẢO MẬT NẾU ĐÁNH MẤT FILE GỐC MÀ VẪN CODE NGU
  if (!finalSuccess && currentIter > MAX_ITERS) {
    report += `\n⛔ BÁO ĐỘNG ĐỎ: Đã chạm Tối Đa Cố Gắng (Max Iters = ${MAX_ITERS}) nhưng mã nguồn mới vẫn sinh Lỗi (Crash). Bỏ cuộc.\n`;
    report += `↩️ ĐANG KÍCH HOẠT HỆ THỐNG ROLLBACK (Undo) TỰ ĐỘNG...\n`;
    console.error("[Auto-Coder] FAILED MAX ITERS. Initiating Rollback sequence!!");
    // Báo Zalo cực độ:
    await notifyZalo(`🚨 [CỨU Y VIỆN CẤP]: Mất não! Trải qua ${MAX_ITERS} vòng lặp mà tụi em vẫn không viết ra code khỏi bị văng đỏ! Đã tự động kích hoạt tính năng ROLLBACK (Lấy file Cũ Backup gỡ ngược đè lên Project) để cứu Hệ thống! Mọi thứ đã an toàn.`);
    
    if (hasBackup && fs.existsSync(backupPath)) {
      fs.copyFileSync(backupPath, targetPath);
      report += `✅ ROLLBACK THÀNH CÔNG: Em đã trả file ${targetPath} về nguyên vẹn bản gốc ban đầu của nó. Project của anh được cam kết an toàn 100% không bị bẻ gãy!\n`;
    } else {
      report += `⚠️ ROLLBACK: Không có file gốc để mà backup do anh tạo file mới từ đầu. Đang giữ nguyên file lỗi ở đó cho anh tự xem.\n`;
    }
  } else if (finalSuccess) {
    if (hasBackup) {
       report += `\n🌟 Ghi chú: Em vẫn còn Cất Bản Lỗi Cũ Của Anh Tại: ${backupPath}. Nếu anh test thấy code mới nó ngon lành rồi thì anh có thể xóa đi nhé!`;
    }
  }

  return report;
};
