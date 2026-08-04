use liva_native_core::messaging::contacts::Platform;
use liva_native_core::messaging::{VoiceMessageAction, VoiceMessageDialogue};

#[test]
fn thieu_nen_tang_thi_hoi_va_nho_lenh_den_luot_sau() {
    let mut dialogue = VoiceMessageDialogue::default();

    assert_eq!(
        dialogue.begin(
            "Minh Hiển".to_string(),
            "chiều đi bắt pokemon k".to_string(),
            None,
        ),
        VoiceMessageAction::AskPlatform,
    );
    assert_eq!(
        dialogue.follow_up("qua ứng dụng hay dùng"),
        Some(VoiceMessageAction::AskPlatform),
    );
    assert_eq!(
        dialogue.follow_up("nhắn bằng messager"),
        Some(VoiceMessageAction::Draft {
            recipient: "Minh Hiển".to_string(),
            body: "chiều đi bắt pokemon k".to_string(),
            platform: Platform::Messenger,
        }),
    );
}

#[test]
fn gui_di_xac_nhan_draft_dung_mot_lan() {
    let mut dialogue = VoiceMessageDialogue::default();
    dialogue.await_confirmation("draft-1".to_string());

    assert_eq!(
        dialogue.follow_up("đồng ý, gửi đi"),
        Some(VoiceMessageAction::Confirm {
            draft_id: "draft-1".to_string(),
        }),
    );
    assert_eq!(dialogue.follow_up("gửi đi"), None);
}

#[test]
fn huy_draft_va_khong_tu_gui_khi_cau_tra_loi_khong_ro() {
    let mut dialogue = VoiceMessageDialogue::default();
    dialogue.await_confirmation("draft-2".to_string());

    assert_eq!(
        dialogue.follow_up("để tôi nghĩ đã"),
        Some(VoiceMessageAction::RepeatConfirmation),
    );
    assert_eq!(
        dialogue.follow_up("không gửi nữa, hủy đi"),
        Some(VoiceMessageAction::Cancel {
            draft_id: "draft-2".to_string(),
        }),
    );
    assert_eq!(dialogue.follow_up("gửi đi"), None);
}

#[test]
fn thieu_noi_dung_thi_hoi_va_nho_nguoi_nhan_cung_nen_tang() {
    let mut dialogue = VoiceMessageDialogue::default();

    assert_eq!(
        dialogue.begin(
            "Minh Hiển".to_string(),
            String::new(),
            Some(Platform::Messenger),
        ),
        VoiceMessageAction::AskBody,
    );
    assert_eq!(
        dialogue.follow_up("hỏi nó chiều đi bắt pokemon k"),
        Some(VoiceMessageAction::Draft {
            recipient: "Minh Hiển".to_string(),
            body: "hỏi nó chiều đi bắt pokemon k".to_string(),
            platform: Platform::Messenger,
        }),
    );
}
