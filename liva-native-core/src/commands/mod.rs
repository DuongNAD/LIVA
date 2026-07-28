//! Tách `handle_command` theo MIỀN.
//!
//! ## Vì sao
//!
//! `handle_command` là một `match` duy nhất dài **1 475 dòng / 51 nhánh** (đo
//! 26/07/2026; đầu tháng là 1 384/29 — nó đang lớn nhanh hơn tốc độ đọc được).
//! Ba hệ quả đo được, không phải cảm tính:
//!
//! - **Mọi tính năng đều sửa cùng một file.** `lib.rs`, `main.rs`, `db.rs` là ba
//!   file mã bị sửa nhiều nhất trong 100 commit gần nhất — thay đổi dồn đúng chỗ
//!   test mỏng nhất.
//! - **Không đặt được allow-list lệnh theo kênh** (đề xuất (3) của §C1): với một
//!   `match` phẳng, "lệnh này chỉ được gọi từ kênh kia" không có chỗ để viết.
//! - **Không có tracing span theo miền**, nên không đo được lệnh nào chậm.
//!
//! ## Hợp đồng
//!
//! Mỗi module miền phơi ra đúng một hàm:
//!
//! ```ignore
//! pub async fn handle(state: Arc<AppState>, verb: &str, payload: Value, …)
//!     -> Result<Value, String>
//! ```
//!
//! `verb` là phần SAU dấu hai chấm (`vision:capture` → `"capture"`), nên module
//! không lặp lại tiền tố của chính nó. Dispatcher ở `handle_command` tách tiền
//! tố **trước** `match`, nên thêm một lệnh mới vào miền đã tách chỉ đụng đúng
//! file của miền đó.
//!
//! Chữ ký của `handle_command` **không đổi** — đây là điều kiện để việc tách
//! không lan ra ngoài: mọi caller (`main.rs`, `websocket.rs`, vỏ Tauri,
//! `telegram.rs`, `verify_integrations`, cùng 34 điểm gọi trong 4 file test)
//! vẫn gọi y như cũ.
//!
//! ## Tách dần, không một cú
//!
//! Mỗi miền là một commit riêng. Lý do thực dụng: repo này thường xuyên có hai
//! phiên làm việc song song cùng sửa `lib.rs`; một cú dời 1 475 dòng sẽ xung đột
//! với bất kỳ nhánh lệnh nào vừa được thêm, và git không hợp nhất nổi "tôi dời
//! 51 nhánh đi" với "tôi thêm nhánh thứ 52 vào giữa".

pub mod config;
pub mod consent;
pub mod integrations;
pub mod llm;
pub mod memory;
pub mod messaging;
pub mod setup;
pub mod task;
pub mod vision;
pub mod voice;
