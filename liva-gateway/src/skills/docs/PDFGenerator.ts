import { logger } from "@utils/logger";
import { SkillMetadata } from "../SkillMetadata";
import { Worker } from "node:worker_threads";
import { promises as fsp } from "node:fs";
import * as path from "node:path";

export const metadata: SkillMetadata = {
  name: "pdf_generator",
  category: "docs",
  short_desc: "Generate PDF files from markdown.",
  semantic_tags: ["#pdf", "#generate", "#document", "#print", "#export"],
  is_cpu_heavy: true,
  search_keywords: ["pdf", "tạo pdf", "generate", "document", "xuất", "export"],
  description: "Generate a formatted PDF document from text or markdown content.",
  parameters: {
    type: "object",
    properties: {
      content: { type: "string", description: "Text or markdown content to convert to PDF (required)." },
      title: { type: "string", description: "PDF document title (optional)." },
      output_path: { type: "string", description: "Where to save the PDF file (required)." },
    },
    required: ["content", "output_path"],
  },
};

export const execute = async (args: {
  content: string;
  title?: string;
  output_path: string;
}): Promise<string> => {
  if (!args.content?.trim()) return "Error: No content provided.";
  if (!args.output_path?.trim()) return "Error: No output path specified.";

  const outputPath = path.resolve(args.output_path.trim());
  await fsp.mkdir(path.dirname(outputPath), { recursive: true });

  logger.info(`[Skill: pdf_generator] Generating PDF: ${outputPath}`);

  const workerCode = `
    const { parentPort, workerData } = require('node:worker_threads');
    const fs = require('node:fs');
    const fsp = fs.promises;

    async function run() {
      try {
        const PDFDocument = require('pdfkit');
        const { content, title, outputPath } = workerData;
        
        const tmpPath = outputPath + '.tmp';
        const doc = new PDFDocument({ margin: 50, size: "A4" });
        const stream = fs.createWriteStream(tmpPath);
        
        const finished = new Promise((resolve, reject) => {
          stream.on("finish", resolve);
          stream.on("error", reject);
        });
        
        doc.pipe(stream);
        
        if (title && title.trim()) {
          doc.fontSize(24).font("Helvetica-Bold").text(title.trim(), { align: "center" });
          doc.moveDown(1.5);
        }
        
        const lines = content.split("\\n");
        for (const line of lines) {
          if (line.startsWith("# ")) {
            doc.fontSize(20).font("Helvetica-Bold").text(line.substring(2));
            doc.moveDown(0.5);
          } else if (line.startsWith("## ")) {
            doc.fontSize(16).font("Helvetica-Bold").text(line.substring(3));
            doc.moveDown(0.3);
          } else if (line.startsWith("### ")) {
            doc.fontSize(14).font("Helvetica-Bold").text(line.substring(4));
            doc.moveDown(0.2);
          } else if (line.startsWith("- ") || line.startsWith("* ")) {
            doc.fontSize(11).font("Helvetica").text(\`  •  \${line.substring(2)}\`, { indent: 10 });
          } else if (line.startsWith("**") && line.endsWith("**")) {
            doc.fontSize(11).font("Helvetica-Bold").text(line.replaceAll("**", ""));
          } else if (line.trim() === "") {
            doc.moveDown(0.5);
          } else {
            doc.fontSize(11).font("Helvetica").text(line);
          }
        }
        
        doc.end();
        await finished;
        
        // Atomic Write with retry (safeRename logic inline to avoid module path issues)
        let attempt = 0, renamed = false;
        while (attempt < 3 && !renamed) {
          try {
            await fsp.rename(tmpPath, outputPath);
            renamed = true;
          } catch (e) {
            attempt++;
            if (attempt >= 3 || !['EPERM', 'EBUSY', 'EACCES'].includes(e.code)) throw e;
            await new Promise(r => setTimeout(r, 50 * Math.pow(2, attempt - 1)));
          }
        }
        
        const stat = await fsp.stat(outputPath);
        const sizeKB = (stat.size / 1024).toFixed(1);
        
        parentPort.postMessage({
          success: true,
          result: \`✅ PDF generated!\\n📄 Path: \${outputPath}\\n📐 Size: \${sizeKB} KB\\n📝 Title: \${title || "(untitled)"}\`
        });
      } catch (err) {
        parentPort.postMessage({ success: false, error: (err && err.message) ? err.message : String(err) });
      }
    }
    run();
  `;

  return new Promise<string>((resolve) => {
    let isDone = false;
    const worker = new Worker(workerCode, {
      eval: true,
      workerData: {
        content: args.content,
        title: args.title,
        outputPath
      }
    });

    const watchdog = setTimeout(() => {
        if (!isDone) {
            isDone = true;
            logger.error(`[Watchdog] PDFGenerator worker deadlocked. Terminating...`);
            worker.terminate();
            resolve(`Error: PDF generation timed out after 30 seconds.`);
        }
    }, 30000);

    const cleanup = () => {
        isDone = true;
        clearTimeout(watchdog);
    };

    worker.on("message", (msg) => {
      cleanup();
      if (msg.success) resolve(msg.result);
      else resolve(`PDF generation error: ${msg.error}`);
    });

    worker.on("error", (err: Error) => {
        cleanup();
        resolve(`Worker error: ${err.message}`);
    });
    
    worker.on("exit", (code) => {
      cleanup();
      if (code !== 0) resolve(`Worker stopped with exit code ${code}`);
    });
  });
};
