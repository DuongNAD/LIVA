import { z } from "zod";
import { logger } from "@utils/logger";

const ChartGeneratorSchema = z.object({
  title: z.string().describe("Tiêu đề biểu đồ"),
  type: z.enum(["bar", "line", "pie", "scatter", "heatmap"]).describe("Loại biểu đồ (ECharts)"),
  xAxisData: z.array(z.string()).optional().describe("Dữ liệu trục X"),
  seriesData: z.array(z.any()).describe("Dữ liệu trục Y (Mảng số hoặc object)"),
  description: z.string().optional().describe("Mô tả ngắn gọn ý nghĩa biểu đồ")
});

export const metadata = {
  name: "generate_chart_ui",
  search_keywords: ["biểu đồ", "chart", "đồ thị", "graph", "vẽ biểu đồ", "thống kê", "pie chart", "bar chart"],
  description: "[AUTO_RUN] Generate ECharts JSON config and push IPC event to display chart directly on Desktop/UI instead of static C++ or Python code.",
  kit: "SOCIAL_KIT", // Dùng SOCIAL_KIT hoặc GENERAL_KIT
  parameters: {
    type: "object",
    properties: {
      title: { type: "string" },
      type: { type: "string", enum: ["bar", "line", "pie", "scatter", "heatmap"] },
      xAxisData: { type: "array", items: { type: "string" } },
      seriesData: { type: "array", items: { type: "number" } },
      description: { type: "string" }
    },
    required: ["title", "type", "seriesData"],
  },
};

export const execute = async (argsObj: any): Promise<string> => {
    try {
        const parsed = ChartGeneratorSchema.parse(argsObj);
        
        // Tạo cấu hình ECharts chuẩn
        const chartConfig = {
            title: { text: parsed.title, subtext: parsed.description || "" },
            tooltip: { trigger: 'axis' },
            xAxis: parsed.xAxisData ? { type: 'category', data: parsed.xAxisData } : undefined,
            yAxis: { type: 'value' },
            series: [
                {
                    data: parsed.seriesData,
                    type: parsed.type,
                    smooth: parsed.type === "line"
                }
            ]
        };

        const ipcMessage = JSON.stringify({
            event: "SHOW_CHART",
            payload: chartConfig
        });

        // Bắn sự kiện IPC qua stdout tới hệ thống UI (Tauri v2)
        process.stdout.write(ipcMessage + "\n");
        logger.info(`[ChartGenerator] Đã bắn sự kiện hiển thị biểu đồ '${parsed.title}' lên UI.`);

        return `[CHART GENERATED] Đã tạo thành công biểu đồ "${parsed.title}" và hiển thị lên màn hình người dùng.\nChi tiết dữ liệu:\n${JSON.stringify(parsed.seriesData.slice(0, 5))}...`;

    } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
        logger.error(`[ChartGenerator] Lỗi: ${errMsg}`);
        if (error instanceof z.ZodError) {
            return `[CHART ERROR] Sai định dạng: ${error.issues.map(e => e.message).join(", ")}`;
        }
        return `[CHART ERROR] Lỗi hệ thống: ${errMsg}`;
    }
};
