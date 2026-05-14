import { spawn, exec } from "node:child_process";
import * as path from "node:path";
import * as os from "node:os";
import { promises as fsp, createWriteStream } from "node:fs";
import { pipeline } from "node:stream/promises";
import { logger } from "@utils/logger";
import { SkillMetadata } from "../SkillMetadata";

export const metadata: SkillMetadata = {
  name: "youtube_downloader",
  category: "web",
  short_desc: "Download and extract YouTube media.",
  semantic_tags: ["#youtube", "#download", "#video", "#audio", "#media"],
  search_keywords: ["youtube", "download", "tải", "video", "mp3", "mp4", "nhạc", "yt"],
  description: "Download YouTube videos or audio using yt-dlp CLI stream. Requires yt-dlp.",
  parameters: {
    type: "object",
    properties: {
      url: { type: "string", description: "YouTube video URL (required)." },
      format: { type: "string", enum: ["mp4", "mp3"], description: "'mp4' for video, 'mp3' for audio only (default: 'mp4')." },
      output_dir: { type: "string", description: "Save location (default: ~/Downloads/)." },
    },
    required: ["url"],
  },
};

function isValidYouTubeUrl(url: string): boolean {
  return /^https?:\/\/(www\.)?(youtube\.com\/(watch\?v=|shorts\/)|youtu\.be\/)/i.test(url);
}

export const execute = async (args: {
  url: string;
  format?: "mp4" | "mp3";
  output_dir?: string;
}): Promise<string> => {
  if (!args.url?.trim()) return "Error: No YouTube URL provided.";
  if (!isValidYouTubeUrl(args.url.trim())) return "Error: Invalid YouTube URL. Must be a youtube.com or youtu.be link.";

  const format = args.format || "mp4";
  const outputDir = args.output_dir?.trim() || path.join(os.homedir(), "Downloads");
  const url = args.url.trim();

  logger.info(`[Skill: youtube_downloader] Downloading ${format}: ${url}`);

  try {
    await fsp.mkdir(outputDir, { recursive: true });

    // Check if yt-dlp is available
    const checkCmd = process.platform === "win32" ? "where yt-dlp" : "which yt-dlp";
    await new Promise<void>((resolve, reject) => {
      exec(checkCmd, (err) => err ? reject(new Error("yt-dlp not found")) : resolve());
    });

    // Build yt-dlp command using stdout pipe
    const titleCmd = process.platform === "win32" 
        ? `yt-dlp --get-title "${url}"` 
        : `yt-dlp --get-title "${url}"`;

    const titleResult = await new Promise<string>((resolve) => {
        exec(titleCmd, (_err: Error | null, stdout: string) => resolve(stdout?.trim() || "video"));
    });
    
    // Sanitize filename
    const safeTitle = titleResult.replace(/[/\\?%*:|"<>]/g, '-');
    const ext = format === "mp3" ? "mp3" : "mp4";
    const outputFile = path.join(outputDir, `${safeTitle}.${ext}`);

    let cmdArgs: string[];
    if (format === "mp3") {
      cmdArgs = ["-x", "--audio-format", "mp3", "--audio-quality", "0", "-o", "-", url];
    } else {
      cmdArgs = ["-f", "best[ext=mp4]/best", "-o", "-", url]; // Request single stream so it can output to stdout without merge
    }

    const ytdlp = spawn("yt-dlp", cmdArgs, { stdio: ["ignore", "pipe", "pipe"] });
    
    // Stream.pipeline properly manages backpressure and prevents RAM bloating
    const writeStream = createWriteStream(outputFile);
    await pipeline(ytdlp.stdout, writeStream);

    return `✅ Download completed via Pipeline!\n🎬 Format: ${format.toUpperCase()}\n📁 Saved: ${path.basename(outputFile)}\n📂 Directory: ${outputDir}`;
  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    if (errMsg.includes("not found")) {
      return "Error: yt-dlp is not installed.\n\n📦 Install it:\n  Windows: winget install yt-dlp\n  Mac: brew install yt-dlp\n  Linux: sudo apt install yt-dlp";
    }
    return `Download error: ${errMsg}`;
  }
};
