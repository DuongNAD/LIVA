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
    if frame.width == 0 || frame.height == 0 {
        return Ok(RawFrame {
            width: 0,
            height: 0,
            format: frame.format,
            data: Vec::new(),
        });
    }
    let fw = frame.width.saturating_sub(1) as i32;
    let fh = frame.height.saturating_sub(1) as i32;
    if rect.x < 0 || rect.y < 0 || rect.width <= 0 || rect.height <= 0 {
        return Err(CuaError::CoordinateOutOfBounds {
            x: rect.x,
            y: rect.y,
        });
    }

    let x0 = rect.x.min(fw) as usize;
    let y0 = rect.y.min(fh) as usize;
    let crop_w = (rect.width as usize)
        .min((frame.width as usize).saturating_sub(x0))
        .max(1);
    let crop_h = (rect.height as usize)
        .min((frame.height as usize).saturating_sub(y0))
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

#[inline]
pub fn count_row_changed_pixels(
    prev_slice: &[u8],
    curr_slice: &[u8],
    roi_w: usize,
    bpp: usize,
    color_tolerance: u8,
) -> usize {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") && bpp == 4 {
            return unsafe {
                count_row_changed_pixels_avx2_bpp4(prev_slice, curr_slice, roi_w, color_tolerance)
            };
        }
    }

    count_row_changed_pixels_scalar(prev_slice, curr_slice, roi_w, bpp, color_tolerance)
}

#[inline(always)]
fn count_row_changed_pixels_scalar(
    prev_slice: &[u8],
    curr_slice: &[u8],
    roi_w: usize,
    bpp: usize,
    color_tolerance: u8,
) -> usize {
    let mut changed = 0;
    for c in 0..roi_w {
        let px_off = c * bpp;
        let p1 = &prev_slice[px_off..px_off + bpp];
        let p2 = &curr_slice[px_off..px_off + bpp];
        if pixel_exceeds_tolerance(p1, p2, bpp, color_tolerance) {
            changed += 1;
        }
    }
    changed
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn count_row_changed_pixels_avx2_bpp4(
    prev_slice: &[u8],
    curr_slice: &[u8],
    roi_w: usize,
    color_tolerance: u8,
) -> usize {
    use std::arch::x86_64::*;

    let mut changed = 0;
    let chunks = roi_w / 8; // 8 pixels of 4 bytes = 32 bytes per chunk
    let tol_vec = _mm256_set1_epi8(color_tolerance as i8);
    let zero = _mm256_setzero_si256();

    let p1_ptr = prev_slice.as_ptr();
    let p2_ptr = curr_slice.as_ptr();

    for i in 0..chunks {
        let offset = i * 32;
        let zero_mask = unsafe {
            let a = _mm256_loadu_si256(p1_ptr.add(offset) as *const __m256i);
            let b = _mm256_loadu_si256(p2_ptr.add(offset) as *const __m256i);

            let diff_ab = _mm256_subs_epu8(a, b);
            let diff_ba = _mm256_subs_epu8(b, a);
            let abs_diff = _mm256_or_si256(diff_ab, diff_ba);

            let excess = _mm256_subs_epu8(abs_diff, tol_vec);
            let eq_zero = _mm256_cmpeq_epi8(excess, zero);
            _mm256_movemask_epi8(eq_zero) as u32
        };

        if zero_mask != 0xFFFF_FFFF {
            let changed_byte_mask = !zero_mask;
            for p in 0..8 {
                if ((changed_byte_mask >> (p * 4)) & 0x0F) != 0 {
                    changed += 1;
                }
            }
        }
    }

    let rem_start = chunks * 8;
    for c in rem_start..roi_w {
        let px_off = c * 4;
        let p1 = &prev_slice[px_off..px_off + 4];
        let p2 = &curr_slice[px_off..px_off + 4];
        if pixel_exceeds_tolerance(p1, p2, 4, color_tolerance) {
            changed += 1;
        }
    }

    changed
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

        if frame_before.width == 0 || frame_before.height == 0 {
            return Ok((0, 0, 0.0));
        }

        let max_w_idx = frame_before.width.saturating_sub(1) as i32;
        let max_h_idx = frame_before.height.saturating_sub(1) as i32;

        let roi_x = roi.x.clamp(0, max_w_idx) as usize;
        let roi_y = roi.y.clamp(0, max_h_idx) as usize;
        let roi_w = (roi.width as usize).min((frame_before.width as usize).saturating_sub(roi_x));
        let roi_h = (roi.height as usize).min((frame_before.height as usize).saturating_sub(roi_y));

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

            changed_pixels +=
                count_row_changed_pixels(prev_slice, curr_slice, roi_w, bpp, color_tolerance);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CuaPixelFormat, CuaRect, RawFrame};

    #[test]
    fn test_diff_padded_roi_zero_dimensions_defensive_clamp() {
        let zero_frame_1 = RawFrame {
            width: 0,
            height: 0,
            format: CuaPixelFormat::Rgba,
            data: vec![],
        };
        let zero_frame_2 = RawFrame {
            width: 0,
            height: 0,
            format: CuaPixelFormat::Rgba,
            data: vec![],
        };
        let roi = CuaRect::new(10, 10, 50, 50);

        // Synthetic 0x0 frames must return (0, 0, 0.0) without panic
        let res = RoiDiffEngine::diff_padded_roi(&zero_frame_1, &zero_frame_2, &roi, 5);
        assert!(res.is_ok());
        let (changed, total, ratio) = res.unwrap();
        assert_eq!(changed, 0);
        assert_eq!(total, 0);
        assert_eq!(ratio, 0.0);
    }

    #[test]
    fn test_crop_raw_frame_zero_dimensions() {
        let zero_frame = RawFrame {
            width: 0,
            height: 0,
            format: CuaPixelFormat::Rgba,
            data: vec![],
        };
        let roi = CuaRect::new(0, 0, 10, 10);
        let cropped = crop_raw_frame(&zero_frame, &roi);
        assert!(cropped.is_ok());
        let f = cropped.unwrap();
        assert_eq!(f.width, 0);
        assert_eq!(f.height, 0);
    }

    #[test]
    fn test_apply_roi_padding_zero_screen() {
        let roi = CuaRect::new(5, 5, 20, 20);
        let padded = apply_roi_padding(&roi, 10, 0, 0);
        assert_eq!(padded.width, 0);
        assert_eq!(padded.height, 0);
    }
}
