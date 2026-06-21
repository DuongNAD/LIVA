import { GeminiAPI } from "../../tools/GeminiAPI";
import { logger } from "../../utils/logger";
import { z } from "zod";
import fs from "fs";

export const metadata = {
  name: "vision_analysis",
  search_keywords: ["vision_analysis", "phân tích ảnh", "nhìn ảnh", "xem ảnh", "đọc ảnh", "image analysis", "read image"],
  short_desc: "Analyze and answer questions about an image.",
  description: "[AUTO_RUN] Use this skill to 'see' an image and answer questions about it. Provide the absolute file path to the image and a prompt describing what to look for.",
  parameters: {
    type: "object",
    properties: {
      imagePath: {
        type: "string",
        description: "The absolute path to the local image file (e.g. C:/images/photo.png).",
      },
      prompt: {
        type: "string",
        description: "What to ask or look for in the image (e.g. 'What text is in this image?', 'Describe this picture').",
      },
    },
    required: ["imagePath", "prompt"],
  },
};

export const execute = async (args: { imagePath: string; prompt: string }): Promise<string> => {
  try {
    if (!fs.existsSync(args.imagePath)) {
      return `Lỗi: Không tìm thấy tệp hình ảnh tại đường dẫn ${args.imagePath}`;
    }

    const imageBuffer = fs.readFileSync(args.imagePath);
    const base64Image = imageBuffer.toString("base64");
    
    let mimeType = "image/jpeg";
    const lowerPath = args.imagePath.toLowerCase();
    if (lowerPath.endsWith(".png")) mimeType = "image/png";
    else if (lowerPath.endsWith(".webp")) mimeType = "image/webp";
    else if (lowerPath.endsWith(".heic")) mimeType = "image/heic";
    else if (lowerPath.endsWith(".heif")) mimeType = "image/heif";

    logger.info(`[Skill: vision_analysis] Đang phân tích ảnh bằng Gemini: ${args.imagePath}`);
    
    const analysisResult = await GeminiAPI.analyzeImage(base64Image, mimeType, args.prompt);
    
    return `[Vision Analysis Result]\n${analysisResult}`;
  } catch (error: any) {
    logger.error(`[Skill: vision_analysis] Error: ${error.message}`);
    return `Lỗi trong quá trình phân tích ảnh: ${error.message}`;
  }
};
