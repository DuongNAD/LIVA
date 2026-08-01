//! Kho skill cục bộ, thuần Rust (rung G2).
//!
//! Xem `docs/03-danh-gia/04-de-xuat-tich-hop-openspace.md` §3 (G2) và §2 (lấy gì,
//! từ chối gì của OpenSpace).
//!
//! ## Vì sao rung này tồn tại
//!
//! G1 cho LIVA một **bộ chọn năng lực mở rộng được** (`llm::tool_calling`). Nhưng
//! thứ nó chọn *từ* vẫn chỉ là tool nội bộ cộng tool của server MCP ngoài — tức
//! một danh mục **cố định**, không tích luỹ được gì qua thời gian. G2 là cái kho:
//! thư mục `SKILL.md` trên đĩa → DB có lịch sử (DAG version) → truy hồi được.
//!
//! ## Ba quyết định đáng nói
//!
//! 1. **Danh tính là `.skill_id`, không phải `name` hay đường dẫn.** Đổi tên thư
//!    mục hay sửa `name:` trong front-matter thì lịch sử và tín hiệu đã tích luỹ
//!    vẫn còn. Xem [`loader::doc_skill_id`] về ca không ghi được file.
//! 2. **Định dạng đúng bằng skill của Claude Code** (`.claude/skills/*/SKILL.md`,
//!    front-matter `name` + `description`). Repo này đã có 7 skill như vậy, nên
//!    kho dùng được ngay mà không phải viết dữ liệu mẫu.
//! 3. ~~**`skill_signals` chỉ được DỰNG BẢNG ở đây.**~~ **G3 (26/07/2026) đã nối
//!    nó vào xếp hạng** — xem [`signals`] và [`ranker::rank_skills_with_prior`].
//!    Cột lấy đúng taxonomy §2 nên G3 **không phải migrate**, đúng như dự tính.

pub mod loader;
pub mod ranker;
pub mod signals;
pub mod store;

pub use loader::{LoadedSkill, load_skill_dir, load_skill_tree, pin_skill_ids};
pub use ranker::{RankedSkill, rank_skills, rank_skills_with_prior};
pub use signals::{
    KIND_SKILL_SELECTION_NOT_INVOKED, KIND_TOOL_CALL_FAILED, KIND_TOOL_FAILURE_AFFECTS_SKILL,
    KIND_TOOL_SEMANTIC_ISSUE, SignalTally,
};
pub use store::{Signal, SkillRecord, SkillStore, SkillVersion};

/// Tên file mang danh tính bền của một skill, đặt trong thư mục skill.
pub const SKILL_ID_FILE: &str = ".skill_id";

/// Tên file nội dung skill.
pub const SKILL_FILE: &str = "SKILL.md";
