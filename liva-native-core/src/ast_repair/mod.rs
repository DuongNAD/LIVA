//! Dual-Layer AST Self-Healing & Reflexion Subsystem (Milestone 2).
//!
//! Subsystem Components:
//! - `json_repair`: High-speed (<0.1ms) deterministic Token-Aware JSON AST repair.
//! - `reflexion`: Iterative error reflection and transactional workspace rollback engine.

pub mod json_repair;
pub mod reflexion;

pub use json_repair::{
    AstRepairError, AstRepairStats, repair_json_ast, repair_json_ast_with_stats, repair_json_string,
};
pub use reflexion::{
    MAX_REFLEXION_RETRIES, ReflexionAction, ReflexionContext, ReflexionEngine, ReflexionError,
    ReflexionErrorEntry, ReflexionStatus, WorkspaceManager, WorkspaceSnapshot,
};
