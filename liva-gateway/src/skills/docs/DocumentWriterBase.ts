import { logger } from "@utils/logger";
import { promises as fsp } from "node:fs";
import path from "node:path";
import { notifyZalo } from "@utils/ZaloNotifier";
import { livaEngine, generateSmartFilename } from "@utils/LivaEngine";

export interface DocumentSection {
    name: string;
    instruction: string;
}

export type ContentEnricher = (rawData: string) => Promise<string>;

export interface DocumentWriterConfig {
    title: string;
    workspace: string;
    type: "plan" | "report";
    systemPrompt: string;
    startMessage: string;
    endMessage: string;
    successMessage: string;
    rawData: string;
    parts: DocumentSection[];
    loggerPrefix: string;
    zaloPrefix: string;
    enrichContent?: ContentEnricher;
}

export const executeDocumentWriter = async (config: DocumentWriterConfig): Promise<string> => {
    try {
        await fsp.mkdir(config.workspace, { recursive: true });
    } catch (err: unknown) {
        if (err instanceof Error && (err as NodeJS.ErrnoException).code !== "EEXIST") throw err;
    }

    await notifyZalo(config.startMessage);
    
    let processedData = config.rawData;
    if (config.enrichContent) {
        processedData = await config.enrichContent(processedData);
    }

    const shortName = await generateSmartFilename(config.title, config.type);
    const targetPath = path.join(config.workspace, shortName.substring(0, 40) + `_${config.type}.md`);
    await fsp.writeFile(targetPath, "", "utf8");

    const conversation: any[] = [
        { 
            role: "system", 
            content: `${config.systemPrompt}\n\n====================\n${processedData}\n====================`
        }
    ];

    for (let i = 0; i < config.parts.length; i++) {
        const part = config.parts[i];
        logger.info(`${config.loggerPrefix} Đang viết ${part.name}...`);
        
        conversation.push({ 
            role: "user", 
            content: `HÃY VIẾT: **${part.name}**\nHướng dẫn: ${part.instruction}\nYêu cầu: Viết dài, sâu sắc. BẮT BUỘC sử dụng Markdown kết hợp với cú pháp Toán học LaTeX ($$..$$ hoặc $..$) để làm nổi bật các phép tính và luận điểm. TRẢ VỀ TRỰC TIẾP NỘI DUNG của Phần này, KHÔNG CẦN CHÀO HỎI.` 
        });

        try {
            const res = await livaEngine.chat.completions.create({
                model: "expert",
                messages: conversation,
                temperature: 0.35,
                max_tokens: 3000,
            });

            let replyContent = res.choices[0]?.message?.content || "";
            if (!replyContent || replyContent.length < 5) {
                replyContent = `*(Lỗi rỗng do giới hạn API)*\n`;
            }

            conversation.push({ role: "assistant", content: replyContent });
            await fsp.appendFile(targetPath, `\n\n## ${part.name}\n\n${replyContent}\n\n---\n`, "utf8");
            await notifyZalo(`${config.zaloPrefix} Đã viết xong ${part.name}...`);

        } catch(e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            logger.error({ err: errMsg }, `Error generating ${part.name}:`);
            await fsp.appendFile(targetPath, `\n\n## ${part.name}\n\n*(Lỗi mạng/VRAM)*\n\n---\n`, "utf8");
        }
    }

    const absolutePath = path.resolve(targetPath);
    await notifyZalo(config.endMessage.replace("{absolutePath}", absolutePath));

    return config.successMessage.replace("{absolutePath}", absolutePath);
};
