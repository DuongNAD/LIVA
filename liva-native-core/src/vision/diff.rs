use super::capture::Frame;
use serde::{Deserialize, Serialize};

/// Represents a rectangular region containing changed pixels on the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundingBox {
    /// The X-coordinate (left) of the bounding box.
    pub x: usize,
    /// The Y-coordinate (top) of the bounding box.
    pub y: usize,
    /// The width of the bounding box in pixels.
    pub width: usize,
    /// The height of the bounding box in pixels.
    pub height: usize,
}

/// Error types representing invalid inputs or buffer mismatches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffError {
    /// The screen width or height is zero.
    InvalidDimensions,
    /// The stride is smaller than the screen width.
    InvalidStride,
    /// Frame A's buffer is smaller than the required dimensions.
    BufferTooSmallFrameA { expected: usize, actual: usize },
    /// Frame B's buffer is smaller than the required dimensions.
    BufferTooSmallFrameB { expected: usize, actual: usize },
    /// Byte buffer alignment / cast mismatch.
    AlignmentError,
}

impl std::fmt::Display for DiffError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiffError::InvalidDimensions => write!(f, "Invalid dimensions"),
            DiffError::InvalidStride => write!(f, "Invalid stride"),
            DiffError::BufferTooSmallFrameA { expected, actual } => {
                write!(
                    f,
                    "Frame A buffer too small. Expected {}, got {}",
                    expected, actual
                )
            }
            DiffError::BufferTooSmallFrameB { expected, actual } => {
                write!(
                    f,
                    "Frame B buffer too small. Expected {}, got {}",
                    expected, actual
                )
            }
            DiffError::AlignmentError => write!(f, "Alignment error casting byte buffer"),
        }
    }
}

impl std::error::Error for DiffError {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScreenRegion {
    pub id: String,
    pub name: String,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub threshold: f32, // Changed pixel ratio threshold (0.0 to 1.0)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RegionDiffResult {
    pub region_id: String,
    pub name: String,
    pub difference: f32, // Ratio of changed pixels [0.0 - 1.0]
    pub is_changed: bool,
}

/// Compares two raw frames and returns the bounding box enclosing all changed pixels.
///
/// * `frame_a` - Slice containing pixels of the first frame.
/// * `frame_b` - Slice containing pixels of the second frame.
/// * `width` - Active width of the screen in pixels.
/// * `height` - Active height of the screen in pixels.
/// * `stride` - Number of pixel elements per row (including alignment padding).
#[inline]
#[allow(clippy::collapsible_if)]
fn search_left<T: Eq>(row_a: &[T], row_b: &[T], x_min_global: &mut usize) {
    if *x_min_global == 0 {
        return;
    }
    let left_slice_a = &row_a[0..*x_min_global];
    let left_slice_b = &row_b[0..*x_min_global];
    if left_slice_a != left_slice_b {
        if let Some(pos) = left_slice_a
            .iter()
            .zip(left_slice_b.iter())
            .position(|(a, b)| a != b)
        {
            *x_min_global = pos;
        }
    }
}

#[inline]
#[allow(clippy::collapsible_if)]
fn search_right<T: Eq>(row_a: &[T], row_b: &[T], width: usize, x_max_global: &mut usize) {
    if *x_max_global >= width - 1 {
        return;
    }
    let right_slice_a = &row_a[*x_max_global + 1..width];
    let right_slice_b = &row_b[*x_max_global + 1..width];
    if right_slice_a != right_slice_b {
        if let Some(pos) = right_slice_a
            .iter()
            .zip(right_slice_b.iter())
            .rposition(|(a, b)| a != b)
        {
            *x_max_global = (*x_max_global + 1) + pos;
        }
    }
}

pub fn find_changes<T>(
    frame_a: &[T],
    frame_b: &[T],
    width: usize,
    height: usize,
    stride: usize,
) -> Result<Option<BoundingBox>, DiffError>
where
    T: Eq + Copy,
{
    // 1. Inputs Validation
    if width == 0 || height == 0 {
        return Err(DiffError::InvalidDimensions);
    }
    if stride < width {
        return Err(DiffError::InvalidStride);
    }

    // Ensure we do not overflow memory boundaries
    let required_len = match (height - 1).checked_mul(stride) {
        Some(val) => match val.checked_add(width) {
            Some(len) => len,
            None => return Err(DiffError::InvalidDimensions),
        },
        None => return Err(DiffError::InvalidDimensions),
    };

    if frame_a.len() < required_len {
        return Err(DiffError::BufferTooSmallFrameA {
            expected: required_len,
            actual: frame_a.len(),
        });
    }
    if frame_b.len() < required_len {
        return Err(DiffError::BufferTooSmallFrameB {
            expected: required_len,
            actual: frame_b.len(),
        });
    }

    // 2. Identity Fast-Path
    if stride == width {
        if frame_a[..required_len] == frame_b[..required_len] {
            return Ok(None);
        }
    } else {
        let mut identical = true;
        for y in 0..height {
            let start = y * stride;
            let end = start + width;
            if frame_a[start..end] != frame_b[start..end] {
                identical = false;
                break;
            }
        }
        if identical {
            return Ok(None);
        }
    }

    // 3. Row-wise Vertical Scan (Top-down)
    let mut y_min = 0;
    for y in 0..height {
        let start = y * stride;
        let end = start + width;
        if frame_a[start..end] != frame_b[start..end] {
            y_min = y;
            break;
        }
    }

    // Row-wise Vertical Scan (Bottom-up)
    let mut y_max = height - 1;
    for y in (y_min..height).rev() {
        let start = y * stride;
        let end = start + width;
        if frame_a[start..end] != frame_b[start..end] {
            y_max = y;
            break;
        }
    }

    // 4. Narrowing Horizontal Scan
    let mut x_min_global = width - 1;
    let mut x_max_global = 0;

    for y in y_min..=y_max {
        let start = y * stride;
        let row_a = &frame_a[start..start + width];
        let row_b = &frame_b[start..start + width];

        search_left(row_a, row_b, &mut x_min_global);
        search_right(row_a, row_b, width, &mut x_max_global);
    }

    Ok(Some(BoundingBox {
        x: x_min_global,
        y: y_min,
        width: x_max_global - x_min_global + 1,
        height: y_max - y_min + 1,
    }))
}

/// Specialized wrapper for 32-bit BGRA/RGBA screen buffers.
pub fn find_changes_u32(
    frame_a: &[u8],
    frame_b: &[u8],
    width: usize,
    height: usize,
    stride_bytes: usize,
) -> Result<Option<BoundingBox>, DiffError> {
    if !stride_bytes.is_multiple_of(4) {
        return Err(DiffError::InvalidStride);
    }
    let bytes_per_pixel = 4;
    let stride_pixels = stride_bytes / bytes_per_pixel;

    let pixels_a =
        bytemuck::try_cast_slice::<u8, u32>(frame_a).map_err(|_| DiffError::AlignmentError)?;
    let pixels_b =
        bytemuck::try_cast_slice::<u8, u32>(frame_b).map_err(|_| DiffError::AlignmentError)?;

    find_changes(pixels_a, pixels_b, width, height, stride_pixels)
}

pub struct DiffEngine;

#[inline]
fn has_pixel_changed(
    prev_pixel: &[u8],
    curr_pixel: &[u8],
    bytes_per_pixel: usize,
    color_tolerance: u8,
) -> bool {
    for c in 0..bytes_per_pixel {
        let diff = (prev_pixel[c] as i16 - curr_pixel[c] as i16).abs();
        if diff > color_tolerance as i16 {
            return true;
        }
    }
    false
}

impl DiffEngine {
    /// Compares two frames within a specific ScreenRegion bounds.
    /// Employs pixel delta tolerance thresholding.
    pub fn diff_region(
        prev: &Frame,
        curr: &Frame,
        region: &ScreenRegion,
        color_tolerance: u8,
    ) -> Result<RegionDiffResult, String> {
        // Bounds checking against frame dimensions with checked arithmetic
        let x_end = (region.x as usize)
            .checked_add(region.width as usize)
            .ok_or_else(|| "Region x bounds overflow".to_string())?;
        if x_end > prev.width as usize {
            return Err(format!(
                "Region bounds [x: {}, y: {}, w: {}, h: {}] exceed previous frame dimensions [w: {}, h: {}]",
                region.x, region.y, region.width, region.height, prev.width, prev.height
            ));
        }
        if x_end > curr.width as usize {
            return Err(format!(
                "Region bounds [x: {}, y: {}, w: {}, h: {}] exceed current frame dimensions [w: {}, h: {}]",
                region.x, region.y, region.width, region.height, curr.width, curr.height
            ));
        }

        let y_end = (region.y as usize)
            .checked_add(region.height as usize)
            .ok_or_else(|| "Region y bounds overflow".to_string())?;
        if y_end > prev.height as usize {
            return Err(format!(
                "Region bounds [x: {}, y: {}, w: {}, h: {}] exceed previous frame dimensions [w: {}, h: {}]",
                region.x, region.y, region.width, region.height, prev.width, prev.height
            ));
        }
        if y_end > curr.height as usize {
            return Err(format!(
                "Region bounds [x: {}, y: {}, w: {}, h: {}] exceed current frame dimensions [w: {}, h: {}]",
                region.x, region.y, region.width, region.height, curr.width, curr.height
            ));
        }

        if prev.format != curr.format {
            return Err("Pixel format mismatch between frames".to_string());
        }

        let bytes_per_pixel = match prev.format {
            super::capture::PixelFormat::Rgb | super::capture::PixelFormat::Bgr => 3,
            super::capture::PixelFormat::Rgba | super::capture::PixelFormat::Bgra => 4,
        };

        // Frame buffer size validation with checked arithmetic
        let expected_prev_len = (prev.width as usize)
            .checked_mul(prev.height as usize)
            .and_then(|len| len.checked_mul(bytes_per_pixel))
            .ok_or_else(|| "Frame dimensions overflow memory limits".to_string())?;
        if prev.data.len() < expected_prev_len {
            return Err(format!(
                "Previous frame buffer too small. Expected {}, got {}",
                expected_prev_len,
                prev.data.len()
            ));
        }
        let expected_curr_len = (curr.width as usize)
            .checked_mul(curr.height as usize)
            .and_then(|len| len.checked_mul(bytes_per_pixel))
            .ok_or_else(|| "Frame dimensions overflow memory limits".to_string())?;
        if curr.data.len() < expected_curr_len {
            return Err(format!(
                "Current frame buffer too small. Expected {}, got {}",
                expected_curr_len,
                curr.data.len()
            ));
        }

        let mut changed_pixels = 0;
        let total_pixels = region.width as usize * region.height as usize;
        if total_pixels == 0 {
            return Ok(RegionDiffResult {
                region_id: region.id.clone(),
                name: region.name.clone(),
                difference: 0.0,
                is_changed: false,
            });
        }

        for row in 0..region.height {
            let py = region.y + row;
            let prev_row_offset = (py * prev.width + region.x) as usize * bytes_per_pixel;
            let curr_row_offset = (py * curr.width + region.x) as usize * bytes_per_pixel;

            // Fast path: if the row segments are identical, skip checking individual pixels
            let row_len = region.width as usize * bytes_per_pixel;
            if prev.data[prev_row_offset..prev_row_offset + row_len]
                == curr.data[curr_row_offset..curr_row_offset + row_len]
            {
                continue;
            }

            for col in 0..region.width {
                let p_off = prev_row_offset + (col as usize * bytes_per_pixel);
                let c_off = curr_row_offset + (col as usize * bytes_per_pixel);

                let prev_pixel = &prev.data[p_off..p_off + bytes_per_pixel];
                let curr_pixel = &curr.data[c_off..c_off + bytes_per_pixel];

                if has_pixel_changed(prev_pixel, curr_pixel, bytes_per_pixel, color_tolerance) {
                    changed_pixels += 1;
                }
            }
        }

        let difference = changed_pixels as f32 / total_pixels as f32;
        let is_changed = difference >= region.threshold;

        Ok(RegionDiffResult {
            region_id: region.id.clone(),
            name: region.name.clone(),
            difference,
            is_changed,
        })
    }

    /// Extracts the visual region-of-interest between two frames.
    /// If changes exceed 35% of total screen area, flags for emergency Co-scale Fallback.
    pub fn compute_roi_patch(
        prev: &Frame,
        curr: &Frame,
        padding: usize,
    ) -> Result<Option<VisualRoiPatch>, String> {
        if prev.width != curr.width || prev.height != curr.height {
            return Err("Frame dimension mismatch".to_string());
        }
        let w = curr.width as usize;
        let h = curr.height as usize;

        let bbox_opt = match curr.format {
            crate::vision::capture::PixelFormat::Bgra
            | crate::vision::capture::PixelFormat::Rgba => {
                find_changes_u32(&prev.data, &curr.data, w, h, w * 4).map_err(|e| e.to_string())?
            }
            crate::vision::capture::PixelFormat::Rgb | crate::vision::capture::PixelFormat::Bgr => {
                find_changes(&prev.data, &curr.data, w * 3, h, w * 3)
                    .map_err(|e| e.to_string())?
                    .map(|b| BoundingBox {
                        x: b.x / 3,
                        y: b.y,
                        width: (b.width / 3).max(1),
                        height: b.height,
                    })
            }
        };

        let Some(raw_box) = bbox_opt else {
            return Ok(None);
        };

        let total_pixels = (w * h) as f32;
        let area_ratio = (raw_box.width * raw_box.height) as f32 / total_pixels;
        let is_co_scaled = should_co_scale(&raw_box, w, h);
        let padded_box = apply_roi_padding(&raw_box, w, h, padding);

        Ok(Some(VisualRoiPatch {
            raw_bounding_box: raw_box,
            padded_bounding_box: padded_box,
            is_co_scaled,
            width: if is_co_scaled {
                1280.min(w)
            } else {
                padded_box.width
            },
            height: if is_co_scaled {
                720.min(h)
            } else {
                padded_box.height
            },
            area_ratio,
        }))
    }
}

pub const CO_SCALE_AREA_THRESHOLD: f64 = 0.35;
pub const DEFAULT_ROI_PADDING: usize = 16;

/// Applies protective border padding around a bounding box without exceeding screen boundaries.
pub fn apply_roi_padding(
    bbox: &BoundingBox,
    screen_w: usize,
    screen_h: usize,
    padding: usize,
) -> BoundingBox {
    let x = bbox.x.saturating_sub(padding);
    let y = bbox.y.saturating_sub(padding);
    let right = (bbox.x + bbox.width + padding).min(screen_w);
    let bottom = (bbox.y + bbox.height + padding).min(screen_h);
    BoundingBox {
        x,
        y,
        width: right.saturating_sub(x).max(1),
        height: bottom.saturating_sub(y).max(1),
    }
}

/// Determines if the bounding box exceeds the 35% threshold of screen area, warranting a Co-scale Fallback.
pub fn should_co_scale(bbox: &BoundingBox, screen_w: usize, screen_h: usize) -> bool {
    if screen_w == 0 || screen_h == 0 {
        return false;
    }
    let total_pixels = (screen_w * screen_h) as f64;
    let bbox_pixels = (bbox.width * bbox.height) as f64;
    (bbox_pixels / total_pixels) > CO_SCALE_AREA_THRESHOLD
}

/// Fast downsampling of RGB byte buffer to standard 720p resolution (max 1280x720).
/// Keeps visual tokens <= 384 and TTFT inference within 250-380 ms.
pub fn downsample_rgb_to_720p(data: &[u8], src_w: usize, src_h: usize) -> (usize, usize, Vec<u8>) {
    if src_w <= 1280 && src_h <= 720 {
        return (src_w, src_h, data.to_vec());
    }
    let scale_x = src_w as f32 / 1280.0;
    let scale_y = src_h as f32 / 720.0;
    let scale = scale_x.max(scale_y).max(1.0);

    let dst_w = ((src_w as f32 / scale).round() as usize).max(1);
    let dst_h = ((src_h as f32 / scale).round() as usize).max(1);

    let mut dst_data = vec![0u8; dst_w * dst_h * 3];

    for dy in 0..dst_h {
        let sy = ((dy as f32 * scale) as usize).min(src_h - 1);
        for dx in 0..dst_w {
            let sx = ((dx as f32 * scale) as usize).min(src_w - 1);
            let src_idx = (sy * src_w + sx) * 3;
            let dst_idx = (dy * dst_w + dx) * 3;
            if src_idx + 2 < data.len() && dst_idx + 2 < dst_data.len() {
                dst_data[dst_idx] = data[src_idx];
                dst_data[dst_idx + 1] = data[src_idx + 1];
                dst_data[dst_idx + 2] = data[src_idx + 2];
            }
        }
    }

    (dst_w, dst_h, dst_data)
}

/// Information about a cropped or co-scaled Visual ROI patch prepared for VLM inference.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VisualRoiPatch {
    pub raw_bounding_box: BoundingBox,
    pub padded_bounding_box: BoundingBox,
    pub is_co_scaled: bool,
    pub width: usize,
    pub height: usize,
    pub area_ratio: f32,
}

impl VisualRoiPatch {
    pub fn center_normalized(&self, screen_w: usize, screen_h: usize) -> (f32, f32) {
        if screen_w == 0 || screen_h == 0 {
            return (0.5, 0.5);
        }
        let cx = (self.raw_bounding_box.x as f32 + self.raw_bounding_box.width as f32 * 0.5)
            / screen_w as f32;
        let cy = (self.raw_bounding_box.y as f32 + self.raw_bounding_box.height as f32 * 0.5)
            / screen_h as f32;
        (cx.clamp(0.0, 1.0), cy.clamp(0.0, 1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vision::capture::PixelFormat;

    fn create_test_frame(w: u32, h: u32, val: u8) -> Frame {
        let size = (w * h * 3) as usize;
        Frame {
            width: w,
            height: h,
            format: PixelFormat::Rgb,
            data: vec![val; size],
        }
    }

    #[test]
    fn test_find_changes_no_change() {
        let a = vec![100; 16];
        let b = vec![100; 16];
        let res = find_changes(&a, &b, 4, 4, 4).unwrap();
        assert_eq!(res, None);
    }

    #[test]
    fn test_find_changes_single_pixel() {
        let a = vec![100; 16];
        let mut b = vec![100; 16];
        b[5] = 200; // row 1, col 1 (0-indexed)
        let res = find_changes(&a, &b, 4, 4, 4).unwrap();
        assert_eq!(
            res,
            Some(BoundingBox {
                x: 1,
                y: 1,
                width: 1,
                height: 1
            })
        );
    }

    #[test]
    fn test_find_changes_multiple_pixels() {
        let a = vec![100; 16];
        let mut b = vec![100; 16];
        b[1] = 200; // row 0, col 1
        b[10] = 200; // row 2, col 2
        let res = find_changes(&a, &b, 4, 4, 4).unwrap();
        assert_eq!(
            res,
            Some(BoundingBox {
                x: 1,
                y: 0,
                width: 2,
                height: 3
            })
        );
    }

    #[test]
    fn test_diff_region_no_change() {
        let prev = create_test_frame(10, 10, 100);
        let curr = create_test_frame(10, 10, 100);
        let region = ScreenRegion {
            id: "r1".to_string(),
            name: "Test Region".to_string(),
            x: 2,
            y: 2,
            width: 5,
            height: 5,
            threshold: 0.1,
        };

        let result = DiffEngine::diff_region(&prev, &curr, &region, 5).unwrap();
        assert_eq!(result.difference, 0.0);
        assert!(!result.is_changed);
    }

    #[test]
    fn test_diff_region_complete_change() {
        let prev = create_test_frame(10, 10, 100);
        let curr = create_test_frame(10, 10, 200); // 100 delta > tolerance 5
        let region = ScreenRegion {
            id: "r1".to_string(),
            name: "Test Region".to_string(),
            x: 0,
            y: 0,
            width: 10,
            height: 10,
            threshold: 0.5,
        };

        let result = DiffEngine::diff_region(&prev, &curr, &region, 5).unwrap();
        assert_eq!(result.difference, 1.0);
        assert!(result.is_changed);
    }

    #[test]
    fn test_diff_region_partial_change_below_threshold() {
        let prev = create_test_frame(10, 10, 100);
        let mut curr = create_test_frame(10, 10, 100);

        // Modify exactly 1 pixel inside a 5x5 region (1/25 = 4% change)
        let pixel_idx = (2 * 10 + 2) * 3;
        curr.data[pixel_idx] = 200;

        let region = ScreenRegion {
            id: "r1".to_string(),
            name: "Test Region".to_string(),
            x: 2,
            y: 2,
            width: 5,
            height: 5,
            threshold: 0.10,
        };

        let result = DiffEngine::diff_region(&prev, &curr, &region, 5).unwrap();
        assert_eq!(result.difference, 0.04);
        assert!(!result.is_changed);
    }

    #[test]
    fn test_diff_region_partial_change_above_threshold() {
        let prev = create_test_frame(10, 10, 100);
        let mut curr = create_test_frame(10, 10, 100);

        for i in 0..4 {
            let pixel_idx = ((2 + i) * 10 + 2) * 3;
            curr.data[pixel_idx] = 200;
        }

        let region = ScreenRegion {
            id: "r1".to_string(),
            name: "Test Region".to_string(),
            x: 2,
            y: 2,
            width: 5,
            height: 5,
            threshold: 0.10,
        };

        let result = DiffEngine::diff_region(&prev, &curr, &region, 5).unwrap();
        assert_eq!(result.difference, 0.16);
        assert!(result.is_changed);
    }

    #[test]
    fn test_diff_region_color_tolerance() {
        let prev = create_test_frame(10, 10, 100);
        let mut curr = create_test_frame(10, 10, 100);

        let pixel_idx = (2 * 10 + 2) * 3;
        curr.data[pixel_idx] = 104;

        let region = ScreenRegion {
            id: "r1".to_string(),
            name: "Test Region".to_string(),
            x: 2,
            y: 2,
            width: 5,
            height: 5,
            threshold: 0.01,
        };

        let res_tol_5 = DiffEngine::diff_region(&prev, &curr, &region, 5).unwrap();
        assert_eq!(res_tol_5.difference, 0.0);

        let res_tol_2 = DiffEngine::diff_region(&prev, &curr, &region, 2).unwrap();
        assert_eq!(res_tol_2.difference, 0.04);
    }

    #[test]
    fn test_diff_region_out_of_bounds() {
        let prev = create_test_frame(10, 10, 100);
        let curr = create_test_frame(10, 10, 100);
        let region = ScreenRegion {
            id: "r1".to_string(),
            name: "Out of Bounds Region".to_string(),
            x: 8,
            y: 8,
            width: 5,
            height: 5,
            threshold: 0.1,
        };

        let res = DiffEngine::diff_region(&prev, &curr, &region, 5);
        assert!(res.is_err());
        assert!(
            res.err()
                .unwrap()
                .contains("exceed previous frame dimensions")
        );
    }

    #[test]
    fn test_diff_region_format_mismatch() {
        let prev = Frame {
            width: 10,
            height: 10,
            format: PixelFormat::Rgb,
            data: vec![0; 300],
        };
        let curr = Frame {
            width: 10,
            height: 10,
            format: PixelFormat::Rgba,
            data: vec![0; 400],
        };
        let region = ScreenRegion {
            id: "r1".to_string(),
            name: "Format Mismatch Region".to_string(),
            x: 0,
            y: 0,
            width: 5,
            height: 5,
            threshold: 0.1,
        };

        let res = DiffEngine::diff_region(&prev, &curr, &region, 5);
        assert!(res.is_err());
        assert!(res.err().unwrap().contains("format mismatch"));
    }

    #[test]
    fn test_find_changes_multi_region_overlap() {
        let width = 20;
        let height = 20;
        let stride = 20;
        let a = vec![100; width * height];
        let mut b = vec![100; width * height];
        // Modify two separate, non-overlapping regions
        // Region 1: x: 2..=3, y: 2..=3
        b[2 * stride + 2] = 200;
        b[3 * stride + 3] = 200;
        // Region 2: x: 8..=10, y: 8..=10
        b[8 * stride + 8] = 200;
        b[10 * stride + 10] = 200;

        let res = find_changes(&a, &b, width, height, stride).unwrap();
        // The bounding box enclosing both should span from x: 2 to 10, y: 2 to 10
        // width = 10 - 2 + 1 = 9, height = 10 - 2 + 1 = 9
        assert_eq!(
            res,
            Some(BoundingBox {
                x: 2,
                y: 2,
                width: 9,
                height: 9,
            })
        );
    }

    #[test]
    fn test_diff_engine_multi_region_overlaps() {
        let prev = create_test_frame(10, 10, 100);
        let mut curr = create_test_frame(10, 10, 100);

        // Region A: x:0, y:0, w:6, h:6
        let region_a = ScreenRegion {
            id: "ra".to_string(),
            name: "Region A".to_string(),
            x: 0,
            y: 0,
            width: 6,
            height: 6,
            threshold: 0.01,
        };
        // Region B: x:4, y:4, w:6, h:6
        let region_b = ScreenRegion {
            id: "rb".to_string(),
            name: "Region B".to_string(),
            x: 4,
            y: 4,
            width: 6,
            height: 6,
            threshold: 0.01,
        };

        // Case 1: Change inside overlap (e.g. at 5,5)
        let pixel_idx = (5 * 10 + 5) * 3;
        curr.data[pixel_idx] = 200;

        let res_a = DiffEngine::diff_region(&prev, &curr, &region_a, 5).unwrap();
        let res_b = DiffEngine::diff_region(&prev, &curr, &region_b, 5).unwrap();
        // Both should detect the change
        assert!(res_a.is_changed);
        assert!(res_b.is_changed);
        // Reset change
        curr.data[pixel_idx] = 100;

        // Case 2: Change inside Region A only (e.g. at 2,2)
        let pixel_idx_a = (2 * 10 + 2) * 3;
        curr.data[pixel_idx_a] = 200;
        let res_a = DiffEngine::diff_region(&prev, &curr, &region_a, 5).unwrap();
        let res_b = DiffEngine::diff_region(&prev, &curr, &region_b, 5).unwrap();
        assert!(res_a.is_changed);
        assert!(!res_b.is_changed);
        curr.data[pixel_idx_a] = 100;

        // Case 3: Change inside Region B only (e.g. at 8,8)
        let pixel_idx_b = (8 * 10 + 8) * 3;
        curr.data[pixel_idx_b] = 200;
        let res_a = DiffEngine::diff_region(&prev, &curr, &region_a, 5).unwrap();
        let res_b = DiffEngine::diff_region(&prev, &curr, &region_b, 5).unwrap();
        assert!(!res_a.is_changed);
        assert!(res_b.is_changed);
    }

    #[test]
    fn test_find_changes_massive_pixel_changes() {
        let width = 100;
        let height = 100;
        let stride = 100;
        let a = vec![100; width * height];
        let b = vec![200; width * height];

        let res = find_changes(&a, &b, width, height, stride).unwrap();
        assert_eq!(
            res,
            Some(BoundingBox {
                x: 0,
                y: 0,
                width,
                height,
            })
        );
    }

    #[test]
    fn test_find_changes_zero_changes() {
        let width = 100;
        let height = 100;
        let stride = 100;
        let a = vec![100; width * height];
        let b = vec![100; width * height];

        let res = find_changes(&a, &b, width, height, stride).unwrap();
        assert_eq!(res, None);
    }

    #[test]
    fn test_find_changes_border_only_changes() {
        let width = 10;
        let height = 10;
        let stride = 10;
        let a = vec![100; width * height];

        // 1. Single pixel at (0,0)
        let mut b = vec![100; width * height];
        b[0] = 200;
        assert_eq!(
            find_changes(&a, &b, width, height, stride).unwrap(),
            Some(BoundingBox {
                x: 0,
                y: 0,
                width: 1,
                height: 1
            })
        );

        // 2. Single pixel at (W-1, 0)
        let mut b = vec![100; width * height];
        b[width - 1] = 200;
        assert_eq!(
            find_changes(&a, &b, width, height, stride).unwrap(),
            Some(BoundingBox {
                x: width - 1,
                y: 0,
                width: 1,
                height: 1
            })
        );

        // 3. Single pixel at (0, H-1)
        let mut b = vec![100; width * height];
        b[(height - 1) * stride] = 200;
        assert_eq!(
            find_changes(&a, &b, width, height, stride).unwrap(),
            Some(BoundingBox {
                x: 0,
                y: height - 1,
                width: 1,
                height: 1
            })
        );

        // 4. Single pixel at (W-1, H-1)
        let mut b = vec![100; width * height];
        b[(height - 1) * stride + (width - 1)] = 200;
        assert_eq!(
            find_changes(&a, &b, width, height, stride).unwrap(),
            Some(BoundingBox {
                x: width - 1,
                y: height - 1,
                width: 1,
                height: 1
            })
        );

        // 5. Border corners (0,0) and (W-1, H-1)
        let mut b = vec![100; width * height];
        b[0] = 200;
        b[(height - 1) * stride + (width - 1)] = 200;
        assert_eq!(
            find_changes(&a, &b, width, height, stride).unwrap(),
            Some(BoundingBox {
                x: 0,
                y: 0,
                width,
                height
            })
        );
    }

    #[test]
    fn test_large_resolutions_4k_diffing() {
        let width = 3840;
        let height = 2160;
        let stride = width;

        // We use u32 to mock RGBA pixels
        let a = vec![0u32; width * height];

        // 1. Zero changes (fast-path)
        let b_zero = vec![0u32; width * height];
        let start = std::time::Instant::now();
        let res_zero = find_changes(&a, &b_zero, width, height, stride).unwrap();
        let duration_zero = start.elapsed();
        assert_eq!(res_zero, None);
        println!("4K Zero Change Scan duration: {:?}", duration_zero);

        // 2. Corner changes (0,0) and (W-1, H-1)
        let mut b_corners = vec![0u32; width * height];
        b_corners[0] = 1;
        b_corners[(height - 1) * stride + (width - 1)] = 1;
        let start = std::time::Instant::now();
        let res_corners = find_changes(&a, &b_corners, width, height, stride).unwrap();
        let duration_corners = start.elapsed();
        assert_eq!(
            res_corners,
            Some(BoundingBox {
                x: 0,
                y: 0,
                width,
                height
            })
        );
        println!("4K Corner Change Scan duration: {:?}", duration_corners);

        // 3. Center small change (100x100 box at center)
        let mut b_center = vec![0u32; width * height];
        let cx_start = width / 2 - 50;
        let cx_end = width / 2 + 50;
        let cy_start = height / 2 - 50;
        let cy_end = height / 2 + 50;
        for y in cy_start..cy_end {
            for x in cx_start..cx_end {
                b_center[y * stride + x] = 1;
            }
        }
        let start = std::time::Instant::now();
        let res_center = find_changes(&a, &b_center, width, height, stride).unwrap();
        let duration_center = start.elapsed();
        assert_eq!(
            res_center,
            Some(BoundingBox {
                x: cx_start,
                y: cy_start,
                width: cx_end - cx_start,
                height: cy_end - cy_start,
            })
        );
        println!("4K Center Box Change Scan duration: {:?}", duration_center);

        // 4. Massive full frame change
        let b_full = vec![1u32; width * height];
        let start = std::time::Instant::now();
        let res_full = find_changes(&a, &b_full, width, height, stride).unwrap();
        let duration_full = start.elapsed();
        assert_eq!(
            res_full,
            Some(BoundingBox {
                x: 0,
                y: 0,
                width,
                height
            })
        );
        println!("4K Full Frame Change Scan duration: {:?}", duration_full);
    }

    #[test]
    fn test_stride_greater_than_width() {
        let width = 5;
        let height = 3;
        let stride = 8; // Row padding present

        let a = vec![100; stride * height];
        let mut b = vec![100; stride * height];

        // Make a change inside the active area of row 1 (0-indexed) at col 2
        // offset: 1 * stride + 2 = 10
        b[10] = 200;

        // Make a change in the padding area of row 1 at col 6 (should be ignored)
        // offset: 1 * stride + 6 = 14
        b[14] = 200;

        let res = find_changes(&a, &b, width, height, stride).unwrap();
        assert_eq!(
            res,
            Some(BoundingBox {
                x: 2,
                y: 1,
                width: 1,
                height: 1
            })
        );
    }

    #[test]
    fn test_diff_region_buffer_too_small_safety() {
        let prev = Frame {
            width: 10,
            height: 10,
            format: PixelFormat::Rgb,
            data: vec![0; 50], // Too small (needs 300)
        };
        let curr = Frame {
            width: 10,
            height: 10,
            format: PixelFormat::Rgb,
            data: vec![0; 300],
        };
        let region = ScreenRegion {
            id: "r1".to_string(),
            name: "Test Region".to_string(),
            x: 0,
            y: 0,
            width: 5,
            height: 5,
            threshold: 0.1,
        };

        let res = DiffEngine::diff_region(&prev, &curr, &region, 5);
        assert!(res.is_err());
        assert!(
            res.err()
                .unwrap()
                .contains("Previous frame buffer too small")
        );
    }

    #[test]
    fn test_find_changes_u32_alignment_error() {
        let frame_a = vec![0u8; 103]; // Odd size and alignment
        let frame_b = vec![0u8; 103];
        let res = find_changes_u32(&frame_a, &frame_b, 5, 5, 20);
        assert!(res.is_err());
        assert_eq!(res.err().unwrap(), DiffError::AlignmentError);
    }

    #[test]
    fn test_diff_region_integer_overflows() {
        // Test 1: Overflowing frame dimensions
        let prev = Frame {
            width: u32::MAX,
            height: u32::MAX,
            format: PixelFormat::Rgb,
            data: vec![0; 10], // very small data to check if we bypass checks or catch overflow
        };
        let curr = Frame {
            width: u32::MAX,
            height: u32::MAX,
            format: PixelFormat::Rgb,
            data: vec![0; 10],
        };
        let region = ScreenRegion {
            id: "r1".to_string(),
            name: "Test Region".to_string(),
            x: 0,
            y: 0,
            width: 5,
            height: 5,
            threshold: 0.1,
        };
        let res = DiffEngine::diff_region(&prev, &curr, &region, 5);
        assert!(res.is_err());
        assert!(
            res.err()
                .unwrap()
                .contains("Frame dimensions overflow memory limits")
        );

        // Test 2: Overflowing region bounds (x)
        let prev = create_test_frame(10, 10, 100);
        let curr = create_test_frame(10, 10, 100);
        let region_x_overflow = ScreenRegion {
            id: "r1".to_string(),
            name: "Test Region".to_string(),
            x: u32::MAX,
            width: 5,
            y: 0,
            height: 5,
            threshold: 0.1,
        };
        let res = DiffEngine::diff_region(&prev, &curr, &region_x_overflow, 5);
        assert!(res.is_err());
        let err_x = res.err().unwrap();
        assert!(
            err_x.contains("Region x bounds overflow")
                || err_x.contains("exceed previous frame dimensions")
        );

        // Test 3: Overflowing region bounds (y)
        let region_y_overflow = ScreenRegion {
            id: "r1".to_string(),
            name: "Test Region".to_string(),
            x: 0,
            width: 5,
            y: u32::MAX,
            height: 5,
            threshold: 0.1,
        };
        let res = DiffEngine::diff_region(&prev, &curr, &region_y_overflow, 5);
        assert!(res.is_err());
        let err_y = res.err().unwrap();
        assert!(
            err_y.contains("Region y bounds overflow")
                || err_y.contains("exceed previous frame dimensions")
        );
    }

    #[test]
    fn test_find_changes_u32_stride_alignment() {
        let frame_a = vec![0u8; 100];
        let frame_b = vec![0u8; 100];
        // Stride is 17 bytes (not a multiple of 4)
        let res = find_changes_u32(&frame_a, &frame_b, 5, 5, 17);
        assert!(res.is_err());
        assert_eq!(res.err().unwrap(), DiffError::InvalidStride);
    }

    #[test]
    fn test_apply_roi_padding_clamps_to_screen_bounds() {
        let bbox = BoundingBox {
            x: 10,
            y: 15,
            width: 100,
            height: 80,
        };
        // Screen 1920x1080, padding 20
        let padded = apply_roi_padding(&bbox, 1920, 1080, 20);
        assert_eq!(padded.x, 0); // 10.saturating_sub(20) = 0
        assert_eq!(padded.y, 0); // 15.saturating_sub(20) = 0
        assert_eq!(padded.width, 130); // right = 10 + 100 + 20 = 130, width = 130 - 0 = 130
        assert_eq!(padded.height, 115); // bottom = 15 + 80 + 20 = 115, height = 115 - 0 = 115

        // Check right/bottom clamping
        let bbox_edge = BoundingBox {
            x: 1900,
            y: 1060,
            width: 50,
            height: 50,
        };
        let padded_edge = apply_roi_padding(&bbox_edge, 1920, 1080, 20);
        assert_eq!(padded_edge.x, 1880);
        assert_eq!(padded_edge.y, 1040);
        assert_eq!(padded_edge.x + padded_edge.width, 1920);
        assert_eq!(padded_edge.y + padded_edge.height, 1080);
    }

    #[test]
    fn test_should_co_scale_threshold_at_35_percent() {
        let screen_w = 1920;
        let screen_h = 1080;

        // 30% area -> should NOT co-scale
        let bbox_30 = BoundingBox {
            x: 0,
            y: 0,
            width: 1920,
            height: 324, // 30% of 1080
        };
        assert!(!should_co_scale(&bbox_30, screen_w, screen_h));

        // 40% area -> SHOULD co-scale
        let bbox_40 = BoundingBox {
            x: 0,
            y: 0,
            width: 1920,
            height: 432, // 40% of 1080
        };
        assert!(should_co_scale(&bbox_40, screen_w, screen_h));
    }

    #[test]
    fn test_downsample_rgb_to_720p() {
        // 4K screen 3840x2160
        let w = 3840;
        let h = 2160;
        let data = vec![128u8; w * h * 3];

        let (dst_w, dst_h, dst_data) = downsample_rgb_to_720p(&data, w, h);
        assert!(dst_w <= 1280, "Width {} should be <= 1280", dst_w);
        assert!(dst_h <= 720, "Height {} should be <= 720", dst_h);
        assert_eq!(dst_data.len(), dst_w * dst_h * 3);
        assert_eq!(dst_data[0], 128);
    }
}
