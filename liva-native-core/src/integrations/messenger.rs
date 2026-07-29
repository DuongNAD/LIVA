//! Gửi tin Messenger bằng cách lái một trình duyệt đã đăng nhập, qua CDP.
//!
//! ## Vì sao phải lái trình duyệt
//!
//! Facebook **không có API cho tin nhắn cá nhân**. Messenger Platform API chỉ
//! cho Page trả lời người đã nhắn Page trước. Muốn nhắn cho bạn bè từ tài khoản
//! cá nhân thì chỉ còn đường điều khiển giao diện. Đây là ràng buộc của nền
//! tảng, không phải lựa chọn kiến trúc.
//!
//! Kèm theo đó là hai sự thật phải nói thẳng: Meta **cấm tự động hoá** trong
//! điều khoản, và rủi ro khoá tài khoản là thật. Module này tồn tại vì người
//! dùng đã được báo và vẫn chọn làm.
//!
//! ## LIVA không bao giờ chạm vào mật khẩu
//!
//! Không có một dòng nào ở đây nhập mật khẩu, và sẽ không có. Cách làm là:
//! người dùng mở Chrome với một `--user-data-dir` riêng, **tự tay đăng nhập một
//! lần**, rồi phiên đăng nhập nằm trong cookie của profile đó. LIVA chỉ gắn vào
//! trình duyệt đang chạy.
//!
//! Không phải chỉ vì an toàn — nó còn đúng hơn về kỹ thuật: đăng nhập tự động
//! kích hoạt checkpoint (2FA, xác minh thiết bị) gần như chắc chắn, còn cookie
//! sẵn thì không.
//!
//! Từ Chrome 136, `--remote-debugging-port` **bị từ chối trên profile mặc
//! định**; bắt buộc có `--user-data-dir` riêng. Nên "gắn vào Chrome bạn đang
//! dùng" là không làm được, không phải do lười.
//!
//! ## Env
//!
//! - `LIVA_MESSENGER_CDP_PORT` — cổng debug, mặc định 9222.
//! - `LIVA_MESSENGER_TIMEOUT_MS` — hạn chờ mỗi bước, mặc định 15000.
//!
//! ## Mức độ đã kiểm chứng
//!
//! Tầng vận chuyển CDP (dò target, JSON-RPC qua WebSocket) có test. Chuỗi thao
//! tác **trên DOM của messenger.com thì CHƯA** — không kiểm được nếu không có
//! một tài khoản đã đăng nhập thật. Vì vậy [`status`] tồn tại: nó nói rõ đang
//! hỏng ở chặng nào thay vì để [`send`] thất bại mù.

use serde_json::{Value, json};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};

fn cong() -> u16 {
    std::env::var("LIVA_MESSENGER_CDP_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(9222)
}

fn han_cho() -> Duration {
    let ms = std::env::var("LIVA_MESSENGER_TIMEOUT_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(15_000);
    Duration::from_millis(ms)
}

/// Câu hướng dẫn dựng lại từ cấu hình hiện tại, để thông điệp lỗi nói được
/// **chính xác** phải gõ gì — chứ không phải "hãy bật debug port".
fn cach_mo_trinh_duyet() -> String {
    format!(
        "Mở Chrome cho LIVA bằng lệnh này (profile RIÊNG, đăng nhập một lần bằng tay):\n\
         chrome.exe --remote-debugging-port={} --user-data-dir=\"%LOCALAPPDATA%\\liva-messenger-profile\" https://www.messenger.com",
        cong()
    )
}

/// Một phiên CDP đã nối tới **một tab**.
struct Phien {
    ws: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    id_ke_tiep: u64,
}

impl Phien {
    async fn noi(ws_url: &str) -> Result<Self, String> {
        let (ws, _) = tokio::time::timeout(han_cho(), tokio_tungstenite::connect_async(ws_url))
            .await
            .map_err(|_| "Hết hạn chờ khi nối WebSocket tới trình duyệt".to_string())?
            .map_err(|e| format!("Không nối được CDP: {e}"))?;
        Ok(Self { ws, id_ke_tiep: 1 })
    }

    /// Gọi một phương thức CDP và chờ đúng phản hồi của nó.
    ///
    /// CDP trộn **sự kiện** (không có `id`) vào cùng dòng với phản hồi, nên
    /// không thể đọc một gói rồi coi đó là kết quả — phải bỏ qua tới khi gặp
    /// `id` mình vừa gửi.
    async fn goi(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.id_ke_tiep;
        self.id_ke_tiep += 1;

        let goi_tin = json!({ "id": id, "method": method, "params": params }).to_string();
        self.ws
            .send(tokio_tungstenite::tungstenite::Message::Text(goi_tin))
            .await
            .map_err(|e| format!("Không gửi được lệnh CDP '{method}': {e}"))?;

        let doc = async {
            while let Some(msg) = self.ws.next().await {
                let msg = msg.map_err(|e| format!("Lỗi đọc CDP: {e}"))?;
                let text = match msg {
                    tokio_tungstenite::tungstenite::Message::Text(t) => t,
                    _ => continue,
                };
                let v: Value = match serde_json::from_str(&text) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                if v.get("id").and_then(Value::as_u64) != Some(id) {
                    continue; // sự kiện, hoặc phản hồi của lệnh khác
                }
                if let Some(err) = v.get("error") {
                    return Err(format!("CDP '{method}' lỗi: {err}"));
                }
                return Ok(v.get("result").cloned().unwrap_or(Value::Null));
            }
            Err(format!("CDP đóng kết nối khi đang chờ '{method}'"))
        };

        tokio::time::timeout(han_cho(), doc)
            .await
            .map_err(|_| format!("Hết hạn chờ phản hồi CDP cho '{method}'"))?
    }

    /// Chạy JS trong tab, trả giá trị đã ép về kiểu nguyên thuỷ.
    async fn danh_gia(&mut self, js: &str) -> Result<Value, String> {
        let r = self
            .goi(
                "Runtime.evaluate",
                json!({ "expression": js, "returnByValue": true, "awaitPromise": true }),
            )
            .await?;
        if let Some(chi_tiet) = r.get("exceptionDetails") {
            return Err(format!("JS ném lỗi trong tab: {chi_tiet}"));
        }
        Ok(r.pointer("/result/value").cloned().unwrap_or(Value::Null))
    }
}

/// PID của tiến trình trình duyệt, hỏi qua CDP ở **tầng browser**.
///
/// `SystemInfo.getProcessInfo` không gọi được trên session của một tab; phải nối
/// vào `webSocketDebuggerUrl` lấy từ `/json/version`.
async fn pid_trinh_duyet() -> Result<u32, String> {
    let cong = cong();
    let body = tokio::time::timeout(
        han_cho(),
        reqwest::get(format!("http://127.0.0.1:{cong}/json/version")),
    )
    .await
    .map_err(|_| "Hết hạn chờ khi hỏi phiên bản trình duyệt".to_string())?
    .map_err(|e| format!("Không hỏi được trình duyệt: {e}"))?
    .text()
    .await
    .map_err(|e| format!("Không đọc được phản hồi phiên bản: {e}"))?;

    let v: Value = serde_json::from_str(&body)
        .map_err(|e| format!("Phản hồi /json/version không phải JSON: {e}"))?;
    let ws_url = v
        .get("webSocketDebuggerUrl")
        .and_then(Value::as_str)
        .ok_or("Trình duyệt không cấp webSocketDebuggerUrl ở tầng browser")?;

    let mut phien = Phien::noi(ws_url).await?;
    let r = phien.goi("SystemInfo.getProcessInfo", json!({})).await?;
    r.get("processInfo")
        .and_then(Value::as_array)
        .and_then(|arr| {
            arr.iter()
                .find(|p| p.get("type").and_then(Value::as_str) == Some("browser"))
                .and_then(|p| p.get("id").and_then(Value::as_u64))
        })
        .map(|id| id as u32)
        .ok_or_else(|| "Không tìm thấy PID của tiến trình trình duyệt".to_string())
}

/// Đưa cửa sổ trình duyệt lên **foreground của Windows**.
///
/// ## Vì sao bước này bắt buộc
///
/// Đo 27/07/2026, và đây là nút thắt của cả tính năng: `Input.dispatchKeyEvent`
/// được Chrome nhận, trả về không lỗi, rồi **im lặng vứt đi** khi cửa sổ không
/// phải foreground ở tầng hệ điều hành. Cài một bộ ghi `keydown` vào trang rồi
/// bắn phím thì trang không nghe thấy gì; đưa cửa sổ ra trước bằng
/// `SetForegroundWindow` rồi bắn lại thì sự kiện tới nơi và `isTrusted`.
///
/// `Page.bringToFront` **không đủ** — nó kích hoạt tab bên trong cửa sổ, không
/// động tới thứ tự cửa sổ của Windows. Cửa sổ vẫn hiện, không thu nhỏ
/// (`IsIconic` = false), vậy mà phím vẫn bị bỏ.
///
/// `Input.insertText` thì KHÔNG dính, vì nó đi đường commit của IME chứ không
/// phải đường phím. Chính sự lệch đó làm lỗi khó đọc: chữ vào được ô soạn nên
/// mọi thứ trông như đang chạy, chỉ mỗi Enter là không.
///
/// Cái giá: nó cướp foreground của bạn trong khoảnh khắc gửi. Không tránh được
/// khi lái một trình duyệt thật, và đây là lý do bước xác nhận đứng trước —
/// người dùng biết trước sắp có gì xảy ra.
#[cfg(windows)]
fn dua_cua_so_ra_truoc(pid: u32) -> Result<(), String> {
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowThreadProcessId, IsWindowVisible, SW_RESTORE, SetForegroundWindow,
        ShowWindow,
    };

    struct Tim {
        pid: u32,
        hwnd: HWND,
    }

    unsafe extern "system" fn tham(hwnd: HWND, lparam: LPARAM) -> i32 {
        // SAFETY: `lparam` là con trỏ tới `Tim` do lời gọi EnumWindows bên dưới
        // truyền vào, và nó sống hết lời gọi đó.
        let tim = unsafe { &mut *(lparam as *mut Tim) };
        let mut cua_pid = 0u32;
        // SAFETY: `hwnd` do Windows cấp, `cua_pid` là biến cục bộ hợp lệ.
        unsafe { GetWindowThreadProcessId(hwnd, &mut cua_pid) };
        // SAFETY: chỉ đọc thuộc tính của một handle hợp lệ.
        if cua_pid == pid_cua(tim) && unsafe { IsWindowVisible(hwnd) } != 0 {
            tim.hwnd = hwnd;
            return 0; // dừng duyệt
        }
        1
    }

    fn pid_cua(t: &Tim) -> u32 {
        t.pid
    }

    let mut tim = Tim { pid, hwnd: 0 };
    // SAFETY: callback và con trỏ dữ liệu đều hợp lệ trong suốt lời gọi.
    unsafe { EnumWindows(Some(tham), &mut tim as *mut Tim as LPARAM) };

    if tim.hwnd == 0 {
        return Err(format!(
            "Không tìm thấy cửa sổ nào của trình duyệt (PID {pid}). Cửa sổ có thể đã đóng."
        ));
    }
    // SAFETY: `tim.hwnd` là handle hợp lệ vừa tìm được.
    unsafe {
        ShowWindow(tim.hwnd, SW_RESTORE);
        SetForegroundWindow(tim.hwnd);
    }
    Ok(())
}

#[cfg(not(windows))]
fn dua_cua_so_ra_truoc(_pid: u32) -> Result<(), String> {
    Err("Chỉ hỗ trợ Windows: chưa có đường đưa cửa sổ ra foreground trên nền này".to_string())
}

/// Một tab đang mở, đọc từ `/json/list`.
#[derive(Debug, Clone)]
struct Tab {
    url: String,
    ws_url: String,
}

async fn liet_ke_tab() -> Result<Vec<Tab>, String> {
    let cong = cong();
    let body = tokio::time::timeout(
        han_cho(),
        reqwest::get(format!("http://127.0.0.1:{cong}/json/list")),
    )
    .await
    .map_err(|_| format!("Hết hạn chờ khi hỏi trình duyệt ở cổng {cong}"))?
    .map_err(|e| {
        format!(
            "Không thấy trình duyệt nào mở debug port ở 127.0.0.1:{cong} ({e}).\n{}",
            cach_mo_trinh_duyet()
        )
    })?
    .text()
    .await
    .map_err(|e| format!("Không đọc được danh sách tab: {e}"))?;

    let v: Value = serde_json::from_str(&body)
        .map_err(|e| format!("Danh sách tab không phải JSON hợp lệ: {e}"))?;

    Ok(v.as_array()
        .map(|arr| {
            arr.iter()
                .filter(|t| t.get("type").and_then(Value::as_str) == Some("page"))
                .filter_map(|t| {
                    Some(Tab {
                        url: t.get("url").and_then(Value::as_str)?.to_string(),
                        ws_url: t
                            .get("webSocketDebuggerUrl")
                            .and_then(Value::as_str)?
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default())
}

fn la_tab_messenger(url: &str) -> bool {
    url.contains("messenger.com") || url.contains("facebook.com/messages")
}

/// Tìm tab Messenger đang mở; nếu chưa có thì lấy tab bất kỳ để điều hướng.
async fn tim_tab() -> Result<Tab, String> {
    let tabs = liet_ke_tab().await?;
    if tabs.is_empty() {
        return Err(format!(
            "Trình duyệt có mở debug port nhưng không có tab nào.\n{}",
            cach_mo_trinh_duyet()
        ));
    }
    Ok(tabs
        .iter()
        .find(|t| la_tab_messenger(&t.url))
        .cloned()
        .unwrap_or_else(|| tabs[0].clone()))
}

/// JS kiểm tra trạng thái trang. Trả chuỗi để lớp Rust khỏi đoán qua URL —
/// messenger.com khi chưa đăng nhập vẫn giữ nguyên đường dẫn `/t/…`.
const JS_TRANG_THAI: &str = r#"
(() => {
  if (document.querySelector('input[name="pass"], #login_form, [data-testid="royal_login_form"]')) {
    return 'chua_dang_nhap';
  }
  const soan = document.querySelector('[contenteditable="true"][role="textbox"]');
  if (soan) return 'san_sang';
  if (document.readyState !== 'complete') return 'dang_tai';
  // Đã đăng nhập, trang xong, mà không có ô soạn. Hai nguyên nhân KHÁC HẲN nhau
  // và trước đây bị gộp làm một, nên lỗi đổ tội nhầm cho bộ chọn:
  //
  //  - Không có hội thoại nào mở  → handle sai / Messenger từ chối mở. Đo được
  //    26/07/2026 với `t/<id-của-chính-mình>`: Messenger KHÔNG cho tự nhắn mình
  //    qua URL đó, nó chỉ hiện danh sách chat.
  //  - Có hội thoại mở mà vẫn không thấy ô  → giao diện đã đổi thật.
  //
  // Phân biệt bằng vùng tin nhắn (`role="main"` + lưới hội thoại). Nếu nó vắng
  // thì chưa có hội thoại nào được chọn.
  const coHoiThoai = !!document.querySelector(
    '[role="main"] [role="grid"], [role="main"] [aria-label][role="log"]'
  );
  return coHoiThoai ? 'khong_thay_o_soan' : 'khong_mo_duoc_hoi_thoai';
})()
"#;

/// Chờ tới khi trang ở trạng thái gửi được, hoặc kết luận vì sao không.
async fn cho_san_sang(phien: &mut Phien) -> Result<(), String> {
    let han = tokio::time::Instant::now() + han_cho();
    let mut cuoi = String::from("dang_tai");
    while tokio::time::Instant::now() < han {
        let tt = phien.danh_gia(JS_TRANG_THAI).await?;
        cuoi = tt.as_str().unwrap_or("khong_ro").to_string();
        match cuoi.as_str() {
            "san_sang" => return Ok(()),
            "chua_dang_nhap" => {
                return Err(
                    "Profile Chrome này CHƯA đăng nhập Facebook. Hãy tự đăng nhập một lần \
                     trong cửa sổ đó rồi thử lại — LIVA không nhập mật khẩu hộ bạn."
                        .to_string(),
                );
            }
            _ => tokio::time::sleep(Duration::from_millis(400)).await,
        }
    }
    // Mỗi trạng thái cuối chỉ về một chỗ sửa khác nhau. Gộp chúng thành một câu
    // chung nghĩa là bắt người đọc đi mò — và câu gộp cũ chỉ nhắc tới bộ chọn,
    // tức nó chủ động chỉ SAI hướng khi nguyên nhân là handle.
    Err(match cuoi.as_str() {
        "khong_mo_duoc_hoi_thoai" => {
            "Đã đăng nhập nhưng không mở được hội thoại nào — gần như chắc chắn `handle` sai. \
             Lấy đúng phần sau `/t/` trong URL khi bạn mở cuộc trò chuyện đó bằng tay. \
             Lưu ý: id của CHÍNH BẠN không dùng được, Messenger không cho tự nhắn mình qua URL."
                .to_string()
        }
        "khong_thay_o_soan" => {
            "Hội thoại đã mở nhưng không thấy ô soạn — nhiều khả năng Messenger đã đổi giao diện. \
             Cập nhật bộ chọn trong `integrations/messenger.rs` (hằng `JS_TRANG_THAI`)."
                .to_string()
        }
        khac => format!("Trang không vào được trạng thái gửi được (trạng thái cuối: {khac})."),
    })
}

/// Báo cáo tiền kiểm: nói rõ hỏng ở chặng nào.
pub async fn status() -> Result<Value, String> {
    let tabs = match liet_ke_tab().await {
        Ok(t) => t,
        Err(e) => {
            return Ok(json!({
                "reachable": false,
                "detail": e,
                "howto": cach_mo_trinh_duyet(),
            }));
        }
    };

    let tab = match tabs.iter().find(|t| la_tab_messenger(&t.url)) {
        Some(t) => t.clone(),
        None => {
            return Ok(json!({
                "reachable": true,
                "messengerTab": false,
                "tabs": tabs.len(),
                "detail": "Trình duyệt nối được nhưng chưa có tab Messenger nào.",
                "howto": cach_mo_trinh_duyet(),
            }));
        }
    };

    let mut phien = Phien::noi(&tab.ws_url).await?;
    let tt = phien.danh_gia(JS_TRANG_THAI).await?;
    let tt = tt.as_str().unwrap_or("khong_ro");

    Ok(json!({
        "reachable": true,
        "messengerTab": true,
        "url": tab.url,
        "state": tt,
        "loggedIn": tt != "chua_dang_nhap",
        "canSend": tt == "san_sang",
    }))
}

/// Gửi `text` cho hội thoại `handle` (id số hoặc username trong URL).
///
/// **Chỉ được gọi từ `messaging::send`**, tức sau khi bản nháp đã qua xác nhận.
pub async fn send(handle: &str, text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("Nội dung rỗng, không gửi".to_string());
    }

    let tab = tim_tab().await?;
    let mut phien = Phien::noi(&tab.ws_url).await?;

    let dich = format!("https://www.messenger.com/t/{handle}");
    if tab.url != dich {
        phien.goi("Page.navigate", json!({ "url": dich })).await?;
    }
    cho_san_sang(&mut phien).await?;

    // Đặt con trỏ vào ô soạn, và DỌN chữ còn sót trước khi gõ.
    //
    // Dọn không phải cho gọn: một lần gửi hỏng trước đó để lại nguyên câu trong
    // ô soạn (đo được 27/07 — Enter không gửi, chữ nằm lại). Lần sau `insertText`
    // sẽ NỐI vào đuôi, thành một tin ghép hai câu mà người dùng chưa từng đọc
    // trên thẻ xác nhận. Với hành động không hoàn tác được thì đó là kiểu hỏng
    // tệ nhất: nội dung gửi đi khác nội dung đã duyệt.
    //
    // Dọn bằng chọn-tất-cả rồi `Input.insertText("")` chứ không gán `textContent`:
    // ô soạn của Messenger là trình soạn thảo có mô hình dữ liệu riêng, sửa DOM
    // thẳng tay thì trạng thái nội bộ của nó không đổi theo.
    let da_focus = phien
        .danh_gia(
            r#"(() => {
                 const o = document.querySelector('[contenteditable="true"][role="textbox"]');
                 if (!o) return false;
                 o.focus();
                 return document.activeElement === o;
               })()"#,
        )
        .await?;
    if da_focus.as_bool() != Some(true) {
        return Err("Không đặt được con trỏ vào ô soạn tin".to_string());
    }

    // ── Tự kiểm TRƯỚC KHI GÕ ─────────────────────────────────────────────────
    //
    // Đo 27/07/2026: `Input.insertText` chạy, nhưng `Input.dispatchKeyEvent`
    // được Chrome nhận rồi **lặng lẽ bỏ** — cài một bộ ghi sự kiện rồi bắn phím
    // qua CDP thì trang không nghe thấy gì, trong khi cùng bộ ghi đó bắt được
    // sự kiện do JS tự phát. Không phải cửa sổ bị thu nhỏ (đã kiểm `IsIconic`),
    // không sửa được bằng `Page.bringToFront`.
    //
    // Hệ quả nếu cứ gõ: chữ vào ô soạn, Enter không ăn, tin KHÔNG đi — và câu
    // đó **nằm lại**. Lần thử sau `insertText` nối tiếp vào đuôi, thành tin ghép
    // hai, ba câu. Đã xảy ra thật: ô soạn tích tụ ba bản của cùng một câu.
    //
    // Nên kiểm khả năng gửi TRƯỚC, và nếu không gửi được thì **không đụng vào ô
    // soạn**. Thà không làm gì còn hơn để lại một câu dở dang trong khung chat
    // của người khác.
    // Đưa cửa sổ ra foreground TRƯỚC khi tự kiểm — nếu không thì phép kiểm sẽ
    // luôn báo "phím không tới nơi", vì đúng là nó không tới.
    match pid_trinh_duyet().await {
        Ok(pid) => dua_cua_so_ra_truoc(pid)?,
        Err(e) => {
            return Err(format!(
                "Không xác định được cửa sổ trình duyệt để kích hoạt: {e}"
            ));
        }
    }
    tokio::time::sleep(Duration::from_millis(300)).await;

    let phim_toi_duoc = phien
        .danh_gia(
            r#"(() => { window.__livaProbe = 0;
                 window.__livaH = () => { window.__livaProbe++; };
                 document.addEventListener('keydown', window.__livaH, true);
                 return true; })()"#,
        )
        .await?;
    if phim_toi_duoc.as_bool() == Some(true) {
        phien
            .goi(
                "Input.dispatchKeyEvent",
                json!({ "type": "keyDown", "key": "F13", "code": "F13",
                        "windowsVirtualKeyCode": 124, "nativeVirtualKeyCode": 124 }),
            )
            .await?;
        tokio::time::sleep(Duration::from_millis(200)).await;
        let dem = phien
            .danh_gia(
                r#"(() => { const n = window.__livaProbe;
                     document.removeEventListener('keydown', window.__livaH, true);
                     return n; })()"#,
            )
            .await?;
        if dem.as_u64() == Some(0) {
            return Err(
                "Trình duyệt nhận lệnh phím nhưng không chuyển tới trang, nên không có cách nào \
                 bấm Enter để gửi. Chưa gõ gì vào ô soạn — khung chat của người nhận không bị \
                 đụng tới. Xem ghi chú trong `integrations/messenger.rs`; đường Telegram không \
                 dính vấn đề này."
                    .to_string(),
            );
        }
    }

    // Dọn chữ sót bằng `execCommand` chứ không phải Ctrl+A: phím không tới được
    // trang (xem trên), còn `Runtime.evaluate` thì tới. `execCommand` vẫn sinh
    // đúng chuỗi sự kiện `beforeinput`/`input` mà trình soạn thảo cần, khác với
    // việc gán thẳng `textContent` — cái đó để lại mô hình dữ liệu nội bộ của nó
    // không đổi theo.
    let con_sot = phien
        .danh_gia(
            r#"(() => {
                 const o = document.querySelector('[contenteditable="true"][role="textbox"]');
                 if (!o || !o.textContent.trim()) return '';
                 o.focus();
                 document.execCommand('selectAll', false, null);
                 document.execCommand('delete', false, null);
                 return o.textContent.trim();
               })()"#,
        )
        .await?;
    if con_sot.as_str().map(str::is_empty) == Some(false) {
        return Err(format!(
            "Ô soạn còn chữ cũ ({:?}) mà không xoá được, nên dừng lại — gõ tiếp sẽ tạo ra một tin \
             ghép mà bạn chưa từng duyệt. Xoá tay trong cửa sổ Chrome rồi thử lại.",
            con_sot.as_str().unwrap_or("")
        ));
    }

    // `Input.insertText` chứ không phải bắn từng phím: nó đi qua đúng đường IME
    // của trình duyệt nên tiếng Việt có dấu vào nguyên vẹn, và không có nhịp gõ
    // giả để phải bịa.
    phien
        .goi("Input.insertText", json!({ "text": text }))
        .await?;

    // Enter. Bản đầu chỉ bắn `keyDown`/`keyUp` **không kèm `text`**, và đo thật
    // 27/07 cho thấy chữ vào ô soạn nhưng tin KHÔNG đi: Chrome coi đó là phím
    // thô không sinh ký tự, nên trình soạn thảo của Messenger không thấy sự kiện
    // nó đang chờ. Bộ ba `rawKeyDown` + `char` + `keyUp` kèm `text: "\r"` là
    // dạng đầy đủ mà một lần gõ Enter thật sinh ra.
    for su_kien in [
        json!({ "type": "rawKeyDown", "key": "Enter", "code": "Enter",
                "windowsVirtualKeyCode": 13, "nativeVirtualKeyCode": 13,
                "text": "\r", "unmodifiedText": "\r" }),
        json!({ "type": "char", "key": "Enter", "text": "\r", "unmodifiedText": "\r" }),
        json!({ "type": "keyUp", "key": "Enter", "code": "Enter",
                "windowsVirtualKeyCode": 13, "nativeVirtualKeyCode": 13 }),
    ] {
        phien.goi("Input.dispatchKeyEvent", su_kien).await?;
    }

    // Enter vẫn không ăn thì bấm nút gửi. Giữ Enter làm đường chính vì nó không
    // phụ thuộc vào nhãn nút (đổi theo ngôn ngữ giao diện), còn đây là lưới an
    // toàn — và nếu cả hai trượt thì hậu kiểm bên dưới vẫn chặn, không có ca
    // nào báo "đã gửi" mà chưa gửi.
    tokio::time::sleep(Duration::from_millis(400)).await;
    let van_con = phien
        .danh_gia(
            r#"(() => {
                 const o = document.querySelector('[contenteditable="true"][role="textbox"]');
                 return o ? o.textContent.trim() : '';
               })()"#,
        )
        .await?;
    if van_con.as_str().map(str::is_empty) == Some(false) {
        // Lưới an toàn: bấm nút gửi bằng JS. Tìm nó theo **vị trí trong cây**
        // (nút cuối cùng trong vùng công cụ soạn, ngay cạnh ô soạn) chứ không
        // theo nhãn: nhãn thật ở đây là "Nhấn Enter để gửi", đổi theo ngôn ngữ
        // giao diện, và lọc theo chữ "gửi" thì trúng cả hàng chục dòng tin nhắn
        // cũ vì nhãn của chúng là "Tin nhắn do Bạn gửi lúc…".
        phien
            .danh_gia(
                r#"(() => {
                     const o = document.querySelector('[contenteditable="true"][role="textbox"]');
                     if (!o) return false;
                     const vung = o.closest('[role="region"]') || o.parentElement;
                     if (!vung) return false;
                     const nut = [...vung.querySelectorAll('[role="button"]')]
                       .filter(b => b.getBoundingClientRect().left > o.getBoundingClientRect().right);
                     const cuoi = nut[nut.length - 1];
                     if (!cuoi) return false;
                     cuoi.click();
                     return true;
                   })()"#,
            )
            .await?;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    // Xác nhận ô soạn đã rỗng — dấu hiệu tin đã rời đi. Không khẳng định mạnh
    // hơn thế: "đã gửi" theo nghĩa Messenger đã nhận thì chỉ máy chủ FB biết.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let con_lai = phien
        .danh_gia(
            r#"(() => {
                 const o = document.querySelector('[contenteditable="true"][role="textbox"]');
                 return o ? o.textContent.trim() : '';
               })()"#,
        )
        .await?;
    if con_lai.as_str().map(str::is_empty) == Some(false) {
        return Err(format!(
            "Đã gõ xong nhưng ô soạn vẫn còn chữ ({:?}) — nhiều khả năng Enter không gửi. \
             KHÔNG coi là đã gửi.",
            con_lai.as_str().unwrap_or("")
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nhan_dien_tab_messenger() {
        assert!(la_tab_messenger("https://www.messenger.com/t/123"));
        assert!(la_tab_messenger("https://www.facebook.com/messages/t/123"));
        assert!(!la_tab_messenger("https://www.google.com"));
        // Đừng khớp nhầm trang nói VỀ messenger.
        assert!(!la_tab_messenger("https://vi.wikipedia.org/wiki/Facebook"));
    }

    /// Cổng và câu hướng dẫn đi chung MỘT test: cả hai đọc cùng một biến môi
    /// trường, mà `cargo test` chạy song song trong một tiến trình — tách ra là
    /// tự chuốc một cặp test đá nhau ngẫu nhiên.
    #[test]
    fn cong_theo_env_va_huong_dan_nhac_dung_cong_do() {
        unsafe { std::env::remove_var("LIVA_MESSENGER_CDP_PORT") };
        assert_eq!(cong(), 9222, "khong dat env thi phai la 9222");

        unsafe { std::env::set_var("LIVA_MESSENGER_CDP_PORT", "9444") };
        assert_eq!(cong(), 9444);

        let h = cach_mo_trinh_duyet();
        assert!(h.contains("9444"), "huong dan phai nhac dung cong: {h}");
        assert!(h.contains("user-data-dir"), "phai nhac profile rieng");

        unsafe { std::env::remove_var("LIVA_MESSENGER_CDP_PORT") };
    }

    /// Nội dung rỗng phải bị chặn TRƯỚC khi mở kết nối nào — nếu không, test
    /// này sẽ treo chờ trình duyệt không tồn tại.
    #[tokio::test]
    async fn noi_dung_rong_bi_chan_som() {
        let e = send("123", "   ").await.unwrap_err();
        assert!(e.contains("rỗng"), "{e}");
    }
}
