//! Browser & System Automation with Multi-Tier Sandbox Guardrails.
//!
//! Covers Features 14–17:
//! - Feature 14: CDP Headless Browser Controller (`browser.rs`)
//! - Feature 15: Semantic DOM Tree Extractor (`dom.rs`)
//! - Feature 16: Cross-Platform OS & Input Automation (`system.rs`)
//! - Feature 17: Multi-Tier Sandbox & Security Guardrails (`sandbox.rs`)

pub mod browser;
pub mod dom;
pub mod sandbox;
pub mod system;

pub use browser::{
    BrowserConfig, BrowserDriver, BrowserError, CdpBrowserController, MockBrowserDriver,
    PageMetadata,
};
pub use dom::{DomExtractMode, InteractiveElement, SemanticDomExtractor};
pub use sandbox::{SandboxGuard, SandboxPolicy, SandboxViolation};
pub use system::{
    KeyAction, MockSystemAutomationDriver, MouseButtonType, NativeSystemDriver,
    SystemAutomationDriver, SystemAutomationError, WindowInfo,
};
