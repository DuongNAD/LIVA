//! Vision types, ROI models, and verification criteria for liva-cua.

use crate::types::CuaRect;
use serde::{Deserialize, Serialize};

/// Supported pixel buffer formats in liva-cua.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CuaPixelFormat {
    Rgb,
    Rgba,
    Bgr,
    Bgra,
}

impl CuaPixelFormat {
    /// Returns the number of bytes per pixel for this format.
    #[inline]
    pub const fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Rgb | Self::Bgr => 3,
            Self::Rgba | Self::Bgra => 4,
        }
    }
}

/// Raw image frame buffer captured from desktop or synthetic test harness.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawFrame {
    pub width: u32,
    pub height: u32,
    pub format: CuaPixelFormat,
    pub data: Vec<u8>,
}

impl std::fmt::Debug for RawFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RawFrame")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("format", &self.format)
            .field("data_len", &self.data.len())
            .finish()
    }
}

impl RawFrame {
    /// Create a new RawFrame validating buffer length.
    pub fn new(width: u32, height: u32, format: CuaPixelFormat, data: Vec<u8>) -> Self {
        Self {
            width,
            height,
            format,
            data,
        }
    }

    /// Expected buffer size in bytes.
    pub fn expected_byte_len(&self) -> usize {
        (self.width as usize) * (self.height as usize) * self.format.bytes_per_pixel()
    }
}

/// Configuration settings for the Localized ROI Diffing Engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoiDiffConfig {
    /// Border padding in pixels added around target element bounding box (default: 16px).
    pub roi_padding: u32,
    /// Color difference tolerance per RGB channel (0..=255, default: 5).
    pub color_tolerance: u8,
    /// Delay after action dispatch for Windows DWM / UI message loop redraw (default: 35ms).
    pub redraw_micro_tick_ms: u64,
    /// Strict SLA limit for total post-execution verification (default: 80ms).
    pub max_verification_sla_ms: u64,
}

impl Default for RoiDiffConfig {
    fn default() -> Self {
        Self {
            roi_padding: 16,
            color_tolerance: 5,
            redraw_micro_tick_ms: 35,
            max_verification_sla_ms: 80,
        }
    }
}

/// Action-specific semantic verification criteria.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "criteria_type", rename_all = "snake_case")]
pub enum SemanticVerificationCriteria {
    /// Click verification: button pressed state, focus ring, checkbox check, or modal spawn.
    Click {
        /// Ratio of changed pixels to total ROI area (default: 0.005 = 0.5%).
        min_changed_ratio: f32,
        /// Absolute changed pixel count (default: 8).
        min_changed_pixels: usize,
    },
    /// TypeText verification: character glyph appearance or cursor blinking.
    TypeText {
        /// Absolute changed pixel count (default: 6).
        min_changed_pixels: usize,
    },
    /// Scroll verification: content displacement across container viewport.
    Scroll {
        /// Ratio of changed pixels to total viewport area (default: 0.03 = 3.0%).
        min_changed_ratio: f32,
        /// Absolute changed pixel count (default: 50).
        min_changed_pixels: usize,
    },
    /// Asynchronous visual change polling loop.
    WaitVisualChange {
        /// Ratio threshold to trigger state change (default: 0.01 = 1.0%).
        min_changed_ratio: f32,
        /// Polling interval in milliseconds (default: 50ms).
        poll_interval_ms: u64,
        /// Timeout in milliseconds (default: 3000ms).
        timeout_ms: u64,
    },
    /// Custom threshold overrides.
    Custom {
        min_changed_ratio: f32,
        min_changed_pixels: usize,
    },
}

impl Default for SemanticVerificationCriteria {
    fn default() -> Self {
        Self::Click {
            min_changed_ratio: 0.005,
            min_changed_pixels: 8,
        }
    }
}

impl SemanticVerificationCriteria {
    /// Standard click criteria (0.5% area or 8 pixels).
    pub fn click_default() -> Self {
        Self::Click {
            min_changed_ratio: 0.005,
            min_changed_pixels: 8,
        }
    }

    /// Standard type text criteria (6 pixels minimum).
    pub fn type_text_default() -> Self {
        Self::TypeText {
            min_changed_pixels: 6,
        }
    }

    /// Standard scroll criteria (3.0% area or 50 pixels).
    pub fn scroll_default() -> Self {
        Self::Scroll {
            min_changed_ratio: 0.03,
            min_changed_pixels: 50,
        }
    }

    /// Standard wait visual change criteria.
    pub fn wait_visual_change_default(timeout_ms: u64) -> Self {
        Self::WaitVisualChange {
            min_changed_ratio: 0.01,
            poll_interval_ms: 50,
            timeout_ms,
        }
    }
}

/// Comprehensive outcome of localized visual ROI diffing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisualDiffResult {
    /// True if visual change satisfied the semantic verification criteria.
    pub diff_detected: bool,
    /// Ratio of changed pixels relative to the evaluated ROI area (0.0 ..= 1.0).
    pub changed_ratio: f32,
    /// Absolute count of pixels exceeding color tolerance.
    pub changed_pixels: usize,
    /// Total pixels evaluated inside the padded ROI.
    pub total_roi_pixels: usize,
    /// Padded ROI rectangle actually evaluated in screen coordinates.
    pub roi_rect: CuaRect,
    /// Raw unpadded element rectangle.
    pub raw_rect: CuaRect,
    /// Total duration in milliseconds for the verification phase (micro-tick + capture + diff).
    pub latency_ms: u64,
    /// Pure pixel diff scan execution time in microseconds.
    pub scan_duration_us: u64,
}
