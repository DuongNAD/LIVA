use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::utils::command::BotCommands;
use tracing::{info, error, warn};
use crate::AppState;
use rusqlite::OptionalExtension;

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "LIVA Remote Control Commands:")]
pub enum TelegramCommand {
    #[command(description = "Start the bot and display Chat ID.")]
    Start,
    #[command(description = "Display help menu.")]
    Help,
    #[command(description = "Show system status.")]
    Status,
    #[command(description = "🔴 Panic! Kill active environments and processes.")]
    Panic,
    #[command(description = "Ask LIVA a question. Example: /ask hello")]
    Ask(String),
    #[command(description = "Fetch latest agent response from short-term memory.")]
    Latest,
    #[command(description = "Barge-in and stop current response generation.")]
    Stop,
    #[command(description = "Explore directory. Example: /ls src")]
    Ls(String),
    #[command(description = "View file contents. Example: /cat Cargo.toml")]
    Cat(String),
}

pub struct TelegramBotManager {
    bot: Bot,
    allowed_ids: std::collections::HashSet<String>,
    state: Arc<AppState>,
    ipc_tx: Option<tokio::sync::mpsc::Sender<String>>,
}

impl TelegramBotManager {
    pub fn new(
        token: String,
        allowed_ids: std::collections::HashSet<String>,
        state: Arc<AppState>,
        ipc_tx: Option<tokio::sync::mpsc::Sender<String>>,
    ) -> Self {
        let bot = Bot::new(token);
        Self {
            bot,
            allowed_ids,
            state,
            ipc_tx,
        }
    }

    pub async fn start(self: Arc<Self>) {
        info!("📡 Starting Rust Telegram Bot Service (Teloxide)...");
        
        let manager = Arc::clone(&self);
        let handler = dptree::entry()
            .branch(
                Update::filter_message()
                    .branch(dptree::entry().filter_command::<TelegramCommand>().endpoint(handle_command))
                    .branch(dptree::endpoint(handle_message))
            );

        Dispatcher::builder(self.bot.clone(), handler)
            .dependencies(dptree::deps![manager])
            .enable_ctrlc_handler()
            .build()
            .dispatch()
            .await;
    }

    fn is_authorized(&self, user_id: &str) -> bool {
        if self.allowed_ids.is_empty() {
            return false;
        }
        self.allowed_ids.contains(user_id)
    }
}

// Handler for textual commands
async fn handle_command(
    bot: Bot,
    msg: Message,
    cmd: TelegramCommand,
    manager: Arc<TelegramBotManager>,
) -> ResponseResult<()> {
    let user_id = msg.from.as_ref().map(|u| u.id.to_string()).unwrap_or_default();
    if !manager.is_authorized(&user_id) {
        bot.send_message(msg.chat.id, "⛔ Bạn không có quyền sử dụng bot này.").await?;
        return Ok(());
    }

    match cmd {
        TelegramCommand::Start => {
            bot.send_message(msg.chat.id, format!("👋 Xin chào! Tôi là LIVA Native Control Hub.\nChat ID của bạn: `{}`", msg.chat.id))
                .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                .await?;
        }
        TelegramCommand::Help => {
            let help_text = "\
🤖 *LIVA Native Control Hub*
/start \\- Xem Chat ID
/help \\- Liệt kê lệnh
/status \\- Xem trạng thái hệ thống
/panic \\- 🔴 Dừng khẩn cấp toàn bộ tiến trình
/ask <query> \\- Gửi câu hỏi tới Agent
/latest \\- Đọc phản hồi mới nhất
/stop \\- Ngắt luồng AI hiện tại
/ls <path> \\- Liệt kê thư mục
/cat <file> \\- Đọc tệp tin";
            bot.send_message(msg.chat.id, help_text)
                .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                .await?;
        }
        TelegramCommand::Status => {
            bot.send_message(msg.chat.id, "🟢 Hệ thống LIVA Native Engine đang hoạt động bình thường.").await?;
        }
        TelegramCommand::Panic => {
            warn!("🔴 PANIC command triggered from Telegram!");
            if let Some(ref tx) = manager.ipc_tx {
                let event = serde_json::json!({
                    "id": format!("tg_panic_{}", msg.id),
                    "command": "panic",
                    "payload": {}
                }).to_string();
                let _ = tx.send(event).await;
            }
            bot.send_message(msg.chat.id, "🔴 PANIC: Yêu cầu dừng khẩn cấp đã được gửi.").await?;
        }
        TelegramCommand::Ask(query) => {
            if query.trim().is_empty() {
                bot.send_message(msg.chat.id, "❌ Vui lòng nhập câu hỏi sau lệnh `/ask`. Ví dụ: `/ask kiểm tra thời tiết`").await?;
                return Ok(());
            }
            route_input_to_agent(&manager, msg.chat.id, query).await;
        }
        TelegramCommand::Latest => {
            let state = manager.state.clone();
            let chat_id = msg.chat.id;
            let bot_clone = bot.clone();
            tokio::task::spawn_blocking(move || {
                match state.db.readers.get() {
                    Ok(conn) => {
                        let mut stmt = conn.prepare("SELECT aiReply FROM turn_layer_nodes ORDER BY temporal_anchor DESC LIMIT 1").unwrap();
                        let latest_reply: Option<String> = stmt.query_row([], |row| row.get(0)).optional().unwrap_or(None);
                        
                        tokio::spawn(async move {
                            if let Some(reply) = latest_reply {
                                let _ = bot_clone.send_message(chat_id, format!("🤖 LIVA phản hồi mới nhất:\n\n{}", reply))
                                    .await;
                            } else {
                                let _ = bot_clone.send_message(chat_id, "💬 LIVA chưa có phản hồi nào trong phiên này.").await;
                            }
                        });
                    }
                    Err(e) => {
                        error!("Failed to fetch database reader connection: {}", e);
                    }
                }
            });
        }
        TelegramCommand::Stop => {
            manager.state.tts_player.stop().await;
            if let Some(ref tx) = manager.ipc_tx {
                let event = serde_json::json!({
                    "id": format!("tg_stop_{}", msg.id),
                    "command": "voice:tts_stop",
                    "payload": {}
                }).to_string();
                let _ = tx.send(event).await;
            }
            bot.send_message(msg.chat.id, "🛑 Đã gửi lệnh dừng tiến trình AI hiện tại.").await?;
        }
        TelegramCommand::Ls(path) => {
            let target = if path.trim().is_empty() { "." } else { path.trim() };
            match tokio::fs::read_dir(target).await {
                Ok(mut entries) => {
                    let mut result = format!("📁 *Thư mục:* `{}`\n\n", target);
                    while let Ok(Some(entry)) = entries.next_entry().await {
                        let name = entry.file_name().to_string_lossy().into_owned();
                        let is_dir = entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false);
                        let icon = if is_dir { "📁" } else { "📄" };
                        result.push_str(&format!("{} `{}`\n", icon, name));
                    }
                    // Escape basic markdown special characters for safety if using MarkdownV2,
                    // or just avoid MarkdownV2. Let's escape only what is required.
                    // Instead of parsing MarkdownV2, we can just send as plain text or simple markdown
                    let result_escaped = result
                        .replace("_", "\\_")
                        .replace("*", "\\*")
                        .replace("[", "\\[")
                        .replace("]", "\\]")
                        .replace("(", "\\(")
                        .replace(")", "\\)")
                        .replace("~", "\\~")
                        .replace("`", "\\`")
                        .replace(">", "\\>")
                        .replace("#", "\\#")
                        .replace("+", "\\+")
                        .replace("-", "\\-")
                        .replace("=", "\\=")
                        .replace("|", "\\|")
                        .replace("{", "\\{")
                        .replace("}", "\\}")
                        .replace(".", "\\.")
                        .replace("!", "\\!");
                    
                    bot.send_message(msg.chat.id, result_escaped)
                        .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                        .await?;
                }
                Err(e) => {
                    bot.send_message(msg.chat.id, format!("❌ Lỗi đọc thư mục: {}", e)).await?;
                }
            }
        }
        TelegramCommand::Cat(file_path) => {
            if file_path.trim().is_empty() {
                bot.send_message(msg.chat.id, "❌ Cung cấp đường dẫn tệp tin. Ví dụ: `/cat Cargo.toml`").await?;
                return Ok(());
            }
            match tokio::fs::read_to_string(file_path.trim()).await {
                Ok(content) => {
                    let max_len = 3500;
                    let display_content = if content.len() > max_len {
                        format!("{}\n... (bị cắt bọt)", &content[..max_len])
                    } else {
                        content
                    };
                    
                    let header = format!("📄 *{}*\n\n", file_path);
                    let display_content_escaped = display_content
                        .replace("\\", "\\\\")
                        .replace("`", "\\`");
                    
                    let mut result_escaped = header
                        .replace("_", "\\_")
                        .replace("*", "\\*")
                        .replace("[", "\\[")
                        .replace("]", "\\]")
                        .replace("(", "\\(")
                        .replace(")", "\\)")
                        .replace("~", "\\~")
                        .replace(">", "\\>")
                        .replace("#", "\\#")
                        .replace("+", "\\+")
                        .replace("-", "\\-")
                        .replace("=", "\\=")
                        .replace("|", "\\|")
                        .replace("{", "\\{")
                        .replace("}", "\\}")
                        .replace(".", "\\.")
                        .replace("!", "\\!");
                    
                    result_escaped.push_str("```\n");
                    result_escaped.push_str(&display_content_escaped);
                    result_escaped.push_str("\n```");

                    bot.send_message(msg.chat.id, result_escaped)
                        .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                        .await?;
                }
                Err(e) => {
                    bot.send_message(msg.chat.id, format!("❌ Lỗi đọc tệp: {}", e)).await?;
                }
            }
        }
    }
    Ok(())
}

// Handler for raw messages (plain text or voice messages)
async fn handle_message(
    bot: Bot,
    msg: Message,
    manager: Arc<TelegramBotManager>,
) -> ResponseResult<()> {
    let user_id = msg.from.as_ref().map(|u| u.id.to_string()).unwrap_or_default();
    if !manager.is_authorized(&user_id) {
        return Ok(());
    }

    if let Some(text) = msg.text() {
        if text.starts_with('/') {
            return Ok(());
        }
        info!("💬 [Telegram] Received text message: {}", text);
        route_input_to_agent(&manager, msg.chat.id, text.to_string()).await;
    } else if let Some(voice) = msg.voice() {
        info!("🗣️ [Telegram] Received voice message!");
        bot.send_chat_action(msg.chat.id, teloxide::types::ChatAction::RecordVoice).await?;
        
        let manager_clone = manager.clone();
        let bot_clone = bot.clone();
        let voice_file_id = voice.file.id.clone();
        let chat_id = msg.chat.id;

        tokio::spawn(async move {
            match process_voice_message(&bot_clone, &voice_file_id, &manager_clone.state).await {
                Ok(transcription) => {
                    let _ = bot_clone.send_message(chat_id, format!("🗣️ Bạn nói: {}", transcription)).await;
                    route_input_to_agent(&manager_clone, chat_id, transcription).await;
                }
                Err(e) => {
                    error!("Failed to process voice message: {}", e);
                    let _ = bot_clone.send_message(chat_id, "⚠️ Đã xảy ra lỗi khi xử lý tin nhắn thoại.").await;
                }
            }
        });
    }

    Ok(())
}

// Download and transcribe voice messages using the local STT model
async fn process_voice_message(
    bot: &Bot,
    file_id: &str,
    state: &Arc<AppState>,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let file = bot.get_file(file_id).await?;
    let token = std::env::var("TELEGRAM_BOT_TOKEN").unwrap_or_default();
    let file_url = format!("https://api.telegram.org/file/bot{}/{}", token, file.path);

    let response = reqwest::get(&file_url).await?;
    let audio_bytes = response.bytes().await?;

    let temp_input_path = std::env::temp_dir().join(format!("tg_voice_{}.ogg", file_id));
    let temp_output_path = std::env::temp_dir().join(format!("tg_voice_{}.raw", file_id));
    tokio::fs::write(&temp_input_path, audio_bytes).await?;

    let status = tokio::process::Command::new("ffmpeg")
        .args(&[
            "-y",
            "-i",
            temp_input_path.to_str().unwrap(),
            "-ar",
            "16000",
            "-ac",
            "1",
            "-f",
            "f32le",
            temp_output_path.to_str().unwrap(),
        ])
        .status()
        .await?;

    if !status.success() {
        return Err("ffmpeg decoding failed".into());
    }

    let raw_bytes = tokio::fs::read(&temp_output_path).await?;
    let samples: Vec<f32> = raw_bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();

    let _ = tokio::fs::remove_file(temp_input_path).await;
    let _ = tokio::fs::remove_file(temp_output_path).await;

    let text = tokio::task::spawn_blocking({
        let state_clone = Arc::clone(state);
        move || {
            let mut stt = state_clone.stt.blocking_lock();
            stt.reset_stream();
            stt.feed_audio(&samples, true)
        }
    })
    .await??;

    text.ok_or_else(|| "ASR output was empty".into())
}

// Forward the input text to the Agent Loop
/// Giới hạn độ dài một tin nhắn Telegram (API từ chối > 4096 ký tự).
const TELEGRAM_MAX_MESSAGE: usize = 4000;

/// Đưa câu của người dùng vào agent và **gửi câu trả lời ngược lại Telegram**.
///
/// Trước đây hàm này chỉ đẩy một chuỗi JSON vào `ipc_tx` — tức là ra **stdout**.
/// Không có ai tiêu thụ stdout như một lệnh, nên `/ask` và mọi tin nhắn thường
/// đều rơi vào hư vô: người dùng gửi câu hỏi và không bao giờ nhận được gì.
///
/// Vẫn giữ phần phát ra `ipc_tx` để kênh IPC cũ (dùng cho tooling/regression)
/// không mất sự kiện, nhưng vòng lặp hội thoại giờ khép kín ngay tại đây.
async fn route_input_to_agent(
    manager: &TelegramBotManager,
    chat_id: ChatId,
    text: String,
) {
    let chat_id_str = chat_id.to_string();

    // Kênh IPC cũ: giữ nguyên hợp đồng sự kiện cho tooling bên ngoài.
    if let Some(ref tx) = manager.ipc_tx {
        let event = serde_json::json!({
            "id": format!("tg_msg_{}", chat_id_str),
            "command": "telegram:message",
            "payload": {
                "senderId": chat_id_str,
                "text": text
            }
        }).to_string();
        let _ = tx.send(event).await;
    }

    // Sinh câu trả lời có thể mất vài giây; báo cho người dùng biết máy đang chạy.
    let _ = manager
        .bot
        .send_chat_action(chat_id, teloxide::types::ChatAction::Typing)
        .await;

    let payload = serde_json::json!({
        "messages": [{ "role": "user", "content": text }],
        "stream": false
    });

    // Không stream: Telegram không hiển thị token dần, gửi một tin trọn vẹn
    // vẫn là trải nghiệm tốt hơn là spam nhiều tin nhắn nhỏ.
    let reply = match crate::handle_command(
        Arc::clone(&manager.state),
        "chat:completion",
        payload,
        None,
        None,
    )
    .await
    {
        Ok(v) => v
            .get("text")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .trim()
            .to_string(),
        Err(e) => {
            error!("[Telegram] chat:completion that bai: {}", e);
            let _ = manager
                .bot
                .send_message(chat_id, format!("⚠️ LIVA chưa trả lời được: {}", e))
                .await;
            return;
        }
    };

    if reply.is_empty() {
        let _ = manager
            .bot
            .send_message(chat_id, "🤔 LIVA không sinh được nội dung nào cho câu này.")
            .await;
        return;
    }

    // Cắt theo ranh giới ký tự (không phải byte) để không vỡ chữ có dấu.
    for chunk in split_for_telegram(&reply) {
        if let Err(e) = manager.bot.send_message(chat_id, chunk).await {
            error!("[Telegram] gui tin nhan that bai: {}", e);
            break;
        }
    }
}

/// Chia câu trả lời dài thành nhiều tin nhắn hợp lệ với Telegram.
///
/// Cắt theo **ký tự** chứ không theo byte: tiếng Việt là đa byte trong UTF-8,
/// cắt theo byte sẽ tạo ký tự vỡ.
fn split_for_telegram(text: &str) -> Vec<String> {
    if text.chars().count() <= TELEGRAM_MAX_MESSAGE {
        return vec![text.to_string()];
    }
    let mut out = Vec::new();
    let mut current = String::new();
    let mut count = 0usize;
    for ch in text.chars() {
        current.push(ch);
        count += 1;
        if count >= TELEGRAM_MAX_MESSAGE {
            out.push(std::mem::take(&mut current));
            count = 0;
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

#[cfg(test)]
mod telegram_tests {
    use super::{split_for_telegram, TELEGRAM_MAX_MESSAGE};

    #[test]
    fn khong_cat_khi_du_ngan() {
        let s = "xin chào";
        assert_eq!(split_for_telegram(s), vec![s.to_string()]);
    }

    #[test]
    fn cat_dung_o_nguong() {
        let s: String = "a".repeat(TELEGRAM_MAX_MESSAGE);
        assert_eq!(split_for_telegram(&s).len(), 1, "dung bang nguong thi khong cat");

        let s2: String = "a".repeat(TELEGRAM_MAX_MESSAGE + 1);
        let parts = split_for_telegram(&s2);
        assert_eq!(parts.len(), 2, "vuot 1 ky tu thi thanh 2 tin");
        assert_eq!(parts[0].chars().count(), TELEGRAM_MAX_MESSAGE);
        assert_eq!(parts[1].chars().count(), 1);
    }

    /// Tiếng Việt là đa byte trong UTF-8. Cắt theo BYTE sẽ tạo ký tự vỡ;
    /// test này khoá lại việc phải cắt theo KÝ TỰ.
    #[test]
    fn cat_tieng_viet_khong_vo_ky_tu() {
        // mỗi "ữ" là 3 byte -> chuỗi này dài gấp 3 nếu tính theo byte
        let s: String = "ữ".repeat(TELEGRAM_MAX_MESSAGE + 500);
        let parts = split_for_telegram(&s);

        // Ghép lại phải bằng đúng chuỗi gốc, không mất và không vỡ ký tự nào
        let joined: String = parts.concat();
        assert_eq!(joined, s, "ghep lai phai bang chuoi goc");

        for p in &parts {
            assert!(p.chars().count() <= TELEGRAM_MAX_MESSAGE, "moi phan phai lot gioi han");
            assert!(p.chars().all(|c| c == 'ữ'), "khong duoc co ky tu vo");
        }
    }

    #[test]
    fn chuoi_rong() {
        assert_eq!(split_for_telegram(""), vec!["".to_string()]);
    }
}
