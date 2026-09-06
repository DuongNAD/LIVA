use super::state::AgentState;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

pub type NodeFuture = Pin<Box<dyn Future<Output = Result<AgentState, String>> + Send>>;
pub type NodeFn = Box<dyn Fn(AgentState) -> NodeFuture + Send + Sync>;

pub struct StateGraph {
    nodes: HashMap<String, NodeFn>,
    edges: HashMap<String, String>,
    entry_point: String,
}

impl Default for StateGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl StateGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: HashMap::new(),
            entry_point: "START".to_string(),
        }
    }

    pub fn add_node<F, Fut>(&mut self, name: &str, node: F)
    where
        F: Fn(AgentState) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<AgentState, String>> + Send + 'static,
    {
        let wrapped = move |state| {
            let fut = node(state);
            Box::pin(fut) as NodeFuture
        };
        self.nodes.insert(name.to_string(), Box::new(wrapped));
    }

    pub fn add_edge(&mut self, from: &str, to: &str) {
        self.edges.insert(from.to_string(), to.to_string());
    }

    pub fn set_entry_point(&mut self, node: &str) {
        self.entry_point = node.to_string();
    }

    pub async fn run(&self, initial_state: AgentState) -> Result<AgentState, String> {
        let mut state = initial_state;
        if state.current_node.is_empty() || state.current_node == "START" {
            state.current_node = self.entry_point.clone();
        }

        while state.current_node != "__END__" {
            let current = state.current_node.clone();
            let node_fn = self
                .nodes
                .get(&current)
                .ok_or_else(|| format!("Node '{}' not found", current))?;

            state = node_fn(state.clone()).await?;

            if state.current_node == current {
                if let Some(next) = self.edges.get(&current) {
                    state.current_node = next.clone();
                } else {
                    state.current_node = "__END__".to_string();
                }
            }
        }

        Ok(state)
    }
}

pub mod checkpoint;
pub mod diff_reviewer;
pub mod hitl;
mod intent;
mod memory_scope;
pub mod pipeline;
pub mod pregel;

pub use checkpoint::{
    CheckpointRecord, Checkpointer, JsonPatchOp, SqliteCheckpointer, apply_json_patch,
    generate_json_patch,
};
pub use diff_reviewer::{
    DiffHunk, DiffLine, DiffLineType, DiffReviewRegistry, DiffReviewSession, DiffReviewStatus,
    FileDiff, HunkStatus, create_diff_review_context, evaluate_session_decision,
    parse_unified_diff, reconstruct_approved_patch,
};
pub use hitl::{ApprovalContext, ApprovalDecision, CheckpointStatus};
pub use intent::{Intent, route_intent};
pub use memory_scope::{
    ConversationMemoryScope, memory_system_message, persist_turn, persist_turn_scoped,
    recall_context, recall_context_scoped,
};
pub use pipeline::build_pipeline_graph;
pub use pregel::{BranchMergeFn, Edge, FutureNodeFn, LivaAgentRuntime, NodeError, NodeResult};

#[cfg(test)]
use intent::tach_nhan_tin;
#[cfg(test)]
use memory_scope::{persist_embedded_turn, rag_top_k, recall_embedded_context};
#[cfg(test)]
use pipeline::{finish_streamed_completion, send_llm_chunk_if_current};

#[cfg(test)]
mod tach_nhan_tin_tests {
    use super::{Intent, route_intent, tach_nhan_tin};

    fn tach(s: &str) -> (String, String) {
        let (recipient, body, _) =
            tach_nhan_tin(s).unwrap_or_else(|| panic!("phai tach duoc: {s}"));
        (recipient, body)
    }

    #[test]
    fn cau_that_cua_nguoi_dung() {
        // Đúng câu đã gõ vào widget và bị LIVA trả lời vòng vo.
        let (ten, noi_dung) = tach("nhắn tin cho Minh hiến bảo nó ngủ đi");
        assert_eq!(ten, "Minh hiến");
        assert_eq!(noi_dung, "ngủ đi", "dai tu 'no' phai bi bo khoi noi dung");
    }

    #[test]
    fn tach_nguoi_nhan_nen_tang_va_noi_dung_tu_cau_messenger() {
        for cau in [
            "nhắn tin cho Minh Hiền bằng Messenger bảo nó chiều đi bắt pokemon k",
            "nhắn tin cho Minh Hiền bằng Messager bảo nó chiều đi bắt pokemon k",
            "Nhắn tin cho Minh Hiền bằng messenger hỏi nó chiều đi bắt pokemon k",
        ] {
            match route_intent(cau) {
                Intent::SendMessage {
                    recipient,
                    body,
                    platform,
                } => {
                    assert_eq!(recipient, "Minh Hiền", "sai nguoi nhan o: {cau}");
                    assert_eq!(body, "chiều đi bắt pokemon k", "sai noi dung o: {cau}");
                    assert_eq!(platform.as_deref(), Some("messenger"), "sai nen o: {cau}");
                }
                khac => panic!("phai la SendMessage, nhan duoc {khac:?}"),
            }
        }
    }

    #[test]
    fn moi_cach_noi_co_deu_an() {
        for cau in [
            "nhắn cho Nam là mai đi học",
            "nhắn tin cho Nam là mai đi học",
            "gửi tin cho Nam là mai đi học",
            "gửi tin nhắn cho Nam là mai đi học",
        ] {
            let (ten, noi_dung) = tach(cau);
            assert_eq!(ten, "Nam", "sai ten o: {cau}");
            assert_eq!(noi_dung, "mai đi học", "sai noi dung o: {cau}");
        }
    }

    /// STT tiếng Việt bỏ dấu là chuyện thường; cò vẫn phải ăn.
    #[test]
    fn khong_dau_van_an() {
        let (ten, noi_dung) = tach("nhan cho Nam rang toi nay ranh");
        assert_eq!(ten, "Nam");
        assert_eq!(noi_dung, "toi nay ranh");
    }

    /// Lỗi đo được trên app thật 26/07/2026: "Lạ" bỏ dấu ra "la", trùng mốc
    /// "là", nên "Người Lạ Hoắc" bị cắt còn "Người". Tên người Việt đầy chữ
    /// trùng mốc sau khi bỏ dấu — `Bảo`, `Là`, `Nói` đều là tên có thật.
    #[test]
    fn ten_trung_moc_sau_khi_bo_dau_khong_bi_cat() {
        let (ten, noi_dung) = tach("nhắn cho Người Lạ Hoắc bảo alo");
        assert_eq!(ten, "Người Lạ Hoắc", "'Lạ' khong duoc coi la moc 'là'");
        assert_eq!(noi_dung, "alo");

        // `Bảo` là tên người rất phổ biến, và ở đây nó đứng ngay sau "cho" —
        // vị trí không bao giờ là mốc, vì người nhận không thể rỗng.
        let (ten2, noi_dung2) = tach("nhắn cho Bảo rằng mai đi học");
        assert_eq!(ten2, "Bảo", "'Bảo' dung ngay sau 'cho' thi van la ten");
        assert_eq!(noi_dung2, "mai đi học");
    }

    #[test]
    fn dau_hai_cham_cung_la_moc() {
        let (ten, noi_dung) = tach("nhắn cho Hiến: ngủ sớm nhé");
        assert_eq!(ten, "Hiến", "dau ':' phai bi cat khoi ten");
        assert_eq!(noi_dung, "ngủ sớm nhé");
    }

    #[test]
    fn khong_co_moc_thi_noi_dung_rong_chu_khong_doan() {
        let (ten, noi_dung) = tach("nhắn tin cho Minh Hiến");
        assert_eq!(ten, "Minh Hiến");
        assert!(noi_dung.is_empty(), "khong duoc bia noi dung: '{noi_dung}'");
    }

    #[test]
    fn dai_tu_hai_tu_cung_bi_bo() {
        assert_eq!(tach("nhắn cho Nam bảo anh ấy về sớm").1, "về sớm");
        // Nhưng "anh" đứng một mình là một phần nội dung, không phải đại từ.
        assert_eq!(tach("nhắn cho Nam bảo anh về sớm").1, "anh về sớm");
    }

    /// Bất biến quan trọng nhất của thứ tự nhánh: thân tin nhắn KHÔNG được một
    /// nhánh khác cướp mất. Nếu câu này ra `OsControl` thì LIVA bật nhạc của
    /// chính máy mình trong khi người dùng tưởng đã nhắn tin.
    #[test]
    fn than_tin_nhan_khong_bi_nhanh_khac_cuop() {
        match route_intent("nhắn cho Nam bảo bật nhạc lên") {
            Intent::SendMessage {
                recipient, body, ..
            } => {
                assert_eq!(recipient, "Nam");
                assert_eq!(body, "bật nhạc lên");
            }
            khac => panic!("phai la SendMessage, nhan duoc {khac:?}"),
        }
        match route_intent("nhắn cho Nam bảo tắt đèn đi") {
            Intent::SendMessage { .. } => {}
            khac => panic!("phai la SendMessage, nhan duoc {khac:?}"),
        }
    }

    #[test]
    fn khong_phai_lenh_nhan_tin_thi_tra_none() {
        for cau in [
            "hôm nay trời thế nào",
            "nhắn tin nhiều quá",   // có "nhắn tin" nhưng không có "cho"
            "cho tôi xem màn hình", // có "cho" nhưng không có cò
            "nhắn cho",             // có cò nhưng không có người nhận
        ] {
            assert!(tach_nhan_tin(cau).is_none(), "khong duoc tach: {cau}");
        }
    }
}

#[cfg(test)]
mod stream_backpressure_tests {
    use super::{finish_streamed_completion, send_llm_chunk_if_current};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant};
    use tokio::sync::mpsc;

    #[test]
    fn llm_chunk_queue_day_dung_sau_deadline() {
        let (tx, mut rx) = mpsc::channel(1);
        tx.try_send("first".to_string()).expect("fill queue");
        let active_session_id = AtomicU64::new(7);
        let started = Instant::now();

        let result = send_llm_chunk_if_current(
            &tx,
            &active_session_id,
            7,
            "second",
            Duration::from_millis(5),
        );

        assert!(result.is_err());
        assert!(
            started.elapsed() < Duration::from_millis(100),
            "LLM must not hold its process-wide mutex indefinitely on TTS backpressure",
        );
        assert_eq!(rx.try_recv().unwrap(), "first");
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn heartbeat_reasoning_rong_kiem_epoch_nhung_khong_chiem_tts_queue() {
        let (tx, mut rx) = mpsc::channel(1);
        let active_session_id = AtomicU64::new(7);

        send_llm_chunk_if_current(&tx, &active_session_id, 7, "", Duration::from_millis(5))
            .expect("heartbeat cua turn hien tai");

        assert!(
            rx.try_recv().is_err(),
            "heartbeat rong khong duoc vao TTS queue"
        );

        active_session_id.store(8, Ordering::SeqCst);
        assert!(
            send_llm_chunk_if_current(&tx, &active_session_id, 7, "", Duration::from_millis(5),)
                .expect_err("heartbeat epoch cu phai bi huy")
                .contains("cancelled"),
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn barge_in_dung_llm_dang_cho_tts_backpressure() {
        let (tx, _rx) = mpsc::channel(1);
        tx.try_send("first".to_string()).expect("fill queue");
        let active_session_id = Arc::new(AtomicU64::new(7));
        let active_session_for_send = Arc::clone(&active_session_id);

        let send = tokio::task::spawn_blocking(move || {
            send_llm_chunk_if_current(
                &tx,
                &active_session_for_send,
                7,
                "second",
                Duration::from_secs(1),
            )
        });
        tokio::time::sleep(Duration::from_millis(5)).await;
        active_session_id.store(8, Ordering::SeqCst);

        let result = tokio::time::timeout(Duration::from_millis(100), send)
            .await
            .expect("barge-in must interrupt backpressure wait")
            .expect("join blocking sender");
        assert!(
            result
                .expect_err("cancelled session must reject the chunk")
                .contains("cancelled"),
        );
    }

    #[test]
    fn loi_stream_khong_duoc_coi_la_completion_de_persist() {
        let active_session_id = AtomicU64::new(7);

        let result = finish_streamed_completion(
            "partial answer".to_string(),
            Some("LLM stream aborted: TTS receiver closed".to_string()),
            &active_session_id,
            7,
        );

        assert!(result.is_err());
    }

    #[test]
    fn completion_epoch_cu_khong_duoc_persist() {
        let active_session_id = AtomicU64::new(8);

        let result =
            finish_streamed_completion("stale answer".to_string(), None, &active_session_id, 7);

        assert!(result.is_err());
    }
}

#[cfg(test)]
mod router_tests {
    use super::{Intent, route_intent};

    fn smart(device: &'static str, action: &'static str) -> Intent {
        Intent::SmartHome { device, action }
    }

    fn os(tool: &'static str, action: &'static str) -> Intent {
        Intent::OsControl { tool, action }
    }

    /// U19: những câu mà model 2B trượt, đường nhanh phải xử đúng và tất định.
    ///
    /// Hai câu đầu là lý do mục này tồn tại — đo trên Qwen3-VL-2B, *"bật nhạc
    /// lên"* rơi sang chỉnh âm lượng và *"chuyển bài khác"* chọn sai hướng.
    #[test]
    fn dieu_khien_may_di_duong_nhanh() {
        assert_eq!(
            route_intent("bật nhạc lên"),
            os("control_media", "play_pause")
        );
        assert_eq!(route_intent("chuyển bài khác"), os("control_media", "next"));
        assert_eq!(
            route_intent("tạm dừng nhạc"),
            os("control_media", "play_pause")
        );
        assert_eq!(
            route_intent("quay lại bài trước"),
            os("control_media", "previous")
        );
        assert_eq!(route_intent("bài tiếp theo"), os("control_media", "next"));

        assert_eq!(
            route_intent("nhỏ nhạc lại giúp mình"),
            os("control_volume", "down")
        );
        assert_eq!(
            route_intent("giảm âm lượng xuống"),
            os("control_volume", "down")
        );
        assert_eq!(
            route_intent("tăng âm lượng lên"),
            os("control_volume", "up")
        );
        assert_eq!(route_intent("tắt tiếng đi"), os("control_volume", "mute"));
    }

    /// ĐỘ TO thắng ĐANG-PHÁT-GÌ khi câu có cả hai loại từ.
    ///
    /// "nhỏ nhạc lại" chứa cả "nhạc" (danh từ media) lẫn "nhỏ" (từ độ to). Ý
    /// người nói là âm lượng — cùng ranh giới đã ghi trong mô tả hai tool.
    #[test]
    fn do_to_thang_dang_phat_gi() {
        assert_eq!(route_intent("nhỏ nhạc lại"), os("control_volume", "down"));
        assert_eq!(route_intent("to nhạc lên"), os("control_volume", "up"));
        // Ngược lại: "tắt nhạc" là dừng phát, KHÔNG phải mute — nhánh mute đòi
        // đúng danh từ âm thanh ("tiếng", "âm lượng", "loa").
        assert_eq!(route_intent("tắt nhạc"), os("control_media", "play_pause"));
    }

    /// HỒI QUY U19: đường mới không được cướp câu của smart-home hay của
    /// những câu đời thường có chứa "bài".
    #[test]
    fn dieu_khien_may_khong_cuop_cau_khac() {
        // Không có danh từ âm thanh/nhạc ⇒ vẫn là smart-home.
        assert_eq!(route_intent("bật đèn"), smart("light", "on"));
        assert_eq!(route_intent("tắt quạt"), smart("fan", "off"));
        assert_eq!(route_intent("bật điều hoà"), smart("ac", "on"));

        // "bài" là từ rất thông dụng; có danh từ nhưng KHÔNG có động từ điều
        // khiển thì phải rơi về Chat, không được đoán bừa.
        assert_eq!(route_intent("làm bài tập xong chưa"), Intent::Chat);
        // Không có cò "cho" thì không phải lệnh nhắn tin.
        assert_eq!(route_intent("nhắn tin nhiều quá"), Intent::Chat);
        assert_eq!(route_intent("bài viết này hay đấy"), Intent::Chat);

        // Bẫy tiếng Anh: bảng từ khoá CỐ TÌNH chỉ có tiếng Việt, nên "track" và
        // "back" không thể thành "quay lại bài trước".
        assert_eq!(route_intent("let's get back on track"), Intent::Chat);
        assert_eq!(route_intent("play the next song"), Intent::Chat);
    }

    /// HỒI QUY: đây là các câu bản cũ hiểu SAI vì dùng contains() khớp chuỗi con.
    #[test]
    fn khong_con_duong_tinh_gia() {
        // "back on track": "ac" trong "b-ac-k" + "on" => bản cũ ra bật điều hoà
        assert_eq!(route_intent("let's get back on track"), Intent::Chat);
        // "coffee": "off" trong "c-off-ee"
        assert_eq!(route_intent("I want coffee and a fan"), Intent::Chat);
        // "money"/"conversation": "on" là chuỗi con
        assert_eq!(route_intent("how much money for the lamp"), Intent::Chat);
        // "office": "off" là chuỗi con
        assert_eq!(route_intent("the office light"), Intent::Chat);
        // "place": "ac" là chuỗi con
        assert_eq!(route_intent("place it on the table"), Intent::Chat);
    }

    /// HỒI QUY: bản cũ không có từ khoá tiếng Việt nào.
    #[test]
    fn hieu_duoc_tieng_viet() {
        assert_eq!(route_intent("bật đèn giúp mình"), smart("light", "on"));
        assert_eq!(route_intent("tắt quạt đi"), smart("fan", "off"));
        assert_eq!(route_intent("mở điều hoà"), smart("ac", "on"));
        assert_eq!(route_intent("tắt điều hòa nhé"), smart("ac", "off"));
        assert_eq!(route_intent("bật máy lạnh lên"), smart("ac", "on"));
    }

    #[test]
    fn van_hieu_tieng_anh() {
        assert_eq!(route_intent("turn on the light"), smart("light", "on"));
        assert_eq!(route_intent("turn off the fan"), smart("fan", "off"));
        assert_eq!(route_intent("ac on"), smart("ac", "on"));
    }

    #[test]
    fn nhan_dien_y_dinh_vision() {
        assert_eq!(route_intent("trên màn hình có gì"), Intent::Vision);
        assert_eq!(route_intent("what's on my screen"), Intent::Vision);
        assert_eq!(route_intent("take a screenshot"), Intent::Vision);
        // Vision phải thắng cả khi câu có từ khoá thiết bị
        assert_eq!(route_intent("bật đèn trên màn hình"), Intent::Vision);
    }

    #[test]
    fn thieu_mot_ve_thi_khong_goi_tool() {
        // có thiết bị nhưng không có hành động
        assert_eq!(route_intent("cái đèn"), Intent::Chat);
        // có hành động nhưng không có thiết bị
        assert_eq!(route_intent("bật lên"), Intent::Chat);
    }

    #[test]
    fn chuoi_rong_va_ky_tu_la() {
        assert_eq!(route_intent(""), Intent::Chat);
        assert_eq!(route_intent("   "), Intent::Chat);
        assert_eq!(route_intent("!!!???"), Intent::Chat);
    }

    #[test]
    fn dau_cau_khong_lam_vo_token() {
        assert_eq!(route_intent("bật đèn, nhanh!"), smart("light", "on"));
        assert_eq!(route_intent("turn on—the light."), smart("light", "on"));
    }

    #[test]
    fn khong_phan_biet_hoa_thuong() {
        assert_eq!(route_intent("BẬT ĐÈN"), smart("light", "on"));
        assert_eq!(route_intent("Turn ON The LIGHT"), smart("light", "on"));
    }
}

#[cfg(test)]
mod rag_tests {
    use super::{
        ConversationMemoryScope, persist_embedded_turn, persist_turn, persist_turn_scoped,
        rag_top_k, recall_context, recall_context_scoped, recall_embedded_context,
    };
    use crate::AppState;
    use std::sync::Arc;

    /// AppState tối thiểu, **không có model embedding** — đúng tình huống của
    /// người dùng chưa tải model về.
    fn state_khong_co_embedder() -> Arc<AppState> {
        let capturer = Arc::new(crate::vision::capture::MockScreenCapturer::new(
            64,
            64,
            crate::vision::capture::PixelFormat::Rgba,
        ));
        Arc::new(AppState {
            db: crate::db::DatabasePool::new_in_memory().expect("in-memory db"),
            crypto: crate::crypto::EncryptionEngine::new("00000000000000000000000000000000"),
            stt: tokio::sync::Mutex::new(crate::stt::SttManager::new("non_existent_dir")),
            tts: tokio::sync::Mutex::new(None),
            tts_player: crate::tts::audio::TtsAudioPlayer::new(None),
            llm: tokio::sync::Mutex::new(
                crate::llm::LlamaRouterManager::new(512, 0).expect("llm manager"),
            ),
            vad: tokio::sync::Mutex::new(None),
            denoiser: tokio::sync::Mutex::new(None),
            turn_shadow: tokio::sync::Mutex::new(None),
            aec: tokio::sync::Mutex::new(None),
            mcp_server: Arc::new(crate::mcp::server::NativeMcpServer::new("test_vault")),
            embedder: tokio::sync::Mutex::new(None),
            vision: tokio::sync::Mutex::new(crate::vision::VisionManager::new(
                capturer,
                crate::vision::VisionConfig::default(),
            )),
        })
    }

    fn dem_vector(state: &Arc<AppState>) -> i64 {
        let conn = state.db.readers.get().unwrap();
        conn.query_row("SELECT count(*) FROM vectors_meta", [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn memory_scope_cach_ly_owner_nhung_nho_xuyen_conversation() {
        let owner_a_conversation_1 =
            ConversationMemoryScope::new("telegram:100", "chat:1").expect("scope hop le");
        let owner_a_conversation_2 =
            ConversationMemoryScope::new("telegram:100", "chat:2").expect("scope hop le");
        let owner_b = ConversationMemoryScope::new("telegram:200", "chat:1").expect("scope hop le");

        let filter_a1 = owner_a_conversation_1.recall_filter();
        let filter_a2 = owner_a_conversation_2.recall_filter();
        let filter_b = owner_b.recall_filter();

        assert_eq!(filter_a1.r#type.as_deref(), Some("conversation_turn"));
        assert_eq!(
            filter_a1.domain, filter_a2.domain,
            "cung owner phai recall duoc ky uc xuyen conversation"
        );
        assert_ne!(
            filter_a1.domain, filter_b.domain,
            "hai owner khong duoc dung chung namespace RAG"
        );
        assert!(
            filter_a1
                .domain
                .as_deref()
                .is_some_and(|v| !v.trim().is_empty()),
            "owner filter la bat buoc, khong duoc roi ve truy van global"
        );
        assert!(
            filter_a1.category.is_none(),
            "recall dai han loc theo owner, khong khoa vao mot conversation"
        );
        assert_eq!(
            owner_a_conversation_1.storage_category(),
            "conversation:chat:1",
            "conversation id phai duoc luu lam lineage"
        );
    }

    #[test]
    fn memory_scope_tu_choi_dinh_danh_rong() {
        assert!(ConversationMemoryScope::new("", "chat:1").is_err());
        assert!(ConversationMemoryScope::new("   ", "chat:1").is_err());
        assert!(ConversationMemoryScope::new("telegram:100", "").is_err());
        assert!(ConversationMemoryScope::new("telegram:100", "   ").is_err());
    }

    #[test]
    fn persist_embedded_turn_noi_vao_event_ledger() {
        let db = crate::db::DatabasePool::new_in_memory().expect("in-memory db");
        let conn = db.writer.get().expect("writer");
        let vector = vec![0.01_f32; crate::db::MEMORY_VECTOR_DIM];
        let scope = ConversationMemoryScope::new("telegram:100", "chat:1").unwrap();

        let engine = crate::crypto::EncryptionEngine::new("graph-memory-test-key-32-bytes-long");
        persist_embedded_turn(&conn, &engine, &scope, "ma du an ORION-7", &vector).unwrap();

        let (event_id, domain, category, source_event_ids, raw_content): (
            String,
            String,
            String,
            String,
            String,
        ) = conn
            .query_row(
                "SELECT e.eventId, e.domain, e.category, m.source_event_ids, m.content \
                 FROM events e \
                 JOIN vectors_meta m ON m.vec_id = e.eventId \
                 WHERE m.domain = ?1 AND m.category = ?2",
                ["memory_owner:telegram:100", "conversation:chat:1"],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(domain, "memory_owner:telegram:100");
        assert_eq!(category, "conversation:chat:1");
        assert_eq!(source_event_ids, format!(r#"["{event_id}"]"#));
        assert_ne!(raw_content, "ma du an ORION-7");
        assert_eq!(
            engine.try_decrypt(&raw_content).unwrap(),
            "ma du an ORION-7"
        );
        let fts_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM vectors_fts WHERE rowid = (
                    SELECT id FROM vectors_meta WHERE vec_id = ?1
                )",
                [&event_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(fts_count, 0);
    }

    #[test]
    fn rag_khong_recall_cheo_owner_khi_vector_giong_nhau() {
        let db = crate::db::DatabasePool::new_in_memory().expect("in-memory db");
        let conn = db.writer.get().expect("writer");
        let vector = vec![0.01_f32; crate::db::MEMORY_VECTOR_DIM];
        let owner_a = ConversationMemoryScope::new("telegram:100", "chat:1").unwrap();
        let owner_b = ConversationMemoryScope::new("telegram:200", "chat:2").unwrap();

        let engine = crate::crypto::EncryptionEngine::new("graph-memory-test-key-32-bytes-long");
        persist_embedded_turn(&conn, &engine, &owner_a, "shared secret owner A", &vector).unwrap();
        persist_embedded_turn(&conn, &engine, &owner_b, "shared secret owner B", &vector).unwrap();

        let recalled_a =
            recall_embedded_context(&conn, &engine, &owner_a, "shared secret", &vector, 10)
                .unwrap()
                .expect("owner A co ky uc");
        let recalled_b =
            recall_embedded_context(&conn, &engine, &owner_b, "shared secret", &vector, 10)
                .unwrap()
                .expect("owner B co ky uc");

        assert!(recalled_a.contains("owner A"));
        assert!(
            !recalled_a.contains("owner B"),
            "owner A doc duoc ky uc owner B"
        );
        assert!(recalled_b.contains("owner B"));
        assert!(
            !recalled_b.contains("owner A"),
            "owner B doc duoc ky uc owner A"
        );
    }

    #[test]
    fn telegram_group_khong_recall_ky_uc_dm_cua_cung_owner() {
        let db = crate::db::DatabasePool::new_in_memory().expect("in-memory db");
        let conn = db.writer.get().expect("writer");
        let vector = vec![0.01_f32; crate::db::MEMORY_VECTOR_DIM];
        let dm_scope = ConversationMemoryScope::new("telegram:100", "telegram_chat:100").unwrap();
        let group_scope =
            ConversationMemoryScope::new_audience_scoped("telegram:100", "telegram_chat:-200")
                .unwrap();

        let engine = crate::crypto::EncryptionEngine::new("graph-memory-test-key-32-bytes-long");
        persist_embedded_turn(&conn, &engine, &dm_scope, "wifi DM la Hunter2", &vector).unwrap();
        persist_embedded_turn(
            &conn,
            &engine,
            &group_scope,
            "noi dung rieng cua group",
            &vector,
        )
        .unwrap();

        let recalled_group =
            recall_embedded_context(&conn, &engine, &group_scope, "wifi", &vector, 10)
                .unwrap()
                .expect("group co ky uc trong cung audience");

        assert!(recalled_group.contains("noi dung rieng cua group"));
        assert!(
            !recalled_group.contains("Hunter2"),
            "group da recall ky uc DM cua cung owner"
        );
    }

    /// Bổ sung khuyến nghị review #2: hai GROUP khác nhau của CÙNG owner phải
    /// cách ly — ký ức group A KHÔNG rò vào audience của group B (mỗi group
    /// audience_scoped theo chat_id riêng → lọc qua `category=conversation`).
    #[test]
    fn hai_group_khac_nhau_cung_owner_khong_ro_cheo() {
        let db = crate::db::DatabasePool::new_in_memory().expect("in-memory db");
        let conn = db.writer.get().expect("writer");
        let vector = vec![0.01_f32; crate::db::MEMORY_VECTOR_DIM];
        let group_a =
            ConversationMemoryScope::new_audience_scoped("telegram:100", "telegram_chat:-200")
                .unwrap();
        let group_b =
            ConversationMemoryScope::new_audience_scoped("telegram:100", "telegram_chat:-300")
                .unwrap();

        let engine = crate::crypto::EncryptionEngine::new("graph-memory-test-key-32-bytes-long");
        persist_embedded_turn(&conn, &engine, &group_a, "bi mat cua group A", &vector).unwrap();
        persist_embedded_turn(&conn, &engine, &group_b, "noi dung group B", &vector).unwrap();

        // Recall trong group B: chỉ thấy của B, KHÔNG thấy của A (dù vector giống
        // hệt và FTS 'bi mat' khớp nội dung A — category filter loại A ra).
        let recalled_b = recall_embedded_context(&conn, &engine, &group_b, "bi mat", &vector, 10)
            .unwrap()
            .expect("group B co ky uc cua chinh no");
        assert!(recalled_b.contains("noi dung group B"));
        assert!(
            !recalled_b.contains("group A"),
            "group B da recall ky uc cua group A (ro cheo audience)"
        );

        // Chiều ngược lại cũng phải cách ly.
        let recalled_a = recall_embedded_context(&conn, &engine, &group_a, "noi dung", &vector, 10)
            .unwrap()
            .expect("group A co ky uc cua chinh no");
        assert!(recalled_a.contains("group A"));
        assert!(
            !recalled_a.contains("group B"),
            "group A da recall ky uc cua group B (ro cheo audience)"
        );
    }

    /// HỢP ĐỒNG QUAN TRỌNG NHẤT của 2.2: chưa có model embedding thì RAG phải
    /// im lặng tắt, KHÔNG lỗi, KHÔNG ghi gì — hệ thống hành xử y như trước.
    #[tokio::test]
    async fn khong_co_model_thi_rag_im_lang_tat() {
        let state = state_khong_co_embedder();

        assert!(
            recall_context(&state, "hôm qua tôi nói gì").await.is_none(),
            "khong co model thi recall phai tra None"
        );

        // persist không được panic và không được ghi gì
        persist_turn(&state, "câu hỏi", "câu trả lời").await;
        assert_eq!(
            dem_vector(&state),
            0,
            "khong co model thi khong duoc ghi ky uc nao"
        );
    }

    #[tokio::test]
    async fn scoped_rag_khong_co_embedder_van_fail_closed() {
        let state = state_khong_co_embedder();
        let scope = ConversationMemoryScope::new("telegram:100", "chat:1").unwrap();

        assert!(
            recall_context_scoped(&state, "ky uc rieng", &scope)
                .await
                .is_none()
        );
        persist_turn_scoped(&state, "cau hoi", "cau tra loi", &scope).await;
        assert_eq!(dem_vector(&state), 0);
    }

    #[tokio::test]
    async fn cau_rong_khong_kich_hoat_rag() {
        let state = state_khong_co_embedder();
        assert!(recall_context(&state, "").await.is_none());
        assert!(recall_context(&state, "   ").await.is_none());

        persist_turn(&state, "", "tra loi").await;
        persist_turn(&state, "hoi", "").await;
        assert_eq!(dem_vector(&state), 0, "ve rong thi khong duoc ghi");
    }

    #[test]
    fn top_k_co_gioi_han_hop_ly() {
        // Không đặt biến -> mặc định 3
        unsafe { std::env::remove_var("LIVA_RAG_TOP_K") };
        assert_eq!(rag_top_k(), 3);

        unsafe { std::env::set_var("LIVA_RAG_TOP_K", "5") };
        assert_eq!(rag_top_k(), 5);

        // 0 và giá trị vô lý phải rơi về mặc định: 0 nghĩa là tắt RAG một cách
        // khó hiểu, còn số quá lớn sẽ làm prompt phình vượt n_ctx.
        unsafe { std::env::set_var("LIVA_RAG_TOP_K", "0") };
        assert_eq!(rag_top_k(), 3);
        unsafe { std::env::set_var("LIVA_RAG_TOP_K", "9999") };
        assert_eq!(rag_top_k(), 3);
        unsafe { std::env::set_var("LIVA_RAG_TOP_K", "abc") };
        assert_eq!(rag_top_k(), 3);

        unsafe { std::env::remove_var("LIVA_RAG_TOP_K") };
    }
}
