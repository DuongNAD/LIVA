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
}
