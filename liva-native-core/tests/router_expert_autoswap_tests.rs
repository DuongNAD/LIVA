use liva_native_core::agent::graph::{DoKho, phan_loai_do_kho};
use liva_native_core::llm::{ExpertSwapGovernor, LlamaRouterManager, ModelRole, SwapDecision};
use std::time::Duration;

#[test]
fn test_governor_initial_state_is_router() {
    let gov = ExpertSwapGovernor::new(Duration::from_secs(120), true);
    assert_eq!(gov.current_role, ModelRole::Router);
    assert!(gov.expert_last_used_at.is_none());
    assert!(gov.auto_swap_enabled);
}

#[test]
fn test_governor_stays_on_router_for_regular_prompts() {
    let gov = ExpertSwapGovernor::new(Duration::from_secs(120), true);

    let decision = gov.evaluate_swap(DoKho::Thuong, true);
    assert_eq!(
        decision,
        SwapDecision::Stay(ModelRole::Router),
        "Prompt thông thường không được kích hoạt tráo đổi sang Expert"
    );
}

#[test]
fn test_governor_escalates_to_expert_on_complex_prompt() {
    let mut gov = ExpertSwapGovernor::new(Duration::from_secs(120), true);

    // 1. Khi không có expert model trên đĩa: phải giữ nguyên Router an toàn
    let decision = gov.evaluate_swap(DoKho::Kho, false);
    assert_eq!(
        decision,
        SwapDecision::Stay(ModelRole::Router),
        "Không có file Expert model thì không được kích hoạt tráo đổi"
    );

    // 2. Khi có file expert model trên đĩa: kích hoạt SwapToExpert
    let decision = gov.evaluate_swap(DoKho::Kho, true);
    assert_eq!(
        decision,
        SwapDecision::SwapToExpert,
        "Khi có Expert model và gặp prompt khó -> phải kích hoạt SwapToExpert"
    );

    gov.record_used(ModelRole::Expert);
    assert_eq!(gov.current_role, ModelRole::Expert);
    assert!(gov.expert_last_used_at.is_some());
}

#[test]
fn test_governor_anti_flapping_policy_keeps_expert_during_cooldown() {
    let mut gov = ExpertSwapGovernor::new(Duration::from_secs(120), true);
    gov.record_used(ModelRole::Expert);

    // Ngay sau khi ở Expert, người dùng hỏi một câu hỏi bình thường ("Cảm ơn bạn nhé")
    // Thay vì swap ngược về Router gây VRAM Thrashing, Governor giữ Expert trong 120s TTL
    let decision = gov.evaluate_swap(DoKho::Thuong, true);
    assert_eq!(
        decision,
        SwapDecision::Stay(ModelRole::Expert),
        "Chính sách chống dao động (Anti-Flapping) phải giữ Expert trong khoảng Cooldown TTL"
    );

    // Tiếp tục hỏi thêm một câu khó -> gia hạn TTL
    let decision_kho = gov.evaluate_swap(DoKho::Kho, true);
    assert_eq!(decision_kho, SwapDecision::Stay(ModelRole::Expert));
    gov.record_used(ModelRole::Expert);
}

#[test]
fn test_governor_reverts_to_router_after_cooldown_expires() {
    // Đặt cooldown ngắn 40ms để kiểm tra hoàn trả tài nguyên
    let mut gov = ExpertSwapGovernor::new(Duration::from_millis(40), true);
    gov.record_used(ModelRole::Expert);

    // Chờ 50ms (> 40ms TTL)
    std::thread::sleep(Duration::from_millis(50));

    // Sau khi hết TTL, một câu hỏi bình thường sẽ hoàn trả về Router
    let decision = gov.evaluate_swap(DoKho::Thuong, true);
    assert_eq!(
        decision,
        SwapDecision::SwapToRouter,
        "Hết Cooldown TTL mà không có prompt khó mới -> phải kích hoạt SwapToRouter"
    );

    gov.record_used(ModelRole::Router);
    assert_eq!(gov.current_role, ModelRole::Router);
    assert!(gov.expert_last_used_at.is_none());
}

#[test]
fn test_governor_disabled_when_auto_swap_is_false() {
    let gov = ExpertSwapGovernor::new(Duration::from_secs(120), false);

    let decision = gov.evaluate_swap(DoKho::Kho, true);
    assert_eq!(
        decision,
        SwapDecision::Stay(ModelRole::Router),
        "Khi tắt auto-swap thì luôn giữ nguyên role hiện tại"
    );
}

#[test]
fn test_complexity_heuristic_categorization_accuracy() {
    // 1. Các trường hợp câu hỏi phức tạp (phải phân loại DoKho::Kho)
    let complex_prompts = [
        "Viết hàm quicksort bằng Rust và phân tích độ phức tạp thời gian thuật toán từng bước?",
        "Hãy so sánh kiến trúc microservices và monolithic trong hệ thống tài chính phân tán?",
        "```rust\nfn main() { println!(\"debug this memory leak\"); }\n```\nSửa lỗi crash giúp mình với",
        "Giải thích cơ chế hoạt động của Raft consensus algorithm qua các trạng thái Leader, Follower, Candidate?",
    ];

    for prompt in complex_prompts {
        assert_eq!(
            phan_loai_do_kho(prompt),
            DoKho::Kho,
            "Prompt phải được phân loại DoKho::Kho: {prompt}"
        );
    }

    // 2. Các trường hợp câu hỏi thông thường (phải phân loại DoKho::Thuong)
    let simple_prompts = [
        "Xin chào LIVA",
        "Bật đèn phòng khách giúp mình",
        "Hôm nay trời có mưa không?",
        "Mấy giờ rồi bạn ơi?",
    ];

    for prompt in simple_prompts {
        assert_eq!(
            phan_loai_do_kho(prompt),
            DoKho::Thuong,
            "Prompt phải được phân loại DoKho::Thuong: {prompt}"
        );
    }
}

#[tokio::test]
async fn test_llama_router_manager_maybe_auto_swap_execution() {
    let mut mgr = LlamaRouterManager::new(2048, 0).expect("create manager");

    let expert_path_opt = liva_native_core::configured_expert_model_path();
    let co_expert = expert_path_opt.as_ref().is_some_and(|p| p.exists());

    let path = mgr
        .maybe_auto_swap(DoKho::Kho)
        .await
        .expect("safe execution");
    assert_eq!(path, mgr.current_model_path);

    if co_expert {
        assert_eq!(mgr.governor.current_role, ModelRole::Expert);
        assert!(mgr.governor.expert_last_used_at.is_some());

        // Lượt tiếp theo với prompt thông thường trong thời gian cooldown:
        // Phải giữ nguyên Expert model (Anti-flapping)!
        let next_path = mgr
            .maybe_auto_swap(DoKho::Thuong)
            .await
            .expect("safe execution");
        assert_eq!(next_path, mgr.current_model_path);
        assert_eq!(mgr.governor.current_role, ModelRole::Expert);
    } else {
        assert_eq!(mgr.governor.current_role, ModelRole::Router);
    }
}
