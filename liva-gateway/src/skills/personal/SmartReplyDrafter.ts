import { z } from "zod";
import { logger } from "@utils/logger";
import { safeFetch } from "@utils/HttpClient";
import LRUCache from "lru-cache";
import { randomUUID } from "node:crypto";

// ─── Draft Types ─────────────────────────────────────────────────────────────

interface DraftSet {
    originalMessage: string;
    sender: string;
    context: string;
    options: [string, string, string];
    generatedAt: number;
}

// ─── LRU Draft Cache — max 50 entries, TTL 1 hour ───────────────────────────

const draftCache = new LRUCache<string, DraftSet>({
    max: 50,
    ttl: 60 * 60 * 1000, // 1 giờ
});

// ─── Zod Input Schema ────────────────────────────────────────────────────────

const SmartReplySchema = z.object({
    action: z.enum(["draft", "send_selected"]).describe("Hành động: soạn thảo hoặc gửi phương án đã chọn"),
    originalMessage: z.string().optional().describe("Nội dung tin nhắn nhận được"),
    senderName: z.string().optional().describe("Tên người gửi"),
    context: z.enum(["email", "zalo", "telegram"]).optional().describe("Ngữ cảnh giao tiếp"),
    selectedOption: z.number().min(1).max(3).optional().describe("Phương án đã chọn (1, 2, hoặc 3)"),
    customEdit: z.string().optional().describe("Nội dung tuỳ chỉnh của người dùng trước khi gửi"),
    draftId: z.string().optional().describe("ID bản nháp để gửi"),
});

// ─── Metadata ────────────────────────────────────────────────────────────────

export const metadata = {
    name: "smart_reply_drafter",
    description:
        "[AUTO_RUN] Draft 3 reply options for incoming messages/emails. Options: Accept, Politely Decline, Reschedule. User selects one and LIVA sends via RPA.",
    kit: "SOCIAL_KIT",
    category: "social" as const,
    search_keywords: [
        "smart_reply_drafter",
        "reply",
        "trả lời",
        "soạn tin",
        "draft",
        "phản hồi",
        "email reply",
        "zalo reply",
        "telegram reply",
        "đồng ý",
        "từ chối",
        "hẹn lại",
    ],
    parameters: {
        type: "object",
        properties: {
            action: {
                type: "string",
                enum: ["draft", "send_selected"],
                description: "Action: draft new reply options or send a selected option",
            },
            originalMessage: {
                type: "string",
                description: "The received message content to draft replies for",
            },
            senderName: {
                type: "string",
                description: "Name of the person who sent the message",
            },
            context: {
                type: "string",
                enum: ["email", "zalo", "telegram"],
                description: "Communication context / platform",
            },
            selectedOption: {
                type: "number",
                enum: [1, 2, 3],
                description: "Which reply option to send (1=Accept, 2=Decline, 3=Reschedule)",
            },
            customEdit: {
                type: "string",
                description: "Optional custom edit to the selected reply before sending",
            },
            draftId: {
                type: "string",
                description: "Draft ID returned from the draft action",
            },
        },
        required: ["action"],
    },
};

// ─── Fallback Templates (Semantic Cache L0.5 — NO main LLM call) ────────────

const FALLBACK_OPTIONS: [string, string, string] = [
    "Ok, anh/chị nhé! Anh sẽ xử lý ngay.",
    "Cảm ơn anh/chị, nhưng hiện tại anh chưa sắp xếp được ạ.",
    "Để anh check lại và phản hồi sau nhé!",
];

// ─── Cloud API Helper — Asymmetric Routing (preserve local VRAM) ─────────────

async function generateDraftsViaCloud(
    senderName: string,
    originalMessage: string
): Promise<[string, string, string] | null> {
    const baseUrl = process.env.AI_BASE_URL;
    const apiKey = process.env.AI_API_KEY;
    const model = process.env.AI_MODEL || "gemini-2.5-flash";

    if (!baseUrl || !apiKey) {
        return null;
    }

    const endpoint = baseUrl.endsWith("/")
        ? `${baseUrl}chat/completions`
        : `${baseUrl}/chat/completions`;

    const systemPrompt =
        "Bạn là trợ lý soạn tin nhắn cho một developer Việt Nam tên Dương. " +
        "Soạn 3 phương án trả lời cho tin nhắn sau. " +
        "Option 1: Đồng ý/Chấp nhận. " +
        "Option 2: Từ chối khéo léo. " +
        "Option 3: Hẹn lại/Trì hoãn. " +
        "Mỗi option ngắn gọn, tự nhiên, đúng ngữ cảnh. " +
        "Format: OPTION_1: ..., OPTION_2: ..., OPTION_3: ...";

    const userMessage = `Tin nhắn từ ${senderName}: "${originalMessage}"`;

    const res = await safeFetch(
        endpoint,
        {
            method: "POST",
            headers: {
                "Content-Type": "application/json",
                Authorization: `Bearer ${apiKey}`,
            },
            body: JSON.stringify({
                model,
                messages: [
                    { role: "system", content: systemPrompt },
                    { role: "user", content: userMessage },
                ],
                max_tokens: 500,
                temperature: 0.7,
            }),
        },
        20000
    );

    const data = (await res.json()) as {
        choices?: Array<{ message?: { content?: string } }>;
    };
    const content = data.choices?.[0]?.message?.content;

    if (!content) {
        return null;
    }

    return parseCloudResponse(content);
}

// ─── Response Parser ─────────────────────────────────────────────────────────

function parseCloudResponse(raw: string): [string, string, string] {
    const lines = raw.split("\n").filter((l) => l.trim().length > 0);

    let opt1 = "";
    let opt2 = "";
    let opt3 = "";

    for (const line of lines) {
        const trimmed = line.trim();
        // Hỗ trợ nhiều format: "OPTION_1:", "Option 1:", "1.", "1)"
        if (/^(OPTION[_\s]?1|Option\s*1|1[.):])\s*[:.)]\s*/i.test(trimmed)) {
            opt1 = trimmed.replace(/^(OPTION[_\s]?1|Option\s*1|1[.):])\s*[:.)]\s*/i, "").trim();
        } else if (/^(OPTION[_\s]?2|Option\s*2|2[.):])\s*[:.)]\s*/i.test(trimmed)) {
            opt2 = trimmed.replace(/^(OPTION[_\s]?2|Option\s*2|2[.):])\s*[:.)]\s*/i, "").trim();
        } else if (/^(OPTION[_\s]?3|Option\s*3|3[.):])\s*[:.)]\s*/i.test(trimmed)) {
            opt3 = trimmed.replace(/^(OPTION[_\s]?3|Option\s*3|3[.):])\s*[:.)]\s*/i, "").trim();
        }
    }

    // Fallback nếu parse thất bại: lấy 3 dòng đầu tiên
    if (!opt1 && !opt2 && !opt3 && lines.length >= 3) {
        opt1 = lines[0].replace(/^\d+[.):\s]+/, "").trim();
        opt2 = lines[1].replace(/^\d+[.):\s]+/, "").trim();
        opt3 = lines[2].replace(/^\d+[.):\s]+/, "").trim();
    }

    return [
        opt1 || FALLBACK_OPTIONS[0],
        opt2 || FALLBACK_OPTIONS[1],
        opt3 || FALLBACK_OPTIONS[2],
    ];
}

// ─── Context → Skill Routing Map ─────────────────────────────────────────────

function getSendInstruction(context: string, replyText: string, senderName: string): string {
    switch (context) {
        case "zalo":
            return (
                `[NEXT_ACTION] Gọi skill "send_zalo_rpa" với targetName="${senderName}" ` +
                `và message="${replyText}" để gửi tin nhắn qua Zalo Web.`
            );
        case "telegram":
            return (
                `[NEXT_ACTION] Gọi skill gửi tin nhắn Telegram đến "${senderName}" ` +
                `với nội dung: "${replyText}".`
            );
        case "email":
            return (
                `[NEXT_ACTION] Gọi skill "send_email" để trả lời email cho "${senderName}" ` +
                `với nội dung: "${replyText}".`
            );
        default:
            return (
                `[NEXT_ACTION] Nền tảng "${context}" — hãy gửi tin nhắn thủ công. ` +
                `Nội dung: "${replyText}".`
            );
    }
}

// ─── Execute Function ────────────────────────────────────────────────────────

export const execute = async (argsObj: any): Promise<string> => {
    try {
        const parsed = SmartReplySchema.parse(argsObj);

        // ──────── ACTION: DRAFT ────────
        if (parsed.action === "draft") {
            const originalMessage = parsed.originalMessage;
            const senderName = parsed.senderName || "Ai đó";
            const context = parsed.context || "zalo";

            if (!originalMessage || originalMessage.trim().length === 0) {
                return "[SMART_REPLY ERROR] Thiếu nội dung tin nhắn gốc (originalMessage). Vui lòng cung cấp tin nhắn cần trả lời.";
            }

            logger.info(
                `[SmartReplyDrafter] Đang soạn 3 phương án trả lời cho "${senderName}" (${context})...`
            );

            // Thử Cloud API trước (Asymmetric Routing — bảo toàn VRAM local)
            let options: [string, string, string];
            let source: string;

            try {
                const cloudResult = await generateDraftsViaCloud(senderName, originalMessage);
                if (cloudResult) {
                    options = cloudResult;
                    source = "Cloud AI";
                    logger.info("[SmartReplyDrafter] Đã tạo phương án qua Cloud API.");
                } else {
                    options = [...FALLBACK_OPTIONS];
                    source = "Template mặc định";
                    logger.info("[SmartReplyDrafter] Cloud API không khả dụng — dùng template mặc định.");
                }
            } catch (err: unknown) {
                const errMsg = err instanceof Error ? err.message : String(err);
                logger.warn(`[SmartReplyDrafter] Cloud API lỗi: ${errMsg} — dùng template.`);
                options = [...FALLBACK_OPTIONS];
                source = "Template mặc định (fallback)";
            }

            // Lưu draft vào LRU Cache
            const draftId = `draft_${randomUUID().substring(0, 8)}`;
            const draftSet: DraftSet = {
                originalMessage,
                sender: senderName,
                context,
                options,
                generatedAt: Date.now(),
            };
            draftCache.set(draftId, draftSet);

            logger.info(`[SmartReplyDrafter] Đã lưu draft ${draftId} (nguồn: ${source}).`);

            return (
                `[SMART_REPLY SUCCESS] Đã soạn 3 phương án trả lời cho tin nhắn từ "${senderName}" (${context}):\n\n` +
                `📨 Tin nhắn gốc: "${originalMessage}"\n\n` +
                `✅ **Option 1 (Đồng ý):** ${options[0]}\n` +
                `❌ **Option 2 (Từ chối):** ${options[1]}\n` +
                `🔄 **Option 3 (Hẹn lại):** ${options[2]}\n\n` +
                `🔑 Draft ID: ${draftId}\n` +
                `📝 Nguồn: ${source}\n\n` +
                `Hãy hỏi người dùng chọn phương án 1, 2, hoặc 3. ` +
                `Sau đó gọi lại skill này với action="send_selected", draftId="${draftId}", và selectedOption=<số>.`
            );
        }

        // ──────── ACTION: SEND_SELECTED ────────
        if (parsed.action === "send_selected") {
            const draftId = parsed.draftId;

            if (!draftId) {
                return "[SMART_REPLY ERROR] Thiếu draftId. Cần gọi action='draft' trước để nhận draftId.";
            }

            const draft = draftCache.get(draftId);
            if (!draft) {
                return `[SMART_REPLY ERROR] Không tìm thấy draft "${draftId}". Draft có thể đã hết hạn (TTL: 1 giờ). Hãy gọi lại action='draft' để soạn mới.`;
            }

            const selectedOption = parsed.selectedOption;
            if (!selectedOption || selectedOption < 1 || selectedOption > 3) {
                return "[SMART_REPLY ERROR] Thiếu hoặc sai selectedOption. Giá trị hợp lệ: 1, 2, hoặc 3.";
            }

            // Lấy nội dung: ưu tiên customEdit nếu có
            const replyText = parsed.customEdit?.trim() || draft.options[selectedOption - 1];
            const senderName = draft.sender;
            const context = draft.context;

            logger.info(
                `[SmartReplyDrafter] Người dùng chọn Option ${selectedOption} cho draft ${draftId}. ` +
                `Gửi qua ${context} đến "${senderName}".`
            );

            // Xoá draft đã sử dụng
            draftCache.delete(draftId);

            const sendInstruction = getSendInstruction(context, replyText, senderName);

            return (
                `[SMART_REPLY SUCCESS] Đã chọn phương án ${selectedOption}.\n\n` +
                `📤 Nội dung gửi: "${replyText}"\n` +
                `👤 Người nhận: ${senderName}\n` +
                `📱 Nền tảng: ${context}\n\n` +
                sendInstruction
            );
        }

        return "[SMART_REPLY ERROR] Action không hợp lệ. Chỉ hỗ trợ 'draft' hoặc 'send_selected'.";
    } catch (error: unknown) {
        const errMsg = error instanceof Error ? error.message : String(error);
        logger.error(`[SmartReplyDrafter] Lỗi: ${errMsg}`);

        if (error instanceof z.ZodError) {
            return `[SMART_REPLY ERROR] Sai định dạng tham số: ${error.issues.map((e) => e.message).join(", ")}`;
        }
        return `[SMART_REPLY ERROR] Lỗi hệ thống: ${errMsg}`;
    }
};
