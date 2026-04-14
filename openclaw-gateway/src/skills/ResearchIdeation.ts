import axios from "axios";
import fs from "fs";
import path from "path";
import { notifyZalo } from "../utils/ZaloNotifier";
import { livaEngine, generateSmartFilename } from "../utils/LivaEngine";

const sleep = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));

export const metadata = {
  name: "sakana_ideation",
  search_keywords: ["sakana_ideation","sakana ideation"],
  description:
    "Kỹ năng AI Scientist (Sakana). Hệ thống Tự Động Tư Duy: Tự đẻ ra 10 ý tưởng, tự Query Semantic Scholar xem có bị trùng lặp thế giới chưa, tự soi điểm Khả Thi và Đột Phá, tự chốt Ý Tưởng duy nhất để vẽ nên Kế hoạch Phát Triển Sản Phẩm AI vượt trội. Dùng khi người dùng yêu cầu 'Nghiên cứu ý tưởng', 'Đẻ idea đột phá'.",
  parameters: {
    type: "object",
    properties: {
      topic: {
        type: "string",
        description: "Chủ đề vĩ mô cần xin ý tưởng. Ví dụ: 'Kết hợp AI và Web'.",
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

  // Dùng LLM tự nặn tên file ngắn gọn (Smart Filename Naming)
  const shortName = await generateSmartFilename(args.topic, "research");

  const baseName = shortName.substring(0, 40);
  const targetPath = path.join(workspace, baseName + "_research.md");
  const rawIdPath = path.join(workspace, baseName + "_raw_ideas.json");
  const logPath = path.join(workspace, baseName + "_evaluation_log.md");
  
  fs.writeFileSync(targetPath, "", "utf8");
  fs.writeFileSync(logPath, `# NHẬT KÝ ĐÁNH GIÁ Ý TƯỞNG (AI SCIENTIST)\nChủ đề: ${args.topic}\n\n`, "utf8");

  // BƯỚC 1: BRANDSTORM 10 IDEAS
  console.log(`[AI Scientist] Phase 1: Ideation...`);
  const ideationPrompt = `You are the Research Legend of LIVA. Topic: ${args.topic}.
PROPOSE EXACTLY 10 Extremely Breakthrough (Novelty) and distinctly different ideas.
MUST RETURN A STRICT JSON ARRAY:
[
  {
     "id": 1,
     "title": "Idea title in Vietnamese",
     "keywords": "3 english search terms",
     "core_idea": "Where does the core breakthrough lie (3 sentences)"
  }
]
RETURN JSON ONLY. Absolutely no extra text.`;

  let ideas: Array<{id: number, title: string, keywords: string, core_idea: string}> = [];
  try {
     const resP1 = await livaEngine.chat.completions.create({
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

  // Đảm bảo không quá 10
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
      const evaluatePrompt = `YOU ARE THE PEER REVIEW JUDGE.
Proposed idea: "${idea.title}"
Details: ${idea.core_idea}

========== EXISTING WORLD PAPERS (RELATED TO KEYWORDS: ${idea.keywords}): ==========
${contextPapers}
=====================================================
SCORING RULES:
1. Novelty (novelty_score): From 0-10. If the idea is exactly the same as papers above -> 0 pt. Completely novel -> 10 pt.
2. Feasibility (feasibility_score): From 0-10. Easy to code and deploy -> 10 pt. Too unrealistic -> 0 pt.

RETURN EXACTLY ONE JSON STRING:
{"novelty_score": 8, "feasibility_score": 7, "review": "2 sentence review..."}`;

      try {
          const resReview = await livaEngine.chat.completions.create({
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
      { name: "Part 1: Architecture Introduction & Core Theory", prompt: "Draw the Macro picture. What is the fundamental concept of this Idea? Why does this system surpass Traditional structures?" },
      { name: "Part 2: Superiority Over Current Science", prompt: `Prove the Novelty. Use the following data to critique old papers and elevate our idea:\n${bestIdea.relatedPapers}` },
      { name: "Part 3: Implementation Engine & Plan", prompt: "To code this in LIVA or Web AI, what technologies do we need? What is the System Architecture design?" },
      { name: "Part 4: Conclusion & Risk Barriers", prompt: "Briefly summarize the benefits and outline potential risks." }
  ];

  let dpHistory: any[] = [
      { role: "system", content: `You are the LIVA AI Scientist. This is your Exclusive Proposal: ${bestIdea.title}. Core Idea: ${bestIdea.core_idea}` }
  ];

  for (const wpt of writeParts) {
      console.log(`[Proposal Writer] Đang viết ${wpt.name}...`);
      dpHistory.push({ role: "user", content: `PLEASE WRITE: **${wpt.name}**\nInstructions: ${wpt.prompt}\nStandard Output Format: YOU MUST use LaTeX syntax to present content (e.g., use \$\$...\$\$ for math/complex algorithms, or formatting structures). Write extensively with heavy Academic tone.` });

      try {
          const resWpt = await livaEngine.chat.completions.create({
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

  return `LIVA Sakana Loop hoàn tất tại ${absolutePath}`;
};
