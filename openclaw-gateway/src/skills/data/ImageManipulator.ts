import { safeRename } from '../../utils/FileUtils';
import { logger } from "@utils/logger";
import { SkillMetadata } from "../SkillMetadata";
import { Worker } from "node:worker_threads";
import { promises as fsp } from "node:fs";
import * as path from "node:path";

export const metadata: SkillMetadata = {
  name: "image_manipulator",
  category: "data",
  short_desc: "Manipulate images: resize, compress, format.",
  semantic_tags: ["#image", "#anh", "#resize", "#compress", "#format"],
  is_cpu_heavy: true,
  search_keywords: ["image", "ảnh", "resize", "compress", "convert", "nén", "chuyển đổi", "hình"],
  description: "Manipulate images: resize, compress, convert format (PNG/JPG/WebP), or get info. Requires 'sharp' package.",
  parameters: {
    type: "object",
    properties: {
      action: { type: "string", enum: ["resize", "compress", "convert", "info"], description: "Operation to perform (required)." },
      input_path: { type: "string", description: "Path to the source image (required)." },
      output_path: { type: "string", description: "Output path (auto-generated if omitted)." },
      width: { type: "number", description: "Target width for resize (maintains aspect ratio if height omitted)." },
      height: { type: "number", description: "Target height for resize." },
      format: { type: "string", enum: ["png", "jpg", "webp", "avif"], description: "Target format for 'convert' action." },
      quality: { type: "number", description: "Compression quality 1-100 (default: 80)." },
    },
    required: ["action", "input_path"],
  },
};

export const execute = async (args: {
  action: "resize" | "compress" | "convert" | "info";
  input_path: string;
  output_path?: string;
  width?: number;
  height?: number;
  format?: string;
  quality?: number;
}): Promise<string> => {
  if (!args.input_path?.trim()) return "Error: No input image path provided.";

  const inputPath = path.resolve(args.input_path.trim());
  try {
    await fsp.access(inputPath);
  } catch {
    return `Error: Image not found at ${inputPath}`;
  }

  logger.info(`[Skill: image_manipulator] ${args.action} on ${inputPath}`);

  // Create inline worker code
  const workerCode = `
    const { parentPort, workerData } = require('node:worker_threads');
    const fs = require('node:fs/promises');
    const path = require('node:path');

    async function run() {
      try {
        const sharp = require('sharp');
        const { action, inputPath, outputPath, width, height, format, quality } = workerData;
        const img = sharp(inputPath);
        const meta = await img.metadata();
        
        if (action === "info") {
          parentPort.postMessage({
            success: true,
            result: \`[IMAGE INFO]\\n📁 Path: \${inputPath}\\n📐 Size: \${meta.width}x\${meta.height}px\\n🎨 Format: \${meta.format}\\n💾 File size: \${((meta.size || 0) / 1024).toFixed(1)} KB\\n🔍 Channels: \${meta.channels}\\n📏 DPI: \${meta.density || "N/A"}\`
          });
          return;
        }

        let outPath = outputPath || "";
        const tmpPath = outPath ? outPath + '.tmp' : '';

        if (action === "resize") {
          if (!outPath) outPath = inputPath.replace(path.extname(inputPath), \`_resized\${path.extname(inputPath)}\`);
          const tp = outPath + '.tmp';
          await img.resize(width || null, height || null, { fit: "inside" }).toFile(tp);
          await safeRename(tp, outPath); // Atomic Write
          
          const outMeta = await sharp(outPath).metadata();
          parentPort.postMessage({
            success: true,
            result: \`✅ Image resized!\\n📐 \${meta.width}x\${meta.height} → \${outMeta.width}x\${outMeta.height}\\n📁 Saved: \${outPath}\`
          });
          return;
        }

        if (action === "compress") {
          if (!outPath) outPath = inputPath.replace(path.extname(inputPath), \`_compressed\${path.extname(inputPath)}\`);
          const tp = outPath + '.tmp';
          const fmt = meta.format || "jpeg";
          if (fmt === "jpeg" || fmt === "jpg") await img.jpeg({ quality }).toFile(tp);
          else if (fmt === "png") await img.png({ quality }).toFile(tp);
          else if (fmt === "webp") await img.webp({ quality }).toFile(tp);
          else await img.jpeg({ quality }).toFile(tp);
          
          await safeRename(tp, outPath); // Atomic Write

          const origSize = (await fs.stat(inputPath)).size;
          const newSize = (await fs.stat(outPath)).size;
          const saved = ((1 - newSize / origSize) * 100).toFixed(1);
          parentPort.postMessage({
            success: true,
            result: \`✅ Image compressed! (quality: \${quality})\\n💾 \${(origSize / 1024).toFixed(1)} KB → \${(newSize / 1024).toFixed(1)} KB (\${saved}% smaller)\\n📁 Saved: \${outPath}\`
          });
          return;
        }

        if (action === "convert") {
          if (!outPath) outPath = inputPath.replace(path.extname(inputPath), \`.\${format}\`);
          const tp = outPath + '.tmp';
          await img.toFormat(format, { quality }).toFile(tp);
          await safeRename(tp, outPath); // Atomic Write
          
          const newSize = (await fs.stat(outPath)).size;
          parentPort.postMessage({
            success: true,
            result: \`✅ Image converted! \${meta.format} → \${format}\\n💾 Size: \${(newSize / 1024).toFixed(1)} KB\\n📁 Saved: \${outPath}\`
          });
          return;
        }
        
        parentPort.postMessage({ success: false, error: "Invalid action." });
      } catch (err) {
        parentPort.postMessage({ success: false, error: (err && err.message) ? err.message : String(err) });
      }
    }
    run();
  `;

  return new Promise<string>((resolve) => {
    const worker = new Worker(workerCode, {
      eval: true,
      workerData: {
        action: args.action,
        inputPath,
        outputPath: args.output_path?.trim(),
        width: args.width,
        height: args.height,
        format: args.format,
        quality: Math.min(Math.max(args.quality || 80, 1), 100),
      }
    });

    worker.on("message", (msg) => {
      if (msg.success) resolve(msg.result);
      else resolve(`Image manipulation error: ${msg.error}`);
    });

    worker.on("error", (err: Error) => {
      resolve(`Worker error: ${err.message}`);
    });

    worker.on("exit", (code) => {
      if (code !== 0) resolve(`Worker stopped with exit code ${code}`);
    });
  });
};
