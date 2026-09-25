//! Vision subsystem for liva-cua: frame capture, localized ROI diffing,
//! semantic verification criteria, and test mocks.

pub mod diff;
pub mod mock;
pub mod traits;
pub mod types;

pub use diff::{apply_roi_padding, crop_raw_frame, DefaultVisualVerifier, RoiDiffEngine};
pub use mock::MockScreenCapturer;
pub use traits::{ScreenCapturerTrait, VisualVerifierTrait};
pub use types::{
    CuaPixelFormat, RawFrame, RoiDiffConfig, SemanticVerificationCriteria, VisualDiffResult,
};
