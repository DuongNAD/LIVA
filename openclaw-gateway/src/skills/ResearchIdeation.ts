import axios from "axios";
import fs from "fs";
import path from "path";
import OpenAI from "openai";

// Helper báo Zalo
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

const sleep = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));

export const metadata = {
  name: "sakana_ideation",
  description:
    "Kỹ năng AI Scientist (Sakana). Hệ thống Tự Động Tư Duy: Tự đẻ ra 10 ý tưởng, tự Query Semantic Scholar xem có bị trùng lặp thế giới chưa, tự soi điểm Khả Thi và Đột Phá, tự chốt Ý Tưởng duy nhất để vẽ nên Kế hoạch Phát Triển Sản Phẩm AI vượt trội. Dùng khi người dùng yêu cầu 'Nghiên cứu ý tưởng', 'Đẻ idea đột phá'.",
  parameters: {
    type: "object",
    properties: {
      topic: {
        type: "string",
        description: "Chủ đề vĩ mô cần xin ý tưởng. Ví dụ: 'Kết hợp AI và Web', 'Tối ưu hoá Local LLM trên PC'.",
      },
      fileLocation: {
        type: "string",
        description: "Thư mục lưu Đề Xuất Nghiên cứu gốc. Khuyến nghị: E:/Project/LIVA/scratch_workspace"
      }
    },
    required: ["topic", "fileLocation"],
  },
};

export const execute = async (args: {
  topic: string;
  fileLocation: string;
}): Promise<string> => {
  
  const workspace = args.fileLocation;
  if (!fs.existsSync(workspace)) {
     fs.mkdirSync(workspace, { recursive: true });
  }

  await notifyZalo(`🔬 [LIVA AI SCIENTIST]: Kích hoạt đường hầm tư duy học thuật! Em sẽ động não 10 ý tưởng siêu phẩm cho chủ đề "${args.topic}" và tự đối soát xem có thằng Tây nào làm chưa.`);

  const aiClient = new OpenAI({
    baseURL: "http://127.0.0.1:8000/v1",
    apiKey: "local-ghost-layer",
  });

  // Dùng LLM tự nặn tên file ngắn gọn (Smart Filename Naming)
  let shortName = "research";
  try {
     const resName = await aiClient.chat.completions.create({
        model: "expert",
        messages: [{ role: "user", content: `Hành động như một bot đổi tên file. Rút gọn chủ đề sau thành 1 tên file tiếng Anh ngắn gọn, cực kỳ ý nghĩa (tối đa 4 từ, cách phối bằng gạch dưới _). VÍ DỤ: "cách luộc trứng" -> "egg_boiling_guide". Tương tự hãy làm với Chủ đề: "${args.topic}". CHỈ TRẢ VỀ TÊN FILE, KHÔNG GIẢI THÍCH.` }],
        temperature: 0.1,
        max_tokens: 20
     });
     const aiName = resName.choices[0]?.message?.content?.trim();
     if (aiName && !aiName.includes(" ")) {
         shortName = aiName.replace(/[^\w-]/g, "").toLowerCase();
     } else if (aiName) {
         shortName = aiName.replace(/[^\w-]/g, "_").replace(/_+/g, "_").toLowerCase();
     }
  } catch(e) {}
  
  const baseName = shortName.substring(0, 40);
  const targetPath = path.join(workspace, baseName + "_research.md");
  const rawIdPath = path.join(workspace, baseName + "_raw_ideas.json");
  const logPath = path.join(workspace, baseName + "_evaluation_log.md");
  
  fs.writeFileSync(targetPath, "", "utf8");
  fs.writeFileSync(logPath, `# NHẬT KÝ ĐÁNH GIÁ Ý TƯỞNG (AI SCIENTIST)\nChủ đề: ${args.topic}\n\n`, "utf8");

  // BƯỚC 1: BRANDSTORM 10 IDEAS
  console.log(`[AI Scientist] Phase 1: Ideation...`);
  const ideationPrompt = `Bạn là Huyền thoại Nghiên Cứu LIVA. Chủ đề: ${args.topic}.
HÃY ĐỀ XUẤT ĐÚNG 10 Ý TƯỞNG Cực Kỳ Đột Phá (Novelty) và khác biệt nhau.
BẮT BUỘC TRẢ VỀ CHUẨN JSON ARRAY:
[
  {
     "id": 1,
     "title": "Tên ý tưởng tiếng Việt",
     "keywords": "3 english search terms",
     "core_idea": "Sự đột phá cốt lõi nằm ở đâu (3 câu)"
  }
]
CHỈ XUẤT RA JSON. Tuyệt đối không sinh text ngoài lề.`;

  let ideas: any[] = [];
  try {
     const resP1 = await aiClient.chat.completions.create({
        model: "expert",
        messages: [{ role: "user", content: ideationPrompt }],
        temperature: 0.8,
        max_tokens: 3000
     });
     
     const rawReply = resP1.choices[0]?.message?.content || "";
     const matchJson = rawReply.match(/\[\s*\{[\s\S]*\}\s*\]/);
     if (matchJson) {
         ideas = JSON.parse(matchJson[0]);
     } else {
         throw new Error("Không bắt được mảng JSON của Idea.");
     }
  } catch(e: any) {
     console.error("Ideation Lỗi Parser:", e);
     await notifyZalo(`❌ [AI Scientist]: Óc em bị bí đoạn rặn 10 ý tưởng rồi sếp ạ! Parser gãy: ${e.message}`);
     return "Lỗi Ideation.";
  }

  // Đảm bảo không quá 10 (phòng hờ)
  ideas = ideas.slice(0, 10);

  // LOG OUT FILE RAW JSON
  fs.writeFileSync(rawIdPath, JSON.stringify(ideas, null, 2), "utf8");
  console.log(`[AI Scientist] Saved 10 ideas to ${rawIdPath}`);
  await notifyZalo(`💡 [Ideation]: Xong bước 1! Đã rặn được ${ideas.length} ý tưởng (từ Ý Tưởng số 1: "${ideas[0].title}"). Em đang cào Semantic Scholar để check ĐẠO VĂN & tính ĐỘT PHÁ cho TỪNG ý tưởng...`);

  // BƯỚC 2 & 3: LITERATURE SEARCH & EVALUATION
  console.log(`[AI Scientist] Phase 2 & 3: Search and Score...`);
  
  const semanticKey = process.env.SEMANTIC_SCHOLAR_API_KEY || "";
  const headers = semanticKey ? { "x-api-key": semanticKey } : {};

  let scoredIdeas = [];

  for (let i = 0; i < ideas.length; i++) {
      const idea = ideas[i];
      let contextPapers = "";

      // 2. Search
      try {
          const scholarUrl = `https://api.semanticscholar.org/graph/v1/paper/search?query=${encodeURIComponent(idea.keywords)}&limit=5&fields=title,abstract,year`;
          const response = await axios.get(scholarUrl, { headers });
          if(response.data?.data && response.data.data.length > 0) {
             const limitData = response.data.data.map((p:any) => `[${p.year}] ${p.title}: ${p.abstract || ""}`);
             contextPapers = limitData.join("\n---");
          } else {
             contextPapers = "Không tìm thấy công trình nào liên quan trên Thế Giới! (Tính Đột Phá Tiềm Năng Rất Cao)";
          }
      } catch (err: any) {
         contextPapers = `Lỗi API Scholar: ${err.message}`;
      }

      await sleep(500); // Tránh Rate Limit

      // 3. Evaluate by AI Judge
      const evaluatePrompt = `BẠN LÀ BAN GIÁM KHẢO XÉT DUYỆT CÔNG TRÌNH.
Ý tưởng được đề xuất: "${idea.title}"
Chi tiết: ${idea.core_idea}

========== CÁC BÀI BÁO THẾ GIỚI HIỆN CÓ (LIÊN QUAN ĐẾN TỪ KHOÁ: ${idea.keywords}): ==========
${contextPapers}
=====================================================
THỂ LỆ CHẤM ĐIỂM:
1. Đột phá (novelty_score): Từ 0-10. Nếu ý tưởng y hệt như các bài báo trên -> 0 đ. Mới hoàn toàn -> 10 đ.
2. Khả thi (feasibility_score): Từ 0-10. Dễ code, dễ triển khai -> 10 đ. Quá ảo tưởng -> 0 đ.

TRẢ VỀ DUY NHẤT MỘT CHUỖI JSON:
{"novelty_score": 8, "feasibility_score": 7, "review": "Đánh giá 2 câu..."}`;

      try {
          const resReview = await aiClient.chat.completions.create({
              model: "expert",
              messages: [{ role: "user", content: evaluatePrompt }],
              temperature: 0.1,
              max_tokens: 300
          });
          const reviewJsonRaw = resReview.choices[0]?.message?.content || "";
          
          let novelty = 0;
          let feasibility = 0;
          let reviewStr = "AI không chốt được chấm điểm.";

          const noveltyMatch = reviewJsonRaw.match(/"novelty_score"\s*:\s*(\d+)/);
          const feastMatch = reviewJsonRaw.match(/"feasibility_score"\s*:\s*(\d+)/);
          const rvMatch = reviewJsonRaw.match(/"review"\s*:\s*"([^"]+)"/);

          if (noveltyMatch && feastMatch) {
              novelty = parseInt(noveltyMatch[1]);
              feasibility = parseInt(feastMatch[1]);
              if (rvMatch) reviewStr = rvMatch[1];
          } else {
              // Try basic JSON parse
              const jMatch = reviewJsonRaw.match(/\{[\s\S]*\}/);
              if (jMatch) {
                  const jobj = JSON.parse(jMatch[0]);
                  novelty = jobj.novelty_score || 0;
                  feasibility = jobj.feasibility_score || 0;
                  reviewStr = jobj.review || "";
              }
          }

          const tsScore = (novelty * 0.65) + (feasibility * 0.35);
          scoredIdeas.push({
             ...idea, novelty, feasibility, totalScore: tsScore, review: reviewStr, relatedPapers: contextPapers
          });

          // THEO DÕI LOG BƯỚC ĐÁNH GIÁ (Nhật ký ngầm)
          let logStep = `## Ý Tưởng ${i + 1}: ${idea.title}\n`;
          logStep += `**Ý tưởng Cốt lõi:** ${idea.core_idea}\n\n`;
          logStep += `### 1. Dữ liệu Đối chứng (Semantic Scholar)\n${contextPapers}\n\n`;
          logStep += `### 2. Chấm Điểm AI (Ban Giám Khảo)\n`;
          logStep += `- Đột phá (Novelty): **${novelty}/10**\n`;
          logStep += `- Khả thi (Feasibility): **${feasibility}/10**\n`;
          logStep += `- Điểm quy đổi (Tổng): **${tsScore.toFixed(2)}**\n`;
          logStep += `- Bình duyệt (Review): ${reviewStr}\n\n---\n\n`;
          fs.appendFileSync(logPath, logStep, "utf8");

      } catch (err) {
         console.error("Judge lỗi:", err);
      }
  }

  // BƯỚC 4: THE CHOSEN ONE
  if (scoredIdeas.length === 0) {
      return "Toàn bộ 10 ý tưởng đã Fail tại khâu Judge. Quá đen.";
  }

  scoredIdeas.sort((a,b) => b.totalScore - a.totalScore);
  const bestIdea = scoredIdeas[0];

  await notifyZalo(`🏆 [Chốt Ý Tưởng]: Rèn luyện ròng rã, Giám Khảo AI đã chọn ra Ý Tưởng vô địch:
👉 "${bestIdea.title}"
⭐ Điểm Đột Phá: ${bestIdea.novelty}/10
⭐ Điểm Khả Thi: ${bestIdea.feasibility}/10
(Đánh giá: ${bestIdea.review})

Tiến hành bắt AI viết Siêu Kế Hoạch / Sách Trắng Phân Đoạn ngay lúc này! Mất khoảng 1 phút... `);

  fs.appendFileSync(logPath, `\n# 🏆 Ý TƯỞNG CHIẾN THẮNG CUỐI CÙNG: ${bestIdea.title}\n(Truy cập file Proposal Mở rộng: ${path.basename(targetPath)})`, "utf8");

  fs.appendFileSync(targetPath, `# PROPOSAL NGHIÊN CỨU: ${bestIdea.title}\n\n`, "utf8");
  fs.appendFileSync(targetPath, `**Mục tiêu Ban Giám Khảo (10 Ý Tưởng Candidate)**\nÝ tưởng Chiến Thắng có độ Đột Phá [${bestIdea.novelty}/10] và Độ Khả Thi [${bestIdea.feasibility}/10].\n*(Dữ liệu chi tiết về 9 ý tưởng bị đào thải được lưu trong Nhật ký: \`${path.basename(logPath)}\`)*\n\n---\n`, "utf8");

  const writeParts = [
      { name: "Phần 1: Giới thiệu Kiến Trúc & Lý thuyết Cốt Lõi", prompt: "Vẽ bức tranh Vĩ Mô. Khái niệm cơ bản của Ý Tưởng này là gì? Vì sao hệ thống này lại vượt qua được cấu trúc Truyền Thống?" },
      { name: "Phần 2: Sự vượt trội so với Khoa Học Hiện Tại", prompt: `Chứng minh tính Novelty (mới lạ). Hãy sử dụng dữ liệu sau để dìm hàng các giấy tờ xưa cũ, nâng tầm ý tưởng của ta:\n${bestIdea.relatedPapers}` },
      { name: "Phần 3: Phương Vị Kỹ Thuật (Implementation Plan)", prompt: "Để code được cái này trong LIVA hoặc Web AI, ta cần những công nghệ gì? Thiết kế hệ thống System Architecture ra sao? Có thể vẽ sơ đồ bằng dạng Markdowns/ASCII." },
      { name: "Phần 4: Kết luận & Rào Cản Rủi Ro", prompt: "Chốt tắt lợi ích và vạch ra Rủi ro có thể cản bước." }
  ];

  let dpHistory: any[] = [
      { role: "system", content: `Bạn là Nhà Khoa Học AI LIVA. Đây là Đề xuất Độc Quyền của bạn: ${bestIdea.title}. Core Idea: ${bestIdea.core_idea}` }
  ];

  for (const wpt of writeParts) {
      console.log(`[Proposal Writer] Đang viết ${wpt.name}...`);
      dpHistory.push({ role: "user", content: `HÃY VIẾT: **${wpt.name}**\nHướng dẫn: ${wpt.prompt}\nĐịnh dạng xuất chuẩn: BẮT BUỘC sử dụng ngữ pháp LaTeX để trình bày nội dung (VD: dùng \$\$...$\$ cho các biểu thức/thuật toán phức tạp, hoặc sử dụng các cấu trúc LaTeX nếu cần minh hoạ). Hãy viết dài và sặc mùi Học Thuật.` });

      try {
          const resWpt = await aiClient.chat.completions.create({
             model: "expert",
             messages: dpHistory,
             temperature: 0.3,
             max_tokens: 3000
          });
          const ans = resWpt.choices[0]?.message?.content || "*(Sinh lỗi)*";
          dpHistory.push({ role: "assistant", content: ans });
          
          fs.appendFileSync(targetPath, `\n\n## ${wpt.name}\n\n${ans}\n\n---\n`, "utf8");
          await notifyZalo(`✒️ Đã draft xong ${wpt.name}...`);
      } catch (err: any) {
          fs.appendFileSync(targetPath, `\n\n## ${wpt.name}\n*(Lỗi Model: ${err.message})*\n---\n`, "utf8");
      }
  }

  const absolutePath = path.resolve(targetPath);
  await notifyZalo(`✅ [SIÊU PHẨM HOÀN TẤT]: Vòng lặp SAKANA AI SCIENTIST đã rèn xong thanh gươm quý báu!
- Top 1 Tên Idea: ${bestIdea.title}
- Vị trí tài liệu Paper: ${absolutePath}
Sếp qua PC mở lên xem thành quả 1 tỷ đô nhé!`);

  return "Success! LIVA Sakana Loop hoàn tất tại " + absolutePath;
};
