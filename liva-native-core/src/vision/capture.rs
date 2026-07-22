use std::fmt;

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

/// Convert a whole frame to tightly-packed RGB (`w*h*3`).
pub fn frame_to_rgb(frame: &Frame) -> (u32, u32, Vec<u8>) {
    let rgb: Vec<u8> = match frame.format {
        PixelFormat::Rgba => frame.data.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect(),
        PixelFormat::Bgra => frame.data.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0]]).collect(),
        PixelFormat::Rgb => frame.data.clone(),
        PixelFormat::Bgr => frame.data.chunks_exact(3).flat_map(|p| [p[2], p[1], p[0]]).collect(),
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
                    out.extend_from_slice(&[frame.data[idx + 2], frame.data[idx + 1], frame.data[idx]]);
                } else {
                    out.extend_from_slice(&[frame.data[idx], frame.data[idx + 1], frame.data[idx + 2]]);
                }
            } else {
                out.extend_from_slice(&[0, 0, 0]);
            }
        }
    }
    (w as u32, h as u32, out)
}

/// Capture an image for a vision request, context-aware so the footprint stays
/// small while a game is running. Region selected by `LIVA_VISION_REGION`:
///   - `full`   → the whole primary screen,
///   - `cursor` → a crop around the mouse (`LIVA_VISION_CROP` px, default 512),
///   - `auto`   → cursor crop when a fullscreen game is foreground, else full.
///
/// Returns `(width, height, RGB)`. Runs on the calling (blocking) thread.
pub fn capture_for_vision() -> Result<(u32, u32, Vec<u8>), String> {
    let frame = NativeScreenCapturer::new(0)
        .capture()
        .map_err(|e| format!("screen capture: {}", e))?;
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
        if let Some((cx, cy)) = cursor_position() {
            tracing::debug!("vision: mouse-guided crop {}px at ({},{})", crop, cx, cy);
            return Ok(region_rgb(&frame, cx, cy, crop, crop));
        }
    }
    Ok(frame_to_rgb(&frame))
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
        Self {
            display_id,
        }
    }

    fn get_monitor(&self) -> Result<xcap::Monitor, CaptureError> {
        CACHED_MONITOR.with(|cache| {
            let mut cache = cache.borrow_mut();
            if let Some(monitor) = cache.get(&self.display_id) {
                return Ok(monitor.clone());
            }
            let monitors = xcap::Monitor::all().map_err(|e| CaptureError::OsError(e.to_string()))?;
            let found = monitors.iter()
                .find(|m| m.id().ok() == Some(self.display_id))
                .cloned()
                .or_else(|| monitors.get(self.display_id as usize).cloned())
                .ok_or(CaptureError::DisplayNotFound)?;
            cache.insert(self.display_id, found.clone());
            Ok(found)
        })
    }

    fn invalidate_cache(&self) {
        let _ = CACHED_MONITOR.with(|cache| {
            cache.borrow_mut().remove(&self.display_id)
        });
    }
}

impl ScreenCapturer for NativeScreenCapturer {
    fn capture(&self) -> Result<Frame, CaptureError> {
        let monitor = self.get_monitor()?;
        match monitor.capture_image() {
            Ok(image) => {
                Ok(Frame {
                    width: image.width(),
                    height: image.height(),
                    format: PixelFormat::Rgba,
                    data: image.into_raw(),
                })
            }
            Err(_) => {
                self.invalidate_cache();
                let monitor = self.get_monitor()?;
                let image = monitor.capture_image().map_err(|e| CaptureError::HardwareError(e.to_string()))?;
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
                monitor.width().map_err(|e| CaptureError::HardwareError(e.to_string()))?
            }
        };
        let h = match monitor.height() {
            Ok(h) => h,
            Err(_) => {
                self.invalidate_cache();
                let monitor = self.get_monitor()?;
                monitor.height().map_err(|e| CaptureError::HardwareError(e.to_string()))?
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
        *self.next_frame_data.lock().unwrap() = data;
    }

    pub fn set_fail_with(&self, err: Option<CaptureError>) {
        *self.fail_with.lock().unwrap() = err;
    }
}

impl ScreenCapturer for MockScreenCapturer {
    fn capture(&self) -> Result<Frame, CaptureError> {
        if let Some(err) = self.fail_with.lock().unwrap().clone() {
            return Err(err);
        }
        Ok(Frame {
            width: self.width,
            height: self.height,
            format: self.format.clone(),
            data: self.next_frame_data.lock().unwrap().clone(),
        })
    }

    fn dimensions(&self) -> Result<(u32, u32), CaptureError> {
        if let Some(err) = self.fail_with.lock().unwrap().clone() {
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
        let frame = Frame { width: 1, height: 1, format: PixelFormat::Bgra, data: vec![10, 20, 30, 255] };
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
        let frame = Frame { width: w, height: h, format: PixelFormat::Rgb, data };

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
}
