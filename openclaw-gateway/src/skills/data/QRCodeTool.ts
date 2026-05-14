import { logger } from "@utils/logger";
import { SkillMetadata } from "../SkillMetadata";

export const metadata: SkillMetadata = {
  name: "qr_code_tool",
  category: "data",
  short_desc: "Generate or read QR codes.",
  semantic_tags: ["#qr", "#code", "#barcode", "#scan", "#generate"],
  search_keywords: ["qr", "qr code", "mã qr", "barcode", "quét"],
  description: "Generate QR codes from text/URLs. Saves as PNG image file.",
  parameters: {
    type: "object",
    properties: {
      data: { type: "string", description: "Text or URL to encode as QR code (required)." },
      output_path: { type: "string", description: "Where to save the QR PNG image (default: Desktop)." },
    },
    required: ["data"],
  },
};

export const execute = async (args: {
  data: string;
  output_path?: string;
}): Promise<string> => {
  if (!args.data?.trim()) return "Error: No data provided for QR code generation.";

  logger.info(`[Skill: qr_code_tool] Generating QR for: ${args.data.substring(0, 50)}...`);

  try {
    let QRCode: any;
    try {
      QRCode = await import("qrcode");
    } catch {
      return "Error: 'qrcode' package not installed. Run: npm install qrcode";
    }

    const os = await import("node:os");
    const path = await import("node:path");

    const outputPath = args.output_path?.trim() ||
      path.join(os.homedir(), "Desktop", `QR_${Date.now()}.png`);

    const { promises: fsp } = await import("node:fs");
    await fsp.mkdir(path.dirname(outputPath), { recursive: true });

    const tmpPath = outputPath + '.tmp';
    await QRCode.toFile(tmpPath, args.data.trim(), {
      type: "png",
      width: 400,
      margin: 2,
      color: { dark: "#000000", light: "#FFFFFF" },
    });
    
    // Atomic Write
    await fsp.rename(tmpPath, outputPath);

    return `✅ QR Code generated!\n📁 Saved to: ${outputPath}\n📝 Data: ${args.data.trim().substring(0, 100)}`;
  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    return `QR code error: ${errMsg}`;
  }
};
