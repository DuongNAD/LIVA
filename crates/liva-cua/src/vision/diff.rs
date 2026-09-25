//! High-performance localized ROI diffing engine with row-slice identity fast-paths
//! and DefaultVisualVerifier implementation.

use super::traits::{ScreenCapturerTrait, VisualVerifierTrait};
use super::types::{RawFrame, RoiDiffConfig, SemanticVerificationCriteria, VisualDiffResult};
use crate::types::{CuaError, CuaRect};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Applies protective border padding around a target CuaRect without exceeding desktop bounds.
/// Guarantees that the resulting rectangle is strictly contained within [0, 0, max_w, max_h]
/// with width >= 1 and height >= 1, even if the raw rect coordinates lie far outside the screen.
pub fn apply_roi_padding(rect: &CuaRect, padding: u32, max_w: u32, max_h: u32) -> CuaRect {
    let max_w_i = max_w as i32;
    let max_h_i = max_h as i32;

    if max_w_i <= 0 || max_h_i <= 0 {
        return CuaRect::new(0, 0, 0, 0);
    }

    let p = padding as i32;

    // 1. Initial expanded bounds using saturating arithmetic to prevent integer overflow
    let raw_left = rect.x.saturating_sub(p);
    let raw_top = rect.y.saturating_sub(p);
    let raw_right = rect.x.saturating_add(rect.width.max(0)).saturating_add(p);
    let raw_bottom = rect.y.saturating_add(rect.height.max(0)).saturating_add(p);

    // 2. Clamp left and top within valid screen raster indices [0, max - 1]
    let x = raw_left.clamp(0, max_w_i - 1);
    let y = raw_top.clamp(0, max_h_i - 1);

    // 3. Clamp right and bottom within [x + 1, max], ensuring at least 1px dimension
    // and strictly maintaining x + width <= max_w_i and y + height <= max_h_i
    let right = raw_right.clamp(x + 1, max_w_i);
    let bottom = raw_bottom.clamp(y + 1, max_h_i);

    CuaRect {
        x,
        y,
        width: right - x,
        height: bottom - y,
    }
}

/// Crops a subregion from a RawFrame directly into a new RawFrame.
pub fn crop_raw_frame(frame: &RawFrame, rect: &CuaRect) -> Result<RawFrame, CuaError> {
    let fw = frame.width as i32;
    let fh = frame.height as i32;
    if rect.x < 0 || rect.y < 0 || rect.width <= 0 || rect.height <= 0 {
        return Err(CuaError::CoordinateOutOfBounds {
            x: rect.x,
            y: rect.y,
        });
    }

    let x0 = rect.x.min(fw.saturating_sub(1)) as usize;
    let y0 = rect.y.min(fh.saturating_sub(1)) as usize;
    let crop_w = (rect.width as usize).min(frame.width as usize - x0).max(1);
    let crop_h = (rect.height as usize)
        .min(frame.height as usize - y0)
        .max(1);

    let bpp = frame.format.bytes_per_pixel();
    let src_stride = frame.width as usize * bpp;
    let dst_stride = crop_w * bpp;
    let mut out_data = Vec::with_capacity(crop_w * crop_h * bpp);

    for row in 0..crop_h {
        let sy = y0 + row;
        let start = sy * src_stride + x0 * bpp;
        let end = start + dst_stride;
        if end <= frame.data.len() {
            out_data.extend_from_slice(&frame.data[start..end]);
        } else {
            out_data.resize(out_data.len() + dst_stride, 0);
        }
    }

    Ok(RawFrame {
        width: crop_w as u32,
        height: crop_h as u32,
        format: frame.format,
        data: out_data,
    })
}

#[inline(always)]
fn pixel_exceeds_tolerance(p1: &[u8], p2: &[u8], bpp: usize, tolerance: u8) -> bool {
    let tol = tolerance as i16;
    for i in 0..bpp {
        let diff = (p1[i] as i16 - p2[i] as i16).abs();
        if diff > tol {
            return true;
        }
    }
    false
}

/// High-performance ROI diffing engine.
pub struct RoiDiffEngine;

impl RoiDiffEngine {
    /// Applies protective border padding around a target CuaRect without exceeding desktop bounds.
    pub fn apply_roi_padding(
        raw_rect: &CuaRect,
        padding: u32,
        screen_width: u32,
        screen_height: u32,
    ) -> CuaRect {
        apply_roi_padding(raw_rect, padding, screen_width, screen_height)
    }

    /// Pure diff calculation on a padded ROI returning (changed_pixels, total_roi_pixels, changed_ratio).
    pub fn diff_padded_roi(
        frame_before: &RawFrame,
        frame_after: &RawFrame,
        roi: &CuaRect,
        color_tolerance: u8,
    ) -> Result<(usize, usize, f32), CuaError> {
        if frame_before.width != frame_after.width
            || frame_before.height != frame_after.height
            || frame_before.format != frame_after.format
        {
            return Err(CuaError::Internal(
                "Mismatched frame geometry or pixel format during visual diff".into(),
            ));
        }

        let bpp = frame_before.format.bytes_per_pixel();

        // Defensive validation: ensure neither buffer is truncated to prevent indexing panics
        let expected_len = (frame_before.width as usize)
            .checked_mul(frame_before.height as usize)
            .and_then(|px| px.checked_mul(bpp))
            .ok_or_else(|| {
                CuaError::InvalidParameter("Frame dimensions overflow expected byte length".into())
            })?;

        if frame_before.data.len() < expected_len {
            return Err(CuaError::InvalidParameter(format!(
                "frame_before buffer truncated: data length {} < expected {}",
                frame_before.data.len(),
                expected_len
            )));
        }
        if frame_after.data.len() < expected_len {
            return Err(CuaError::InvalidParameter(format!(
                "frame_after buffer truncated: data length {} < expected {}",
                frame_after.data.len(),
                expected_len
            )));
        }

        let fw = frame_before.width as i32;
        let fh = frame_before.height as i32;

        let roi_x = roi.x.clamp(0, fw.saturating_sub(1)) as usize;
        let roi_y = roi.y.clamp(0, fh.saturating_sub(1)) as usize;
        let roi_w = (roi.width as usize).min(frame_before.width as usize - roi_x);
        let roi_h = (roi.height as usize).min(frame_before.height as usize - roi_y);

        let total_roi_pixels = roi_w * roi_h;
        if total_roi_pixels == 0 {
            return Ok((0, 0, 0.0));
        }

        let stride = frame_before.width as usize * bpp;
        let row_bytes = roi_w * bpp;
        let mut changed_pixels = 0usize;

        for r in 0..roi_h {
            let y = roi_y + r;
            let offset = y * stride + roi_x * bpp;
            let prev_slice = &frame_before.data[offset..offset + row_bytes];
            let curr_slice = &frame_after.data[offset..offset + row_bytes];

            // Row-slice identity fast-path: skips identical pixel rows in 1 instruction
            if prev_slice == curr_slice {
                continue;
            }

            for c in 0..roi_w {
                let px_off = c * bpp;
                let p1 = &prev_slice[px_off..px_off + bpp];
                let p2 = &curr_slice[px_off..px_off + bpp];

                if pixel_exceeds_tolerance(p1, p2, bpp, color_tolerance) {
                    changed_pixels += 1;
                }
            }
        }

        let changed_ratio = changed_pixels as f32 / total_roi_pixels as f32;
        Ok((changed_pixels, total_roi_pixels, changed_ratio))
    }

    /// Evaluates visual difference in a padded ROI against SemanticVerificationCriteria.
    pub fn diff_roi(
        prev: &RawFrame,
        curr: &RawFrame,
        padded_roi: &CuaRect,
        raw_rect: &CuaRect,
        color_tolerance: u8,
        criteria: &SemanticVerificationCriteria,
    ) -> Result<VisualDiffResult, CuaError> {
        let start_scan = Instant::now();

        let (changed_pixels, total_roi_pixels, changed_ratio) =
            Self::diff_padded_roi(prev, curr, padded_roi, color_tolerance)?;

        let scan_duration_us = start_scan.elapsed().as_micros() as u64;

        if total_roi_pixels == 0 {
            return Ok(VisualDiffResult {
                diff_detected: false,
                changed_ratio: 0.0,
                changed_pixels: 0,
                total_roi_pixels: 0,
                roi_rect: *padded_roi,
                raw_rect: *raw_rect,
                latency_ms: 0,
                scan_duration_us,
            });
        }

        let diff_detected = match criteria {
            SemanticVerificationCriteria::Click {
                min_changed_ratio,
                min_changed_pixels,
            } => changed_ratio >= *min_changed_ratio || changed_pixels >= *min_changed_pixels,
            SemanticVerificationCriteria::TypeText { min_changed_pixels } => {
                changed_pixels >= *min_changed_pixels
            }
            SemanticVerificationCriteria::Scroll {
                min_changed_ratio,
                min_changed_pixels,
            } => changed_ratio >= *min_changed_ratio || changed_pixels >= *min_changed_pixels,
            SemanticVerificationCriteria::WaitVisualChange {
                min_changed_ratio, ..
            } => changed_ratio >= *min_changed_ratio,
            SemanticVerificationCriteria::Custom {
                min_changed_ratio,
                min_changed_pixels,
            } => changed_ratio >= *min_changed_ratio || changed_pixels >= *min_changed_pixels,
        };

        Ok(VisualDiffResult {
            diff_detected,
            changed_ratio,
            changed_pixels,
            total_roi_pixels,
            roi_rect: *padded_roi,
            raw_rect: *raw_rect,
            latency_ms: 0,
            scan_duration_us,
        })
    }
}

/// Production DefaultVisualVerifier implementation coordinating screen capture,
/// redraw micro-tick delays, and localized ROI diff scanning.
pub struct DefaultVisualVerifier {
    capturer: Arc<dyn ScreenCapturerTrait>,
    config: RoiDiffConfig,
}

impl DefaultVisualVerifier {
    /// Create a new verifier wrapping a ScreenCapturerTrait and configuration.
    pub fn new(capturer: Arc<dyn ScreenCapturerTrait>, config: RoiDiffConfig) -> Self {
        Self { capturer, config }
    }
}

#[async_trait]
impl VisualVerifierTrait for DefaultVisualVerifier {
    async fn capture_baseline(&self) -> Result<RawFrame, CuaError> {
        self.capturer.capture().await
    }

    async fn verify_action(
        &self,
        target_roi: &CuaRect,
        criteria: &SemanticVerificationCriteria,
        baseline_frame: Option<&RawFrame>,
    ) -> Result<VisualDiffResult, CuaError> {
        let total_start = Instant::now();

        // 1. Obtain baseline frame if not supplied
        let pre = match baseline_frame {
            Some(f) => f.clone(),
            None => self.capturer.capture().await?,
        };

        // 2. Windows DWM / UI message loop redraw micro-tick
        if self.config.redraw_micro_tick_ms > 0 {
            tokio::time::sleep(Duration::from_millis(self.config.redraw_micro_tick_ms)).await;
        }

        // 3. Post-execution capture
        let post = self.capturer.capture().await?;

        // 4. Calculate padded ROI bounds
        let (screen_w, screen_h) = self.capturer.dimensions()?;
        let padded_roi = RoiDiffEngine::apply_roi_padding(
            target_roi,
            self.config.roi_padding,
            screen_w,
            screen_h,
        );

        // 5. Execute microsecond pixel diff scan
        let mut result = RoiDiffEngine::diff_roi(
            &pre,
            &post,
            &padded_roi,
            target_roi,
            self.config.color_tolerance,
            criteria,
        )?;

        result.latency_ms = total_start.elapsed().as_millis() as u64;
        Ok(result)
    }

    async fn wait_visual_change(
        &self,
        target_roi: &CuaRect,
        criteria: &SemanticVerificationCriteria,
        baseline_frame: &RawFrame,
    ) -> Result<VisualDiffResult, CuaError> {
        let start = Instant::now();
        let (screen_w, screen_h) = self.capturer.dimensions()?;
        let padded_roi = RoiDiffEngine::apply_roi_padding(
            target_roi,
            self.config.roi_padding,
            screen_w,
            screen_h,
        );

        let (timeout, poll_interval) = match criteria {
            SemanticVerificationCriteria::WaitVisualChange {
                poll_interval_ms,
                timeout_ms,
                ..
            } => (
                Duration::from_millis(*timeout_ms),
                Duration::from_millis(*poll_interval_ms),
            ),
            _ => (Duration::from_millis(3000), Duration::from_millis(50)),
        };

        let mut last_result = VisualDiffResult {
            diff_detected: false,
            changed_ratio: 0.0,
            changed_pixels: 0,
            total_roi_pixels: (padded_roi.width * padded_roi.height) as usize,
            roi_rect: padded_roi,
            raw_rect: *target_roi,
            latency_ms: 0,
            scan_duration_us: 0,
        };

        while start.elapsed() < timeout {
            tokio::time::sleep(poll_interval).await;
            let current = self.capturer.capture().await?;

            let res = RoiDiffEngine::diff_roi(
                baseline_frame,
                &current,
                &padded_roi,
                target_roi,
                self.config.color_tolerance,
                criteria,
            )?;

            last_result = res;
            if last_result.diff_detected {
                last_result.latency_ms = start.elapsed().as_millis() as u64;
                return Ok(last_result);
            }
        }

        last_result.latency_ms = start.elapsed().as_millis() as u64;
        Ok(last_result)
    }

    fn config(&self) -> RoiDiffConfig {
        self.config.clone()
    }
}
