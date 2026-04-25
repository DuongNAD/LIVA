/**
 * DocumentWriterBase — Shared Multi-Part Document Generation Engine
 * ==================================================================
 * Eliminates code duplication between PlanWriter and ReportWriter.
 * Both skills follow the same execution flow:
 *   1. Create workspace directory
 *   2. Generate smart filename via LLM
 *   3. Loop over document parts, calling LLM for each
 *   4. Append each part to the output Markdown file
 *   5. Notify via Zalo at each milestone
 *
 * Skills only need to provide: parts[], system prompt, file suffix, and notify messages.
 */
import { logger } from "../utils/logger";
import { promises as fsp } from "node:fs";
import path from "node:path";
import { notifyZalo } from "../utils/ZaloNotifier";
import { livaEngine, generateSmartFilename } from "../utils/LivaEngine";

export interface DocumentPart {
    name: string;
    instruction: string;
}

export interface DocumentWriterConfig {
    /** Skill name for logging, e.g. "PlanWriter" or "ReportWriter" */
    skillTag: string;
    /** File suffix, e.g. "_plan.md" or "_report.md" */
    fileSuffix: string;
    /** Filename hint for LLM, e.g. "plan" or "report" */
    fileHint: string;
    /** System prompt for the LLM conversation */
    systemPrompt: string;
    /** Document parts to generate */
    parts: DocumentPart[];
    /** User prompt template — receives part.name and part.instruction */
    userPromptTemplate: string;
    /** Zalo notification messages */
    notifications: {
        start: string;
        partDone: (partName: string) => string;
        complete: (absolutePath: string) => string;
    };
}

/**
 * Execute a multi-part document generation workflow.
 * Shared by PlanWriter and ReportWriter to eliminate code duplication.
 */
export async function executeDocumentWriter(
    topicOrProjectName: string,
    fileLocation: string,
    rawData: string,
    config: DocumentWriterConfig
): Promise<string> {
    const workspace = fileLocation;
    try {
        await fsp.mkdir(workspace, { recursive: true });
    } catch (err: any) {
        if (err.code !== "EEXIST") throw err;
    }

    await notifyZalo(config.notifications.start);

    // Generate smart filename via LLM
    const shortName = await generateSmartFilename(topicOrProjectName, config.fileHint);
    const targetPath = path.join(workspace, shortName.substring(0, 40) + config.fileSuffix);
    await fsp.writeFile(targetPath, "", "utf8");

    const conversation: any[] = [
        { role: "system", content: config.systemPrompt }
    ];

    for (let i = 0; i < config.parts.length; i++) {
        const part = config.parts[i];
        logger.info(`[${config.skillTag}] Đang viết ${part.name}...`);

        const userContent = config.userPromptTemplate
            .replace("{{PART_NAME}}", part.name)
            .replace("{{INSTRUCTION}}", part.instruction);

        conversation.push({ role: "user", content: userContent });

        try {
            const res = await livaEngine.chat.completions.create({
                model: "expert",
                messages: conversation,
                temperature: 0.35,
                max_tokens: 3000,
            });

            let replyContent = res.choices[0]?.message?.content || "";
            if (!replyContent || replyContent.length < 5) {
                replyContent = `*(Lỗi: Tràn quá trình sinh ký tự)*\n`;
            }

            conversation.push({ role: "assistant", content: replyContent });
            await fsp.appendFile(targetPath, `\n\n## ${part.name}\n\n${replyContent}\n\n---\n`, "utf8");
            await notifyZalo(config.notifications.partDone(part.name));
        } catch (e: any) {
            logger.error({ data: e.message }, `Error generating ${part.name}:`);
            await fsp.appendFile(targetPath, `\n\n## ${part.name}\n\n*(Lỗi mạng/VRAM)*\n\n---\n`, "utf8");
        }
    }

    const absolutePath = path.resolve(targetPath);
    await notifyZalo(config.notifications.complete(absolutePath));

    return absolutePath;
}
