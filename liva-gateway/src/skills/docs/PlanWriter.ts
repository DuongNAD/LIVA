import { executeDocumentWriter, DocumentSection } from "./DocumentWriterBase";
export const metadata = {
  name: "plan_writer",
  search_keywords: ["plan", "writer", "kế hoạch", "lộ trình", "action plan"],
  description:
    "[AUTO_RUN] Project Plan Writer. Use when the user requests 'Lập kế hoạch', 'Lên lộ trình', 'Plan ra mắt', or 'Action plan'. Automatically splits into 8 standard management sections (SWOT, SMART, Action Plan, Timeline, Budget, Risk Management).",
  parameters: {
    type: "object",
    properties: {
      projectName: {
        type: "string",
        description: "[VIETNAMESE] Project/plan name exactly as requested by user. Example: 'Kế hoạch ra mắt sản phẩm mới', 'Lộ trình Marketing Q2'.",
      },
      fileLocation: {
        type: "string",
        description: "Output directory for Markdown plan. Recommended: E:/Project/LIVA/scratch_workspace"
      },
      providedContext: {
         type: "string",
         description: "[VIETNAMESE] Real-world facts (requirements, budget, deadline) provided by user (if any).",
      }
    },
    required: ["projectName", "fileLocation"],
  },
};

export const execute = async (args: {
  projectName: string;
  fileLocation: string;
  providedContext?: string;
}): Promise<string> => {
  const parts: DocumentSection[] = [
    { name: "Part 1: Project Overview", instruction: "Plan Name, Main PIC (LIVA AI Project Manager), Purpose Summary: What problem does this plan solve?" },
    { name: "Part 2: Situation Analysis - SWOT", instruction: "Assess current state using SWOT framework. Analyze sharply." },
    { name: "Part 3: Objectives & KPIs - SMART", instruction: "Set objectives using SMART principles. Propose specific KPI numbers." },
    { name: "Part 4: Strategy & Action Plan", instruction: "Overall Strategy and Task List (What & How). Break down into Phases, Tasks, and PIC (Who)." },
    { name: "Part 5: Timeline", instruction: "Deadlines for each item. Present clearly in a Markdown Table format (acting as Gantt Chart)." },
    { name: "Part 6: Resources & Budget", instruction: "How much money? Detailed cost estimate table. HR and tools required." },
    { name: "Part 7: Risk Management", instruction: "Forecast potential risks and MUST provide Backup Plans (Plan B)." },
    { name: "Part 8: Evaluation Metrics", instruction: "Which metrics/tools measure success? Check-point frequency." }
  ];

  return executeDocumentWriter({
    title: args.projectName,
    workspace: args.fileLocation,
    type: "plan",
    systemPrompt: `You are the world's most outstanding Project Manager.
Your task is to create an EXTREMELY DETAILED FUTURE PLAN (Avoid generalizations; create realistic assumptions for numbers, deadlines, and practical tasks).
[CRITICAL INSTRUCTION] You MUST use Chain-of-Thought (CoT). First, analyze and plan your section using English inside a <thought> block. Then, output the final plan content in fluent VIETNAMESE inside a <report> block.
Example:
<thought>
I need to define SMART goals here. Let's aim for a 30% user growth.
</thought>
<report>
Mục tiêu là tăng trưởng 30% lượng người dùng...
</report>`,
    startMessage: `📋 [PlanWriter]: Đã nhận lệnh lập kế hoạch "${args.projectName}". Bắt đầu xây dựng 8 phần chuẩn chỉnh!`,
    endMessage: `🚀 [PlanWriter]: Kế hoạch kinh điển với 8 phần đã hoàn tất! 📂 Mời xem tại: {absolutePath}`,
    successMessage: "Hoàn tất Xuất chúng! Đã tạo thành công Bản Kế Hoạch tại: {absolutePath}",
    rawData: args.providedContext || "No specific requirements provided, propose reasonable numbers and data to illustrate a perfect plan.",
    parts,
    loggerPrefix: "[PlanWriter]",
    zaloPrefix: "🗓️ [LIVA Plan]"
  });
};
