//! In-memory mock capturer for headless, deterministic testing.

use super::traits::ScreenCapturerTrait;
use super::types::{CuaPixelFormat, RawFrame};
use crate::types::CuaError;
use async_trait::async_trait;
use std::sync::Mutex;

/// Thread-safe in-memory screen capturer for deterministic tests.
pub struct MockScreenCapturer {
    pub width: u32,
    pub height: u32,
    pub format: CuaPixelFormat,
    frames: Mutex<Vec<RawFrame>>,
    default_pixel_val: u8,
}

impl MockScreenCapturer {
    /// Create a new mock capturer with specified geometry and default fill pixel value.
    pub fn new(width: u32, height: u32, format: CuaPixelFormat, default_val: u8) -> Self {
        Self {
            width,
            height,
            format,
            frames: Mutex::new(Vec::new()),
            default_pixel_val: default_val,
        }
    }

    /// Enqueue a specific frame to be returned on the next capture call.
    pub fn enqueue_frame(&self, frame: RawFrame) {
        self.frames.lock().unwrap().push(frame);
    }

    /// Clear all enqueued frames.
    pub fn clear_enqueued_frames(&self) {
        self.frames.lock().unwrap().clear();
    }

    /// Helper to synthesize a uniform frame.
    pub fn create_uniform_frame(&self, val: u8) -> RawFrame {
        let size = (self.width * self.height * self.format.bytes_per_pixel() as u32) as usize;
        RawFrame {
            width: self.width,
            height: self.height,
            format: self.format,
            data: vec![val; size],
        }
    }

    /// Helper to synthesize a frame with a modified rectangular patch.
    pub fn create_frame_with_rect(
        &self,
        base_val: u8,
        patch_val: u8,
        rx: u32,
        ry: u32,
        rw: u32,
        rh: u32,
    ) -> RawFrame {
        let mut frame = self.create_uniform_frame(base_val);
        let bpp = self.format.bytes_per_pixel();
        let stride = self.width as usize * bpp;

        let end_y = (ry + rh).min(self.height);
        let end_x = (rx + rw).min(self.width);

        for y in ry..end_y {
            for x in rx..end_x {
                let off = (y as usize * stride) + (x as usize * bpp);
                for c in 0..bpp {
                    frame.data[off + c] = patch_val;
                }
            }
        }
        frame
    }
}

#[async_trait]
impl ScreenCapturerTrait for MockScreenCapturer {
    async fn capture(&self) -> Result<RawFrame, CuaError> {
        let mut lock = self.frames.lock().unwrap();
        if !lock.is_empty() {
            Ok(lock.remove(0))
        } else {
            Ok(self.create_uniform_frame(self.default_pixel_val))
        }
    }

    fn dimensions(&self) -> Result<(u32, u32), CuaError> {
        Ok((self.width, self.height))
    }
}
