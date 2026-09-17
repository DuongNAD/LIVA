use crate::vision::diff::{BoundingBox, DiffEngine, VisualRoiPatch, downsample_rgb_to_720p};
use std::fmt;
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PixelFormat {
    Rgb,
    Rgba,
    Bgr,
    Bgra,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub data: Vec<u8>,
}

impl fmt::Debug for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Frame")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("format", &self.format)
            .field("data_len", &self.data.len())
            .finish()
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum CaptureError {
    DisplayNotFound,
    PermissionDenied,
    HardwareError(String),
    InvalidFrameSize,
    OsError(String),
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CaptureError::DisplayNotFound => write!(f, "Display not found"),
            CaptureError::PermissionDenied => write!(f, "OS permission denied capture"),
            CaptureError::HardwareError(s) => write!(f, "Hardware error: {}", s),
            CaptureError::InvalidFrameSize => write!(f, "Invalid frame size"),
            CaptureError::OsError(s) => write!(f, "OS capture error: {}", s),
        }
    }
}

impl std::error::Error for CaptureError {}

/// Current mouse cursor position in virtual-desktop pixels, or `None` if
/// unavailable (also on non-Windows).
#[cfg(windows)]
pub fn cursor_position() -> Option<(i32, i32)> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut pt = POINT { x: 0, y: 0 };
    if unsafe { GetCursorPos(&mut pt) } != 0 {
        Some((pt.x, pt.y))
    } else {
        None
    }
}
#[cfg(not(windows))]
pub fn cursor_position() -> Option<(i32, i32)> {
    None
}

/// Crop an arbitrary rectangular BoundingBox from a Frame directly into tightly-packed RGB.
pub fn crop_frame_bbox_rgb(frame: &Frame, bbox: &BoundingBox) -> (u32, u32, Vec<u8>) {
    let fw = frame.width as usize;
    let fh = frame.height as usize;
    if fw == 0 || fh == 0 || bbox.width == 0 || bbox.height == 0 {
        return (0, 0, Vec::new());
    }

    let x0 = bbox.x.min(fw.saturating_sub(1));
    let y0 = bbox.y.min(fh.saturating_sub(1));
    let crop_w = bbox.width.min(fw.saturating_sub(x0)).max(1);
    let crop_h = bbox.height.min(fh.saturating_sub(y0)).max(1);

    let (channels, bgr) = match frame.format {
        PixelFormat::Rgba => (4usize, false),
        PixelFormat::Bgra => (4usize, true),
        PixelFormat::Rgb => (3usize, false),
        PixelFormat::Bgr => (3usize, true),
    };
    let stride = fw * channels;
    let mut out = Vec::with_capacity(crop_w * crop_h * 3);

    for y in 0..crop_h {
        let sy = y0 + y;
        let row_start = sy * stride;
        for x in 0..crop_w {
            let idx = row_start + (x0 + x) * channels;
            if idx + 2 < frame.data.len() {
                if bgr {
                    out.extend_from_slice(&[
                        frame.data[idx + 2],
                        frame.data[idx + 1],
                        frame.data[idx],
                    ]);
                } else {
                    out.extend_from_slice(&[
                        frame.data[idx],
                        frame.data[idx + 1],
                        frame.data[idx + 2],
                    ]);
                }
            } else {
                out.extend_from_slice(&[0, 0, 0]);
            }
        }
    }
    (crop_w as u32, crop_h as u32, out)
}

/// Convert a whole frame to tightly-packed RGB (`w*h*3`).
pub fn frame_to_rgb(frame: &Frame) -> (u32, u32, Vec<u8>) {
    let rgb: Vec<u8> = match frame.format {
        PixelFormat::Rgba => frame
            .data
            .chunks_exact(4)
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect(),
        PixelFormat::Bgra => frame
            .data
            .chunks_exact(4)
            .flat_map(|p| [p[2], p[1], p[0]])
            .collect(),
        PixelFormat::Rgb => frame.data.clone(),
        PixelFormat::Bgr => frame
            .data
            .chunks_exact(3)
            .flat_map(|p| [p[2], p[1], p[0]])
            .collect(),
    };
    (frame.width, frame.height, rgb)
}

/// Crop a `w`×`h` region centred on `(cx, cy)` (clamped to the frame) and return
/// it as tightly-packed RGB. The Mouse-Guided Vision primitive: a small crop
/// around the cursor is far cheaper for the VLM than a full 4K frame.
pub fn region_rgb(frame: &Frame, cx: i32, cy: i32, w: u32, h: u32) -> (u32, u32, Vec<u8>) {
    let fw = frame.width as i32;
    let fh = frame.height as i32;
    let w = (w.min(frame.width) as i32).max(1);
    let h = (h.min(frame.height) as i32).max(1);
    let x0 = (cx - w / 2).clamp(0, (fw - w).max(0));
    let y0 = (cy - h / 2).clamp(0, (fh - h).max(0));
    let (channels, bgr) = match frame.format {
        PixelFormat::Rgba => (4usize, false),
        PixelFormat::Bgra => (4usize, true),
        PixelFormat::Rgb => (3usize, false),
        PixelFormat::Bgr => (3usize, true),
    };
    let stride = frame.width as usize * channels;
    let mut out = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        let sy = (y0 + y) as usize;
        for x in 0..w {
            let idx = sy * stride + (x0 + x) as usize * channels;
            if idx + 2 < frame.data.len() {
                if bgr {
                    out.extend_from_slice(&[
                        frame.data[idx + 2],
                        frame.data[idx + 1],
                        frame.data[idx],
                    ]);
                } else {
                    out.extend_from_slice(&[
                        frame.data[idx],
                        frame.data[idx + 1],
                        frame.data[idx + 2],
                    ]);
                }
            } else {
                out.extend_from_slice(&[0, 0, 0]);
            }
        }
    }
    (w as u32, h as u32, out)
}

/// Clamps packed RGB image dimensions to at most `(max_w, max_h)` while preserving
/// the aspect ratio using bilinear downsampling.
pub fn clamp_rgb_dimensions(
    src_rgb: &[u8],
    w: u32,
    h: u32,
    max_w: u32,
    max_h: u32,
) -> (u32, u32, Vec<u8>) {
    if w <= max_w && h <= max_h {
        return (w, h, src_rgb.to_vec());
    }

    let ratio = (max_w as f32 / w as f32).min(max_h as f32 / h as f32);
    let target_w = ((w as f32 * ratio).round() as u32).max(1);
    let target_h = ((h as f32 * ratio).round() as u32).max(1);

    tracing::debug!(
        "vision: downscaling image from {}x{} to {}x{} (max {}x{})",
        w,
        h,
        target_w,
        target_h,
        max_w,
        max_h
    );

    let mut out = Vec::with_capacity((target_w * target_h * 3) as usize);
    let x_scale = w as f32 / target_w as f32;
    let y_scale = h as f32 / target_h as f32;

    for dy in 0..target_h {
        let sy = (dy as f32 + 0.5) * y_scale - 0.5;
        let y0 = (sy.floor() as i32).clamp(0, (h - 1) as i32) as usize;
        let y1 = (y0 + 1).min((h - 1) as usize);
        let y_weight = (sy - y0 as f32).clamp(0.0, 1.0);

        for dx in 0..target_w {
            let sx = (dx as f32 + 0.5) * x_scale - 0.5;
            let x0 = (sx.floor() as i32).clamp(0, (w - 1) as i32) as usize;
            let x1 = (x0 + 1).min((w - 1) as usize);
            let x_weight = (sx - x0 as f32).clamp(0.0, 1.0);

            let idx00 = (y0 * w as usize + x0) * 3;
            let idx01 = (y0 * w as usize + x1) * 3;
            let idx10 = (y1 * w as usize + x0) * 3;
            let idx11 = (y1 * w as usize + x1) * 3;

            for c in 0..3 {
                let p00 = src_rgb.get(idx00 + c).copied().unwrap_or(0) as f32;
                let p01 = src_rgb.get(idx01 + c).copied().unwrap_or(0) as f32;
                let p10 = src_rgb.get(idx10 + c).copied().unwrap_or(0) as f32;
                let p11 = src_rgb.get(idx11 + c).copied().unwrap_or(0) as f32;

                let top = p00 * (1.0 - x_weight) + p01 * x_weight;
                let bottom = p10 * (1.0 - x_weight) + p11 * x_weight;
                let val = top * (1.0 - y_weight) + bottom * y_weight;

                out.push(val.round().clamp(0.0, 255.0) as u8);
            }
        }
    }

    (target_w, target_h, out)
}

static PREV_VISION_FRAME: Mutex<Option<Frame>> = Mutex::new(None);

pub fn set_last_vision_frame(frame: Option<Frame>) {
    *PREV_VISION_FRAME.lock().unwrap_or_else(|e| e.into_inner()) = frame;
}

pub fn clear_last_vision_frame() {
    *PREV_VISION_FRAME.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// Clamps packed RGB image dimensions so that visual tokens (ceil(W/28) * ceil(H/28))
/// never exceed `max_tokens` (default 384) while preserving aspect ratio.
pub fn clamp_to_visual_tokens(
    src_rgb: &[u8],
    w: u32,
    h: u32,
    max_tokens: usize,
) -> (u32, u32, Vec<u8>) {
    if w == 0 || h == 0 {
        return (w, h, src_rgb.to_vec());
    }

    let calc_tokens = |cw: u32, ch: u32| -> usize {
        let grid_w = (cw as f32 / 28.0).ceil() as usize;
        let grid_h = (ch as f32 / 28.0).ceil() as usize;
        grid_w * grid_h
    };

    if calc_tokens(w, h) <= max_tokens {
        return (w, h, src_rgb.to_vec());
    }

    let gw = (w as f32 / 28.0).ceil() as usize;
    let gh = (h as f32 / 28.0).ceil() as usize;
    let scale = (max_tokens as f32 / (gw * gh) as f32).sqrt();

    let mut target_gw = ((gw as f32 * scale).floor() as usize).max(1);
    let mut target_gh = ((gh as f32 * scale).floor() as usize).max(1);

    while target_gw * target_gh > max_tokens {
        if target_gw * h as usize >= target_gh * w as usize {
            target_gw = target_gw.saturating_sub(1).max(1);
        } else {
            target_gh = target_gh.saturating_sub(1).max(1);
        }
    }

    let max_w = (target_gw * 28) as u32;
    let max_h = (target_gh * 28) as u32;
    clamp_rgb_dimensions(src_rgb, w, h, max_w, max_h)
}

/// Pure vision frame processing engine.
/// Compares `curr` against `prev`:
/// - If diff area <= 35%: crops ROI with 16px padding and preserves aspect ratio.
/// - If diff area > 35%, 0% diff, or initial frame: falls back to 720p co-scale.
pub fn process_vision_frame(
    curr: &Frame,
    prev: Option<&Frame>,
) -> Result<(u32, u32, Vec<u8>, Option<VisualRoiPatch>), String> {
    let region = std::env::var("LIVA_VISION_REGION").unwrap_or_else(|_| "auto".to_string());
    let use_cursor = match region.to_lowercase().as_str() {
        "full" => false,
        "cursor" => true,
        _ => crate::governor::game_mode_active_now(),
    };

    if use_cursor {
        let crop: u32 = std::env::var("LIVA_VISION_CROP")
            .ok()
            .and_then(|s| s.parse().ok())
            .filter(|&c| c > 0)
            .unwrap_or(512);
        let (cw, ch, rgb) = if let Some((cx, cy)) = cursor_position() {
            tracing::debug!("vision: mouse-guided crop {}px at ({},{})", crop, cx, cy);
            region_rgb(curr, cx, cy, crop, crop)
        } else {
            frame_to_rgb(curr)
        };
        let (fw, fh, f_rgb) = clamp_rgb_dimensions(&rgb, cw, ch, 1280, 720);
        return Ok((fw, fh, f_rgb, None));
    }

    let prev_frame = match prev {
        Some(p) if p.width == curr.width && p.height == curr.height && p.format == curr.format => p,
        _ => {
            // Initial frame or display resolution mismatch: fallback to 720p co-scale
            let (w, h, rgb) = frame_to_rgb(curr);
            let (dw, dh, d_rgb) = downsample_rgb_to_720p(&rgb, w as usize, h as usize);
            return Ok((dw as u32, dh as u32, d_rgb, None));
        }
    };

    let patch_res = DiffEngine::compute_roi_patch(prev_frame, curr, 16);
    match patch_res {
        Ok(Some(patch)) if !patch.is_co_scaled => {
            // Changed area <= 35%: crop ROI patch
            let (cw, ch, crop_rgb) = crop_frame_bbox_rgb(curr, &patch.padded_bounding_box);
            // Cap visual tokens strictly at <= 384 tokens across all aspect ratios (PROJECT.md SLA)
            let (final_w, final_h, final_rgb) = clamp_to_visual_tokens(&crop_rgb, cw, ch, 384);
            Ok((final_w, final_h, final_rgb, Some(patch)))
        }
        Ok(Some(patch)) => {
            // Changed area > 35%: Co-scale Fallback to 720p
            let (w, h, rgb) = frame_to_rgb(curr);
            let (dw, dh, d_rgb) = downsample_rgb_to_720p(&rgb, w as usize, h as usize);
            Ok((dw as u32, dh as u32, d_rgb, Some(patch)))
        }
        Ok(None) => {
            // 0% difference (identical frame): return 720p co-scale baseline
            let (w, h, rgb) = frame_to_rgb(curr);
            let (dw, dh, d_rgb) = downsample_rgb_to_720p(&rgb, w as usize, h as usize);
            Ok((dw as u32, dh as u32, d_rgb, None))
        }
        Err(e) => {
            tracing::warn!(
                "DiffEngine::compute_roi_patch failed: {}, falling back to 720p",
                e
            );
            let (w, h, rgb) = frame_to_rgb(curr);
            let (dw, dh, d_rgb) = downsample_rgb_to_720p(&rgb, w as usize, h as usize);
            Ok((dw as u32, dh as u32, d_rgb, None))
        }
    }
}

pub fn capture_for_vision_from<C: ScreenCapturer + ?Sized>(
    capturer: &C,
    prev_slot: &Mutex<Option<Frame>>,
) -> Result<(u32, u32, Vec<u8>, Option<VisualRoiPatch>), String> {
    let frame = capturer
        .capture()
        .map_err(|e| format!("screen capture: {}", e))?;
    let mut guard = prev_slot.lock().unwrap_or_else(|e| e.into_inner());
    let result = process_vision_frame(&frame, guard.as_ref());
    *guard = Some(frame);
    result
}

/// Capture an image for a vision request, context-aware so the footprint stays
/// small while a game is running.
///
/// Returns `(width, height, RGB)`. Runs on the calling (blocking) thread.
pub fn capture_for_vision() -> Result<(u32, u32, Vec<u8>), String> {
    let capturer = NativeScreenCapturer::new(0);
    let (w, h, rgb, _patch) = capture_for_vision_from(&capturer, &PREV_VISION_FRAME)?;
    Ok((w, h, rgb))
}

/// Extended version returning VisualRoiPatch metadata for Avatar gaze orientation.
pub fn capture_for_vision_with_meta() -> Result<(u32, u32, Vec<u8>, Option<VisualRoiPatch>), String>
{
    let capturer = NativeScreenCapturer::new(0);
    capture_for_vision_from(&capturer, &PREV_VISION_FRAME)
}

pub trait ScreenCapturer: Send + Sync {
    fn capture(&self) -> Result<Frame, CaptureError>;
    fn dimensions(&self) -> Result<(u32, u32), CaptureError>;
}

use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static CACHED_MONITOR: RefCell<HashMap<u32, xcap::Monitor>> = RefCell::new(HashMap::new());
}

/// Production implementation using native bindings (xcap library)
pub struct NativeScreenCapturer {
    pub display_id: u32,
}

impl NativeScreenCapturer {
    pub fn new(display_id: u32) -> Self {
        Self { display_id }
    }

    fn get_monitor(&self) -> Result<xcap::Monitor, CaptureError> {
        CACHED_MONITOR.with(|cache| {
            let mut cache = cache.borrow_mut();
            if let Some(monitor) = cache.get(&self.display_id) {
                return Ok(monitor.clone());
            }
            let monitors =
                xcap::Monitor::all().map_err(|e| CaptureError::OsError(e.to_string()))?;
            let found = monitors
                .iter()
                .find(|m| m.id().ok() == Some(self.display_id))
                .cloned()
                .or_else(|| monitors.get(self.display_id as usize).cloned())
                .ok_or(CaptureError::DisplayNotFound)?;
            cache.insert(self.display_id, found.clone());
            Ok(found)
        })
    }

    fn invalidate_cache(&self) {
        let _ = CACHED_MONITOR.with(|cache| cache.borrow_mut().remove(&self.display_id));
    }
}

impl ScreenCapturer for NativeScreenCapturer {
    fn capture(&self) -> Result<Frame, CaptureError> {
        let monitor = self.get_monitor()?;
        match monitor.capture_image() {
            Ok(image) => Ok(Frame {
                width: image.width(),
                height: image.height(),
                format: PixelFormat::Rgba,
                data: image.into_raw(),
            }),
            Err(_) => {
                self.invalidate_cache();
                let monitor = self.get_monitor()?;
                let image = monitor
                    .capture_image()
                    .map_err(|e| CaptureError::HardwareError(e.to_string()))?;
                Ok(Frame {
                    width: image.width(),
                    height: image.height(),
                    format: PixelFormat::Rgba,
                    data: image.into_raw(),
                })
            }
        }
    }

    fn dimensions(&self) -> Result<(u32, u32), CaptureError> {
        let monitor = self.get_monitor()?;
        let w = match monitor.width() {
            Ok(w) => w,
            Err(_) => {
                self.invalidate_cache();
                let monitor = self.get_monitor()?;
                monitor
                    .width()
                    .map_err(|e| CaptureError::HardwareError(e.to_string()))?
            }
        };
        let h = match monitor.height() {
            Ok(h) => h,
            Err(_) => {
                self.invalidate_cache();
                let monitor = self.get_monitor()?;
                monitor
                    .height()
                    .map_err(|e| CaptureError::HardwareError(e.to_string()))?
            }
        };
        Ok((w, h))
    }
}

/// Mock implementation for testing
pub struct MockScreenCapturer {
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub next_frame_data: std::sync::Mutex<Vec<u8>>,
    pub fail_with: std::sync::Mutex<Option<CaptureError>>,
}

impl MockScreenCapturer {
    pub fn new(width: u32, height: u32, format: PixelFormat) -> Self {
        let channels = match format {
            PixelFormat::Rgb | PixelFormat::Bgr => 3,
            PixelFormat::Rgba | PixelFormat::Bgra => 4,
        };
        Self {
            width,
            height,
            format,
            next_frame_data: std::sync::Mutex::new(vec![0; (width * height * channels) as usize]),
            fail_with: std::sync::Mutex::new(None),
        }
    }

    pub fn set_frame_data(&self, data: Vec<u8>) {
        *self
            .next_frame_data
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = data;
    }

    pub fn set_fail_with(&self, err: Option<CaptureError>) {
        *self.fail_with.lock().unwrap_or_else(|e| e.into_inner()) = err;
    }
}

impl ScreenCapturer for MockScreenCapturer {
    fn capture(&self) -> Result<Frame, CaptureError> {
        if let Some(err) = self
            .fail_with
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
        {
            return Err(err);
        }
        Ok(Frame {
            width: self.width,
            height: self.height,
            format: self.format.clone(),
            data: self
                .next_frame_data
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
        })
    }

    fn dimensions(&self) -> Result<(u32, u32), CaptureError> {
        if let Some(err) = self
            .fail_with
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
        {
            return Err(err);
        }
        Ok((self.width, self.height))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_capturer_happy_path() {
        let capturer = MockScreenCapturer::new(4, 4, PixelFormat::Rgb);
        capturer.set_frame_data(vec![128; 48]); // 4x4x3 = 48 bytes

        let frame_result = capturer.capture();
        assert!(frame_result.is_ok());
        let frame = frame_result.unwrap();
        assert_eq!(frame.width, 4);
        assert_eq!(frame.height, 4);
        assert_eq!(frame.format, PixelFormat::Rgb);
        assert_eq!(frame.data[0], 128);
    }

    #[test]
    fn test_mock_capturer_failure_propagation() {
        let capturer = MockScreenCapturer::new(4, 4, PixelFormat::Rgb);
        capturer.set_fail_with(Some(CaptureError::PermissionDenied));

        let frame_result = capturer.capture();
        assert!(frame_result.is_err());
        match frame_result.err().unwrap() {
            CaptureError::PermissionDenied => {} // Pass
            other => panic!("Expected PermissionDenied, got {:?}", other),
        }
    }

    #[test]
    fn test_mock_capturer_dimensions() {
        let capturer = MockScreenCapturer::new(1920, 1080, PixelFormat::Rgba);
        let dims = capturer.dimensions().unwrap();
        assert_eq!(dims, (1920, 1080));
    }

    #[test]
    fn test_frame_to_rgb_bgra_swaps_channels() {
        // BGRA pixel B=10,G=20,R=30,A=255 → RGB 30,20,10.
        let frame = Frame {
            width: 1,
            height: 1,
            format: PixelFormat::Bgra,
            data: vec![10, 20, 30, 255],
        };
        let (w, h, rgb) = frame_to_rgb(&frame);
        assert_eq!((w, h), (1, 1));
        assert_eq!(rgb, vec![30, 20, 10]);
    }

    #[test]
    fn test_region_rgb_crop_and_clamp() {
        // 4x4 RGB frame where each pixel encodes (R=x, G=y, B=0).
        let (w, h) = (4u32, 4u32);
        let mut data = Vec::new();
        for y in 0..h {
            for x in 0..w {
                data.extend_from_slice(&[x as u8, y as u8, 0]);
            }
        }
        let frame = Frame {
            width: w,
            height: h,
            format: PixelFormat::Rgb,
            data,
        };

        // 2x2 crop centred on (1,1) → covers x∈{0,1}, y∈{0,1}.
        let (cw, ch, rgb) = region_rgb(&frame, 1, 1, 2, 2);
        assert_eq!((cw, ch), (2, 2));
        assert_eq!(&rgb[0..3], &[0, 0, 0]); // (0,0)
        assert_eq!(&rgb[3..6], &[1, 0, 0]); // (1,0)
        assert_eq!(&rgb[6..9], &[0, 1, 0]); // (0,1)

        // Crop larger than the frame clamps to the full frame.
        let (cw2, ch2, _) = region_rgb(&frame, 2, 2, 100, 100);
        assert_eq!((cw2, ch2), (4, 4));

        // Cursor near the bottom-right edge clamps the origin.
        let (cw3, ch3, rgb3) = region_rgb(&frame, 3, 3, 2, 2);
        assert_eq!((cw3, ch3), (2, 2));
        assert_eq!(&rgb3[0..3], &[2, 2, 0]); // top-left of the clamped crop is (2,2)
    }

    #[test]
    fn test_clamp_rgb_dimensions_within_bounds() {
        let dummy = vec![128; 100 * 100 * 3];
        let (w, h, out) = clamp_rgb_dimensions(&dummy, 100, 100, 1920, 1080);
        assert_eq!((w, h), (100, 100));
        assert_eq!(out.len(), 100 * 100 * 3);
        assert_eq!(out[0], 128);
    }

    #[test]
    fn test_clamp_rgb_dimensions_downscales_4k() {
        // 4K 3840x2160 -> should scale down to 1920x1080 (factor 0.5)
        let dummy = vec![200; 3840 * 2160 * 3];
        let (w, h, out) = clamp_rgb_dimensions(&dummy, 3840, 2160, 1920, 1080);
        assert_eq!((w, h), (1920, 1080));
        assert_eq!(out.len(), 1920 * 1080 * 3);
        assert_eq!(out[0], 200);
    }

    #[test]
    fn test_crop_frame_bbox_rgb_extracts_correct_subregion() {
        let w = 10u32;
        let h = 10u32;
        let mut data = vec![0u8; (w * h * 4) as usize];
        // Write RGBA pixel at (x=3, y=4): R=100, G=150, B=200, A=255
        let idx = (4 * w as usize + 3) * 4;
        data[idx] = 100;
        data[idx + 1] = 150;
        data[idx + 2] = 200;
        data[idx + 3] = 255;

        let frame = Frame {
            width: w,
            height: h,
            format: PixelFormat::Rgba,
            data,
        };

        let bbox = BoundingBox {
            x: 2,
            y: 3,
            width: 3,
            height: 3,
        };
        let (cw, ch, rgb) = crop_frame_bbox_rgb(&frame, &bbox);
        assert_eq!((cw, ch), (3, 3));
        assert_eq!(rgb.len(), 3 * 3 * 3);
        // In the 3x3 crop (x: 2..5, y: 3..6), the modified pixel (3,4) is at crop relative (1,1)
        let crop_idx = (3 + 1) * 3;
        assert_eq!(&rgb[crop_idx..crop_idx + 3], &[100, 150, 200]);
    }

    #[test]
    fn test_process_vision_frame_initial_frame_falls_back_to_720p() {
        let frame = Frame {
            width: 1920,
            height: 1080,
            format: PixelFormat::Rgba,
            data: vec![200; 1920 * 1080 * 4],
        };
        let (w, h, rgb, patch) = process_vision_frame(&frame, None).unwrap();
        assert_eq!((w, h), (1280, 720));
        assert_eq!(rgb.len(), 1280 * 720 * 3);
        assert!(patch.is_none());
    }

    #[test]
    fn test_process_vision_frame_small_diff_crops_roi_with_16px_padding() {
        let w = 1920u32;
        let h = 1080u32;
        let frame_a = Frame {
            width: w,
            height: h,
            format: PixelFormat::Rgba,
            data: vec![100; (w * h * 4) as usize],
        };
        let mut frame_b = frame_a.clone();
        // Modify a 100x100 box at (x: 500, y: 400)
        // Area ratio = 10,000 / 2,073,600 = 0.48% (<< 35%)
        for y in 400..500 {
            for x in 500..600 {
                let idx = (y * w as usize + x) * 4;
                frame_b.data[idx] = 200;
            }
        }

        let (cw, ch, rgb, patch_opt) = process_vision_frame(&frame_b, Some(&frame_a)).unwrap();
        assert!(patch_opt.is_some());
        let patch = patch_opt.unwrap();
        assert!(!patch.is_co_scaled);
        // Raw box is 100x100, with 16px padding on all sides -> 132x132
        assert_eq!(patch.padded_bounding_box.width, 132);
        assert_eq!(patch.padded_bounding_box.height, 132);
        assert_eq!(cw, 132);
        assert_eq!(ch, 132);
        assert_eq!(rgb.len(), 132 * 132 * 3);

        // Verify visual tokens are well within 144 - 384 bounds
        let tokens = ((cw as f32 / 28.0).ceil() * (ch as f32 / 28.0).ceil()) as usize;
        assert!(tokens <= 384, "Tokens {} exceeded 384", tokens);
    }

    #[test]
    fn test_process_vision_frame_large_diff_triggers_720p_co_scale() {
        let w = 1920u32;
        let h = 1080u32;
        let frame_a = Frame {
            width: w,
            height: h,
            format: PixelFormat::Rgba,
            data: vec![100; (w * h * 4) as usize],
        };
        let mut frame_b = frame_a.clone();
        // Modify 1920 x 800 box -> area ratio = 74% (> 35%)
        for y in 100..900 {
            for x in 0..1920 {
                let idx = (y * w as usize + x) * 4;
                frame_b.data[idx] = 200;
            }
        }

        let (cw, ch, rgb, patch_opt) = process_vision_frame(&frame_b, Some(&frame_a)).unwrap();
        assert!(patch_opt.is_some());
        let patch = patch_opt.unwrap();
        assert!(patch.is_co_scaled);
        assert_eq!((cw, ch), (1280, 720));
        assert_eq!(rgb.len(), 1280 * 720 * 3);
    }

    #[test]
    fn test_process_vision_frame_identical_frames_fallback_to_720p() {
        let w = 1920u32;
        let h = 1080u32;
        let frame_a = Frame {
            width: w,
            height: h,
            format: PixelFormat::Rgba,
            data: vec![100; (w * h * 4) as usize],
        };
        let frame_b = frame_a.clone();

        let (cw, ch, rgb, patch_opt) = process_vision_frame(&frame_b, Some(&frame_a)).unwrap();
        assert!(patch_opt.is_none());
        assert_eq!((cw, ch), (1280, 720));
        assert_eq!(rgb.len(), 1280 * 720 * 3);
    }

    #[test]
    fn test_capture_for_vision_from_mock_capturer_sequence() {
        let slot = Mutex::new(None);
        let capturer = MockScreenCapturer::new(1920, 1080, PixelFormat::Rgba);

        // Call 1: Initial frame
        capturer.set_frame_data(vec![100; 1920 * 1080 * 4]);
        let (w1, h1, _rgb1, patch1) = capture_for_vision_from(&capturer, &slot).unwrap();
        assert_eq!((w1, h1), (1280, 720));
        assert!(patch1.is_none());

        // Call 2: Small change at (x:100..200, y:100..200)
        let mut data2 = vec![100; 1920 * 1080 * 4];
        for y in 100..200 {
            for x in 100..200 {
                data2[(y * 1920 + x) * 4] = 250;
            }
        }
        capturer.set_frame_data(data2);
        let (w2, h2, _rgb2, patch2) = capture_for_vision_from(&capturer, &slot).unwrap();
        assert!(patch2.is_some());
        let p2 = patch2.unwrap();
        assert!(!p2.is_co_scaled);
        assert_eq!(w2, 132); // 100 + 32 padding
        assert_eq!(h2, 132);

        let (cx, cy) = p2.center_normalized(1920, 1080);
        assert!((cx - 150.0 / 1920.0).abs() < 0.01);
        assert!((cy - 150.0 / 1080.0).abs() < 0.01);
    }

    #[test]
    fn test_clamp_to_visual_tokens_guarantees_ceiling_384() {
        let dummy = vec![128; 700 * 700 * 3];
        let (w, h, out) = clamp_to_visual_tokens(&dummy, 700, 700, 384);
        let tokens = ((w as f32 / 28.0).ceil() * (h as f32 / 28.0).ceil()) as usize;
        assert!(tokens <= 384, "Tokens {} exceeded 384 ceiling", tokens);
        assert_eq!((w, h), (532, 532));
        assert_eq!(out.len(), 532 * 532 * 3);

        // Adversarial 720x720 square crop
        let dummy_720 = vec![128; 720 * 720 * 3];
        let (w720, h720, _) = clamp_to_visual_tokens(&dummy_720, 720, 720, 384);
        let tokens_720 = ((w720 as f32 / 28.0).ceil() * (h720 as f32 / 28.0).ceil()) as usize;
        assert!(
            tokens_720 <= 384,
            "Tokens {} exceeded 384 ceiling",
            tokens_720
        );
        assert_eq!((w720, h720), (532, 532));

        // Extreme wide aspect ratio (1920x200)
        let dummy_wide = vec![128; 1920 * 200 * 3];
        let (ww, wh, _) = clamp_to_visual_tokens(&dummy_wide, 1920, 200, 384);
        let tokens_wide = ((ww as f32 / 28.0).ceil() * (wh as f32 / 28.0).ceil()) as usize;
        assert!(
            tokens_wide <= 384,
            "Wide tokens {} exceeded 384 ceiling",
            tokens_wide
        );
    }

    #[test]
    fn test_prev_vision_frame_poison_resilience() {
        let slot = Mutex::new(None);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = slot.lock().unwrap();
            panic!("simulated thread panic while holding lock");
        }));
        assert!(slot.is_poisoned());

        let capturer = MockScreenCapturer::new(1920, 1080, PixelFormat::Rgba);
        capturer.set_frame_data(vec![100; 1920 * 1080 * 4]);
        let res = capture_for_vision_from(&capturer, &slot);
        assert!(res.is_ok(), "Should recover from poisoned mutex");
    }
}
