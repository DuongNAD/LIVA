//! Trait abstractions decoupling screen capture and visual verification.

use super::types::{RawFrame, RoiDiffConfig, SemanticVerificationCriteria, VisualDiffResult};
use crate::types::{CuaError, CuaRect};
use async_trait::async_trait;

/// Screen capture interface decoupling live OS capture from synthetic/mock capture.
#[async_trait]
pub trait ScreenCapturerTrait: Send + Sync {
    /// Capture the full screen / virtual desktop frame.
    async fn capture(&self) -> Result<RawFrame, CuaError>;

    /// Convenience alias for `capture()`.
    async fn capture_screen(&self) -> Result<RawFrame, CuaError> {
        self.capture().await
    }

    /// Capture a localized ROI rectangle directly.
    /// Default implementation captures full screen and crops in memory.
    async fn capture_roi(&self, rect: &CuaRect) -> Result<RawFrame, CuaError> {
        let full = self.capture().await?;
        super::diff::crop_raw_frame(&full, rect)
    }

    /// Query desktop dimensions in pixels (width, height).
    fn dimensions(&self) -> Result<(u32, u32), CuaError>;
}

/// Verification interface executing post-action visual feedback checks.
#[async_trait]
pub trait VisualVerifierTrait: Send + Sync {
    /// Capture a baseline frame prior to action execution.
    async fn capture_baseline(&self) -> Result<RawFrame, CuaError>;

    /// Capture post-action frame after redraw micro-tick and evaluate ROI diff.
    async fn verify_action(
        &self,
        target_roi: &CuaRect,
        criteria: &SemanticVerificationCriteria,
        baseline_frame: Option<&RawFrame>,
    ) -> Result<VisualDiffResult, CuaError>;

    /// Asynchronously poll a target ROI until visual change is detected or timeout expires.
    async fn wait_visual_change(
        &self,
        target_roi: &CuaRect,
        criteria: &SemanticVerificationCriteria,
        baseline_frame: &RawFrame,
    ) -> Result<VisualDiffResult, CuaError>;

    /// Convenience helper delegating to `verify_action`.
    async fn verify_post_action(
        &self,
        pre_frame: Option<&RawFrame>,
        target_roi: CuaRect,
        criteria: &SemanticVerificationCriteria,
    ) -> Result<VisualDiffResult, CuaError> {
        self.verify_action(&target_roi, criteria, pre_frame).await
    }

    /// Query active ROI diffing configuration.
    fn config(&self) -> RoiDiffConfig;
}
