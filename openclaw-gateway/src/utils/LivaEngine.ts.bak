import OpenAI from "openai";

export const livaEngine = new OpenAI({
    baseURL: "http://127.0.0.1:8000/v1",
    apiKey: "local-ghost-layer",
});

/**
 * Sử dụng LIVA AI để rút gọn một Chủ đề dài thành tên file tiếng Anh chuẩn mực.
 */
export async function generateSmartFilename(topic: string, defaultName: string): Promise<string> {
    let shortName = defaultName;
    try {
       const resName = await livaEngine.chat.completions.create({
          model: "expert",
          messages: [{ role: "user", content: `Hành động như một bot đổi tên file. Rút gọn cụm từ/chủ đề sau thành 1 tên file tiếng Anh ngắn gọn, súc tích (tối đa 4 từ, nối bằng gạch dưới _). VÍ DỤ: "Báo cáo doanh số quý 1" -> "q1_revenue_report". Tương tự hãy làm với Chủ đề: "${topic}". CHỈ TRẢ VỀ DUY NHẤT TÊN FILE (không giải thích, không in ký tự lạ).` }],
          temperature: 0.1,
          max_tokens: 20
       });
       const aiName = resName.choices[0]?.message?.content?.trim();
       if (aiName && !aiName.includes(" ")) {
           shortName = aiName.replace(/[^\w-]/g, "").toLowerCase();
       } else if (aiName) {
           shortName = aiName.replace(/[^\w-]/g, "_").replace(/_+/g, "_").toLowerCase();
       }
    } catch(e: any) {
       console.error("[LivaEngine] Smart Naming Error:", e.message);
    }
    return shortName;
}
