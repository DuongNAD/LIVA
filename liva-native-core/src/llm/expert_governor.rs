use crate::agent::graph::DoKho;
use std::time::{Duration, Instant};

/// Vai trò hiện tại của mô hình trong kiến trúc Hot-Swap Router/Expert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ModelRole {
    /// Router model (nhỏ, 2B-4B, phản hồi nhanh cho hội thoại hàng ngày, điều khiển OS/SmartHome).
    #[default]
    Router,
    /// Expert model (lớn, 12B+, phục vụ lập trình chuyên sâu, phân tích cấu trúc, toán học).
    Expert,
}

/// Quyết định hành động tráo mô hình từ bộ điều phối.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwapDecision {
    /// Giữ nguyên mô hình hiện tại đang chạy.
    Stay(ModelRole),
    /// Cần hoán đổi sang Expert model.
    SwapToExpert,
    /// Cooldown TTL đã hết, cần hoàn trả tài nguyên về Router model.
    SwapToRouter,
}

/// Bộ điều phối chống dao động (Anti-Flapping Governor) cho Router ↔ Expert Hot-Swap.
///
/// Tuân thủ quy chuẩn an toàn tài nguyên VRAM trong Obsidian Vault (`Knowledge/anti_patterns.md`):
/// - Không hoán đổi về Router ngay lập tức sau câu hỏi khó (tránh VRAM Thrashing).
/// - Giữ Expert model trong bộ nhớ tối thiểu `cooldown_duration` (mặc định 120s)
///   để phục vụ các câu hỏi tiếp nối với độ trễ 0ms tráo đổi.
#[derive(Debug)]
pub struct ExpertSwapGovernor {
    pub current_role: ModelRole,
    pub expert_last_used_at: Option<Instant>,
    pub cooldown_duration: Duration,
    pub auto_swap_enabled: bool,
}

impl Default for ExpertSwapGovernor {
    fn default() -> Self {
        let cooldown_secs = std::env::var("LIVA_EXPERT_COOLDOWN_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(120);

        let auto_swap_enabled = std::env::var("LIVA_DISABLE_EXPERT_AUTOSWAP")
            .map(|v| v != "1" && !v.eq_ignore_ascii_case("true"))
            .unwrap_or(true);

        Self {
            current_role: ModelRole::Router,
            expert_last_used_at: None,
            cooldown_duration: Duration::from_secs(cooldown_secs),
            auto_swap_enabled,
        }
    }
}

impl ExpertSwapGovernor {
    /// Khởi tạo một Governor mới với cấu hình tùy chỉnh.
    pub fn new(cooldown_duration: Duration, auto_swap_enabled: bool) -> Self {
        Self {
            current_role: ModelRole::Router,
            expert_last_used_at: None,
            cooldown_duration,
            auto_swap_enabled,
        }
    }

    /// Đánh giá quyết định tráo đổi dựa trên độ khó câu hỏi và sự sẵn có của Expert model.
    pub fn evaluate_swap(&self, do_kho: DoKho, co_expert: bool) -> SwapDecision {
        if !self.auto_swap_enabled || !co_expert {
            return SwapDecision::Stay(self.current_role);
        }

        match (self.current_role, do_kho) {
            // Đang ở Router và gặp câu hỏi khó -> cần chuyển sang Expert
            (ModelRole::Router, DoKho::Kho) => SwapDecision::SwapToExpert,

            // Đang ở Router và gặp câu thường -> giữ nguyên Router
            (ModelRole::Router, DoKho::Thuong) => SwapDecision::Stay(ModelRole::Router),

            // Đang ở Expert và tiếp tục gặp câu khó -> giữ nguyên Expert (sẽ touch lại timestamp)
            (ModelRole::Expert, DoKho::Kho) => SwapDecision::Stay(ModelRole::Expert),

            // Đang ở Expert nhưng gặp câu thường:
            // Áp dụng chính sách chống dao động (Anti-Flapping):
            // Kiểm tra xem đã hết Cooldown TTL chưa?
            (ModelRole::Expert, DoKho::Thuong) => {
                if let Some(last_used) = self.expert_last_used_at {
                    if last_used.elapsed() >= self.cooldown_duration {
                        // Đã hết cooldown -> hoàn trả tài nguyên về Router
                        SwapDecision::SwapToRouter
                    } else {
                        // Vẫn trong khoảng Cooldown TTL -> tiếp tục phục vụ bằng Expert sẵn có trong VRAM
                        SwapDecision::Stay(ModelRole::Expert)
                    }
                } else {
                    // Chưa có mốc thời gian -> mặc định giữ Expert
                    SwapDecision::Stay(ModelRole::Expert)
                }
            }
        }
    }

    /// Ghi nhận mô hình đã được sử dụng ở lượt này, cập nhật trạng thái và gia hạn TTL nếu là Expert.
    pub fn record_used(&mut self, role: ModelRole) {
        self.current_role = role;
        match role {
            ModelRole::Expert => {
                self.expert_last_used_at = Some(Instant::now());
            }
            ModelRole::Router => {
                self.expert_last_used_at = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_router_to_expert_escalation_on_kho() {
        let mut gov = ExpertSwapGovernor::new(Duration::from_secs(120), true);
        assert_eq!(gov.current_role, ModelRole::Router);

        // Khi co_expert = false, không được tráo
        let decision = gov.evaluate_swap(DoKho::Kho, false);
        assert_eq!(decision, SwapDecision::Stay(ModelRole::Router));

        // Khi co_expert = true, kích hoạt SwapToExpert
        let decision = gov.evaluate_swap(DoKho::Kho, true);
        assert_eq!(decision, SwapDecision::SwapToExpert);

        // Ghi nhận chuyển sang Expert
        gov.record_used(ModelRole::Expert);
        assert_eq!(gov.current_role, ModelRole::Expert);
        assert!(gov.expert_last_used_at.is_some());
    }

    #[test]
    fn test_anti_flapping_cooldown_keeps_expert_for_thuong_prompt() {
        let mut gov = ExpertSwapGovernor::new(Duration::from_secs(120), true);
        gov.record_used(ModelRole::Expert);

        // Gặp câu hỏi thường ngay sau khi đang ở Expert -> Stay(Expert) để chống dao động
        let decision = gov.evaluate_swap(DoKho::Thuong, true);
        assert_eq!(
            decision,
            SwapDecision::Stay(ModelRole::Expert),
            "Phải giữ Expert trong khoảng Cooldown TTL để chống VRAM thrashing"
        );
    }

    #[test]
    fn test_revert_to_router_after_cooldown_expires() {
        let mut gov = ExpertSwapGovernor::new(Duration::from_millis(50), true);
        gov.record_used(ModelRole::Expert);

        // Giả lập trôi qua 60ms (> 50ms TTL)
        std::thread::sleep(Duration::from_millis(60));

        let decision = gov.evaluate_swap(DoKho::Thuong, true);
        assert_eq!(
            decision,
            SwapDecision::SwapToRouter,
            "Sau khi hết Cooldown TTL, câu hỏi thường phải kích hoạt SwapToRouter"
        );

        // Ghi nhận hoàn về Router
        gov.record_used(ModelRole::Router);
        assert_eq!(gov.current_role, ModelRole::Router);
        assert!(gov.expert_last_used_at.is_none());
    }

    #[test]
    fn test_expert_ttl_refreshed_on_subsequent_kho_prompt() {
        let mut gov = ExpertSwapGovernor::new(Duration::from_secs(120), true);
        gov.record_used(ModelRole::Expert);

        let decision = gov.evaluate_swap(DoKho::Kho, true);
        assert_eq!(decision, SwapDecision::Stay(ModelRole::Expert));
    }
}
