//! Dual-Targeting Perception Subsystem for LIVA Computer-Use Agent (CUA).
//!
//! Provides structured semantic UI element extraction, deterministic reading-order
//! Set-of-Mark (SoM) indexing ([1..N]), direct coordinate and CuaAction resolution,
//! and automatic fallback to localized visual ROI capture when semantic trees are absent.

pub mod engine;
pub mod indexer;
pub mod mock;
pub mod resolver;
pub mod types;
pub mod win32;

pub use engine::PerceptionEngine;
pub use indexer::{SoMIndexer, SoMIndexerConfig};
pub use mock::PerceptionMockFixtures;
pub use resolver::PerceptionResolver;
pub use types::{PerceptionMode, PerceptionResult, UiElement, UiRole, UiState, UiTree};
