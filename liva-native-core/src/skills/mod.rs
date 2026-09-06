//! Kho skill cục bộ, thuần Rust và hệ sinh thái ClawHub / OpenClaw Skills Runtime.
//!
//! Milestone 3 / Features 10–13:
//! - Feature 10: ClawHub `SKILL.md` Manifest Parser & Schema Validator (`manifest.rs`)
//! - Feature 11: Skills Live Hot-Reload Watcher & SHA-256 Fingerprint Diffing (`watcher.rs` & `store.rs`)
//! - Feature 12: Mid-Execution Consent Engine (`consent.rs`)
//! - Feature 13: Unified Tool Dispatcher & Hot-Reloaded Skill Router (`dispatcher.rs`)

pub mod consent;
pub mod dispatcher;
pub mod loader;
pub mod manifest;
pub mod ranker;
pub mod signals;
pub mod store;
pub mod watcher;

pub use consent::{
    ConsentAuthority, ConsentDecision, ConsentLevel, ConsentRequest, ConsentStatus,
    ConsentSuspender, InMemoryConsentManager,
};
pub use dispatcher::{
    MockToolDispatcher, NativeToolHandler, ToolCallRequest, ToolCallResult, ToolDispatcher,
    UnifiedToolDispatcher,
};
pub use loader::{LoadedSkill, load_skill_dir, load_skill_tree, pin_skill_ids};
pub use manifest::{
    ClawHubSkillParser, LoadedSkillPackage, PermissionRequirement, RiskLevel, SkillError,
    SkillManifest, SkillParser, SkillRuntimeType, SkillToolDefinition, SkillTrigger,
    parse_simple_yaml_manifest, parse_skill_markdown,
};
pub use ranker::{RankedSkill, rank_skills, rank_skills_with_prior};
pub use signals::{
    KIND_SKILL_SELECTION_NOT_INVOKED, KIND_TOOL_CALL_FAILED, KIND_TOOL_FAILURE_AFFECTS_SKILL,
    KIND_TOOL_SEMANTIC_ISSUE, SignalTally,
};
pub use store::{Signal, SkillPackageStore, SkillRecord, SkillStore, SkillVersion};
pub use watcher::{DEFAULT_DEBOUNCE_DURATION, SkillChangeEvent, SkillWatcher};

/// Tên file mang danh tính bền của một skill, đặt trong thư mục skill.
pub const SKILL_ID_FILE: &str = ".skill_id";

/// Tên file nội dung skill.
pub const SKILL_FILE: &str = "SKILL.md";
