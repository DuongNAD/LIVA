import { Context } from "telegraf";
import { logger } from "../utils/logger";
import { CDPBridge } from "../bridges/CDPBridge";
import { FileExplorer } from "../services/FileExplorer";
import { HierarchicalGraphRAG } from "../evolution/HierarchicalGraphRAG";
import { GitNexusIndexer } from "../evolution/GitNexusIndexer";

export class TelegramCommandHandler {
    constructor() {}

    private fileExplorer = new FileExplorer();
    private graphRag = new HierarchicalGraphRAG();
    private gitIndexer = new GitNexusIndexer();

    public registerHandlers(bot: any, cdpBridge: CDPBridge, autoAcceptDaemon: any) {
        bot.command("start", this.handleStart.bind(this));
        bot.command("help", this.handleHelp.bind(this));
        bot.command("status", this.handleStatus.bind(this));
        bot.command("panic", (ctx: Context) => this.handlePanic(ctx, cdpBridge, autoAcceptDaemon));
        
        // Explorer
        bot.command("ls", this.handleLs.bind(this));
        bot.command("cat", this.handleCat.bind(this));
        bot.action(/^ls:(.*)$/, this.handleLsCallback.bind(this));

        // Graph RAG
        bot.command("graph_index", this.handleGraphIndex.bind(this));
        bot.command("graph_s1", this.handleGraphS1.bind(this));
        bot.command("graph_s2", this.handleGraphS2.bind(this));
    }

    private async handleStart(ctx: Context) {
        await ctx.reply(`👋 Xin chào! Tôi là LIVA Remote Control Hub.\nChat ID của bạn: \`${ctx.chat?.id}\``, { parse_mode: "Markdown" });
    }

    private async handleHelp(ctx: Context) {
        const helpText = `
🤖 *LIVA Remote Control Hub*

/start - Xem Chat ID và Welcome
/help - Liệt kê lệnh
/status - Xem trạng thái hệ thống
/panic - 🔴 Dừng khẩn cấp toàn bộ IDE & Auto-Accept
/ask <query> - Gửi lệnh tới Agent
/latest - Đọc response mới nhất
/stop - Dừng tiến trình hiện tại

📁 *Explorer*
/ls [path] - Liệt kê thư mục (có giao diện nút bấm)
/cat <file> - Đọc nội dung file

🌳 *Graph RAG*
/graph_index - Xây dựng AST Hierarchical Graph
/graph_s1 <keyword> - System 1 (Tìm kiếm File/Class)
/graph_s2 <func> - System 2 (Truy vết Call Graph)
        `;
        await ctx.reply(helpText, { parse_mode: "Markdown" });
    }

    private async handleStatus(ctx: Context) {
        await ctx.reply("🟢 Hệ thống LIVA đang hoạt động bình thường.");
    }

    private async handlePanic(ctx: Context, cdpBridge: CDPBridge, autoAcceptDaemon: any) {
        logger.warn("[TG] 🔴 PANIC triggered!");
        // 1. Tắt Auto-Accept ngay (nếu có)
        if (autoAcceptDaemon) {
            await autoAcceptDaemon.disable(cdpBridge);
        }
        
        // 2. Kill IDE process qua CDP
        try {
            await cdpBridge.send("Browser.close");
        } catch (e) {
            logger.error(`[TG] Panic close failed: ${e}`);
            // Fallback: exec
            import("child_process").then(({ exec }) => {
                exec(`taskkill /F /IM Antigravity.exe`);
            });
        }
        await ctx.reply("🔴 PANIC: IDE đã bị đóng. Auto-Accept đã tắt.");
    }

    private async renderLs(dirPath: string): Promise<{ text: string, markup: any }> {
        const targetPath = dirPath || "/";
        try {
            const files = await this.fileExplorer.listDirectory(dirPath);
            
            let text = `📁 *Explorer:* \`${targetPath}\`\n\n`;
            if (files.length === 0) text += "_Thư mục trống_";
            
            const keyboard = [];
            
            // Add "Up" directory button if not root
            if (targetPath !== "/" && targetPath !== "") {
                const parts = targetPath.split("/");
                parts.pop();
                const parent = parts.join("/") || "/";
                keyboard.push([{ text: "🔙 Lên 1 cấp", callback_data: `ls:${parent}` }]);
            }

            for (const f of files) {
                const icon = f.isDirectory ? "📁" : "📄";
                const sizeStr = f.isDirectory ? "" : `(${(f.size/1024).toFixed(1)}kb)`;
                const itemPath = targetPath === "/" ? f.name : `${targetPath}/${f.name}`;
                
                if (f.isDirectory) {
                    keyboard.push([{ text: `${icon} ${f.name} ${sizeStr}`, callback_data: `ls:${itemPath}` }]);
                } else {
                    // Files can't be clicked for ls, maybe provide a cat command copy
                    text += `${icon} \`${f.name}\` ${sizeStr}\n`;
                }
            }

            return {
                text,
                markup: { inline_keyboard: keyboard }
            };
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            return { text: `❌ Lỗi truy cập \`${targetPath}\`: ${errMsg}`, markup: null };
        }
    }

    private async handleLs(ctx: Context) {
        // @ts-expect-error - context typing
        const args = ctx.message?.text.split(" ").slice(1).join(" ");
        const { text, markup } = await this.renderLs(args);
        await ctx.reply(text, { parse_mode: "Markdown", reply_markup: markup });
    }

    private async handleLsCallback(ctx: Context) {
        // @ts-expect-error - context typing
        const match = ctx.match;
        const dirPath = match[1];
        
        const { text, markup } = await this.renderLs(dirPath);
        
        try {
            await ctx.editMessageText(text, { parse_mode: "Markdown", reply_markup: markup });
            await ctx.answerCbQuery();
        } catch (e) {
            await ctx.answerCbQuery("Lỗi tải thư mục").catch(() => {});
        }
    }

    private async handleCat(ctx: Context) {
        // @ts-expect-error - context typing
        const args = ctx.message?.text.split(" ").slice(1).join(" ");
        if (!args) {
            return ctx.reply("❌ Cần cung cấp đường dẫn tệp. Ví dụ: `/cat src/main.ts`", { parse_mode: "Markdown" });
        }

        try {
            const content = await this.fileExplorer.readFile(args);
            // Telegram limits to 4096 chars per message
            const MAX_LEN = 3500;
            const isTruncated = content.length > MAX_LEN;
            const displayContent = isTruncated ? content.substring(0, MAX_LEN) + "\n... (bị cắt gọt)" : content;
            
            await ctx.reply(`📄 *${args}*\n\n\`\`\`\n${displayContent}\n\`\`\``, { parse_mode: "Markdown" });
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            await ctx.reply(`❌ Lỗi đọc tệp: ${errMsg}`);
        }
    }

    private async handleGraphIndex(ctx: Context) {
        await ctx.reply("🌳 Đang khởi chạy AST Graph Builder & GitNexus Analyzer. Tiến trình chạy ngầm...");
        this.gitIndexer.triggerIndex(0); // Chạy ngay lập tức
    }

    private async handleGraphS1(ctx: Context) {
        // @ts-expect-error - context typing
        const args = ctx.message?.text.split(" ").slice(1).join(" ");
        if (!args) return ctx.reply("❌ Cần cung cấp từ khóa. VD: `/graph_s1 Telegram`", { parse_mode: "Markdown" });

        await ctx.reply(`🔍 **System 1:** Tìm kiếm \`${args}\`...`, { parse_mode: "Markdown" });
        try {
            const results = await this.graphRag.system1Search(args);
            if (results.length === 0) {
                return ctx.reply("Không tìm thấy kết quả nào trong Graph.");
            }
            
            let text = `🌳 *Kết quả System 1 (${results.length})*\n\n`;
            for (const r of results.slice(0, 15)) { // Limit 15
                text += `- [${r.type.toUpperCase()}] \`${r.name}\` (${r.filePath || ""})\n`;
            }
            if (results.length > 15) text += `\n_... và ${results.length - 15} kết quả khác._`;
            
            await ctx.reply(text, { parse_mode: "Markdown" });
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            await ctx.reply(`❌ Lỗi: ${errMsg}`);
        }
    }

    private async handleGraphS2(ctx: Context) {
        // @ts-expect-error - context typing
        const args = ctx.message?.text.split(" ").slice(1).join(" ");
        if (!args) return ctx.reply("❌ Cần cung cấp tên hàm/class. VD: `/graph_s2 sendMessage`", { parse_mode: "Markdown" });

        await ctx.reply(`🕵️ **System 2:** Đang truy vết Call Graph cho \`${args}\`...`, { parse_mode: "Markdown" });
        try {
            const results = await this.graphRag.system2DeepDive(args, 1);
            if (results.length === 0) {
                return ctx.reply("Không tìm thấy hàm này hoặc hàm không gọi hàm con nào.");
            }

            const MAX_LEN = 3500;
            const jsonStr = JSON.stringify(results, null, 2);
            const isTruncated = jsonStr.length > MAX_LEN;
            const display = isTruncated ? jsonStr.substring(0, MAX_LEN) + "\n... (bị cắt gọt)" : jsonStr;

            await ctx.reply(`🌳 *Call Graph (Độ sâu: 1)*\n\n\`\`\`json\n${display}\n\`\`\``, { parse_mode: "Markdown" });
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            await ctx.reply(`❌ Lỗi: ${errMsg}`);
        }
    }
}
