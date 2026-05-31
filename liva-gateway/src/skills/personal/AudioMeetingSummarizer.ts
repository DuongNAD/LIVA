import { promises as fsp } from "node:fs";
import * as path from "node:path";
import { exec } from "node:child_process";
import { promisify } from "node:util";
import { safeFetch } from "@utils/HttpClient";
import { logger } from "@utils/logger";
import { safeRename } from "@utils/FileUtils";
import { SkillMetadata } from "../SkillMetadata";

const execAsync = promisify(exec);

export const metadata: SkillMetadata = {
  name: "audio_meeting_summarizer",
  category: "personal",
  short_desc: "Transcribe meeting audio file and generate AI summary.",
  semantic_tags: ["#meeting", "#audio", "#transcribe", "#whisper", "#summarize"],
  search_keywords: ["meeting", "audio", "transcribe", "whisper", "summarize", "họp", "ghi âm", "tóm tắt"],
  description: "Transcribe a recorded meeting audio file using the Whisper engine and generate a structured summary including key topics, discussion points, decisions, and action items using the local or cloud LLM. Automatically splits large files (>20MB) into chunks.",
  parameters: {
    type: "object",
    properties: {
      audio_path: {
        type: "string",
        description: "The absolute path to the recorded audio file (e.g. .wav or .mp3) to transcribe and summarize."
      },
      output_path: {
        type: "string",
        description: "Optional absolute path to write the markdown summary report."
      },
      language: {
        type: "string",
        description: "Optional language for the final summary (e.g., 'Vietnamese', 'English'). Default is 'Vietnamese'."
      }
    },
    required: ["audio_path"]
  }
};

const getWhisperEndpoint = (): string => {
  if (process.env.WHISPER_URL) {
    return process.env.WHISPER_URL;
  }
  const isLocalLLM = process.env.AI_PROVIDER === "local";
  if (isLocalLLM && process.env.WHISPER_CLOUD_URL) {
    return process.env.WHISPER_CLOUD_URL;
  }
  return "http://127.0.0.1:8100/v1/audio/transcriptions";
};

const buildRequestHeaders = (endpoint: string): Record<string, string> => {
  const headers: Record<string, string> = {};
  const isCloud = !endpoint.includes("localhost") && !endpoint.includes("127.0.0.1");
  if (isCloud) {
    const apiKey = process.env.AI_API_KEY;
    if (apiKey) {
      headers["Authorization"] = `Bearer ${apiKey}`;
    }
  }
  return headers;
};

async function splitAudioFile(inputPath: string, outputDir: string, segmentTimeSec = 600): Promise<string[]> {
  try {
    await execAsync("ffmpeg -version");
  } catch {
    throw new Error("ffmpeg command line tool is not installed or not available on system PATH. Cannot split large audio files (>20MB).");
  }

  const ext = path.extname(inputPath);
  const baseName = path.basename(inputPath, ext);
  const outputPattern = path.join(outputDir, `${baseName}_%03d${ext}`);

  // ffmpeg command using segment muxer. Try fast stream copy first.
  try {
    const command = `ffmpeg -y -i "${inputPath}" -f segment -segment_time ${segmentTimeSec} -c copy "${outputPattern}"`;
    logger.debug(`[AudioMeetingSummarizer] Splitting file using copy: ${command}`);
    await execAsync(command);
  } catch (err) {
    logger.warn(`[AudioMeetingSummarizer] Segment with copy failed, trying with re-encoding: ${(err as Error).message}`);
    const command = `ffmpeg -y -i "${inputPath}" -f segment -segment_time ${segmentTimeSec} "${outputPattern}"`;
    await execAsync(command);
  }

  const files = await fsp.readdir(outputDir);
  const matchedFiles = files
    .filter(f => f.startsWith(`${baseName}_`) && f.endsWith(ext))
    .map(f => path.join(outputDir, f))
    .sort();

  return matchedFiles;
}

async function transcribeFile(filePath: string): Promise<string> {
  const fileBuffer = await fsp.readFile(filePath);
  const fileName = path.basename(filePath);
  
  const blob = new Blob([fileBuffer], { type: "audio/wav" });
  const fd = new FormData();
  fd.append("file", blob, fileName);
  fd.append("response_format", "text");

  const whisperEndpoint = getWhisperEndpoint();
  const headers = buildRequestHeaders(whisperEndpoint);

  const response = await safeFetch(whisperEndpoint, {
    method: "POST",
    body: fd,
    headers
  }, 180000); // 3-minute timeout

  if (!response.ok) {
    throw new Error(`Whisper API returned error status: ${response.status} ${response.statusText}`);
  }

  const rawTranscript = await response.text();
  let transcript = rawTranscript.trim();

  if (transcript.startsWith("{")) {
    try {
      const parsed = JSON.parse(transcript);
      transcript = parsed.text || parsed.transcription || transcript;
    } catch {
      // Fallback
    }
  }

  return transcript;
}

export const execute = async (args: {
  audio_path: string;
  output_path?: string;
  language?: string;
}): Promise<string> => {
  if (!args.audio_path?.trim()) {
    return "Error: Please provide a valid 'audio_path'.";
  }

  const audioPath = path.resolve(args.audio_path.trim());
  const language = args.language?.trim() || "Vietnamese";

  try {
    await fsp.access(audioPath);
  } catch {
    return `Error: Audio file not found at path: ${audioPath}`;
  }

  let tempDir = "";
  try {
    const stats = await fsp.stat(audioPath);
    const fileSizeMb = stats.size / (1024 * 1024);
    let transcript = "";

    if (fileSizeMb > 20) {
      logger.info(`[Skill: audio_meeting_summarizer] File size (${fileSizeMb.toFixed(2)}MB) exceeds 20MB limit. Splitting into chunks...`);
      tempDir = path.join(path.dirname(audioPath), `_liva_chunks_${Date.now()}`);
      await fsp.mkdir(tempDir, { recursive: true });

      const chunks = await splitAudioFile(audioPath, tempDir, 600); // 10 minutes chunks
      logger.info(`[Skill: audio_meeting_summarizer] Split into ${chunks.length} chunks.`);

      const transcripts: string[] = [];
      for (let i = 0; i < chunks.length; i++) {
        logger.info(`[Skill: audio_meeting_summarizer] Transcribing chunk ${i + 1}/${chunks.length}...`);
        const chunkTranscript = await transcribeFile(chunks[i]);
        if (chunkTranscript) {
          transcripts.push(chunkTranscript);
        }
      }
      transcript = transcripts.join("\n ");
    } else {
      transcript = await transcribeFile(audioPath);
    }

    if (!transcript) {
      return "Error: Transcription was empty. No speech detected in the audio file.";
    }

    logger.info(`[Skill: audio_meeting_summarizer] Transcription complete (${transcript.length} chars). Generating AI summary...`);

    const llmUrl = process.env.LLM_ENDPOINT || "http://localhost:8000/v1/chat/completions";

    const llmResponse = await safeFetch(llmUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        model: "local",
        messages: [
          {
            role: "system",
            content: `You are an expert meeting assistant. Analyze the meeting transcript below and compile a detailed markdown summary in ${language}. Structure the report as follows:
1. **General Info**: Meeting title, duration, date/time (if detectable).
2. **Key Discussion Points**: Detailed bullet points of main topics discussed.
3. **Key Decisions**: Bullet points of agreed-upon decisions.
4. **Action Items**: A Markdown table with columns: Task, Owner, Deadline (if specified).

Keep the summary structured, accurate, and professional. Output only the markdown summary.`
          },
          {
            role: "user",
            content: transcript
          }
        ],
        temperature: 0.3,
        max_tokens: 2048
      })
    }, 60000);

    const data = await llmResponse.json();
    const summary = data.choices?.[0]?.message?.content?.trim();

    if (!summary) {
      return "Error: Summary generation failed. LLM returned no response.";
    }

    const resultSummary = `### 📹 Meeting Summary Report\n\n${summary}`;

    if (args.output_path?.trim()) {
      const outputPath = path.resolve(args.output_path.trim());
      const outDir = path.dirname(outputPath);
      
      await fsp.mkdir(outDir, { recursive: true });
      const tmpPath = `${outputPath}.tmp`;
      await fsp.writeFile(tmpPath, resultSummary, "utf-8");
      await safeRename(tmpPath, outputPath);
      logger.info(`[Skill: audio_meeting_summarizer] Summary written to ${outputPath}`);
      return `✅ Transcription and summary complete. Report saved to: ${outputPath}\n\n${resultSummary}`;
    }

    return resultSummary;

  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    logger.error(`[Skill: audio_meeting_summarizer] Failed: ${errMsg}`);
    return `Error processing meeting audio: ${errMsg}`;
  } finally {
    if (tempDir) {
      try {
        await fsp.rm(tempDir, { recursive: true, force: true });
        logger.debug(`[Skill: audio_meeting_summarizer] Temporary chunk directory cleaned: ${tempDir}`);
      } catch (err) {
        logger.warn(`[Skill: audio_meeting_summarizer] Failed to clean temporary directory ${tempDir}: ${(err as Error).message}`);
      }
    }
  }
};
