use liva_native_core::vision::{
    BoundingBox, Frame, MockScreenCapturer, PixelFormat, capture_for_vision_from,
    crop_frame_bbox_rgb, process_vision_frame,
};
use std::sync::Mutex;

#[test]
fn test_initial_frame_fallback_720p() {
    let frame = Frame {
        width: 1920,
        height: 1080,
        format: PixelFormat::Rgba,
        data: vec![128; 1920 * 1080 * 4],
    };

    let (w, h, rgb, patch) = process_vision_frame(&frame, None).unwrap();
    assert_eq!((w, h), (1280, 720));
    assert_eq!(rgb.len(), 1280 * 720 * 3);
    assert!(patch.is_none());
}

#[test]
fn test_small_diff_crops_roi_with_16px_padding() {
    let w = 1920u32;
    let h = 1080u32;
    let frame_a = Frame {
        width: w,
        height: h,
        format: PixelFormat::Rgba,
        data: vec![100; (w * h * 4) as usize],
    };
    let mut frame_b = frame_a.clone();

    // Modify a 120x80 box at (x: 400, y: 300)
    // Area ratio = 9,600 / 2,073,600 = ~0.46% (well below 35%)
    for y in 300..380 {
        for x in 400..520 {
            let idx = (y * w as usize + x) * 4;
            frame_b.data[idx] = 220;
            frame_b.data[idx + 1] = 180;
            frame_b.data[idx + 2] = 90;
            frame_b.data[idx + 3] = 255;
        }
    }

    let (cw, ch, rgb, patch_opt) = process_vision_frame(&frame_b, Some(&frame_a)).unwrap();
    assert!(patch_opt.is_some());
    let patch = patch_opt.unwrap();
    assert!(!patch.is_co_scaled);
    assert!(patch.area_ratio <= 0.35);

    // Raw box: 120x80. With 16px padding on all sides -> (120+32)x(80+32) = 152x112
    assert_eq!(patch.raw_bounding_box.width, 120);
    assert_eq!(patch.raw_bounding_box.height, 80);
    assert_eq!(patch.padded_bounding_box.width, 152);
    assert_eq!(patch.padded_bounding_box.height, 112);
    assert_eq!(cw, 152);
    assert_eq!(ch, 112);
    assert_eq!(rgb.len(), 152 * 112 * 3);

    // Calculate visual tokens: ceil(W/28) * ceil(H/28)
    let tokens = ((cw as f32 / 28.0).ceil() * (ch as f32 / 28.0).ceil()) as usize;
    assert!(
        (144..=384).contains(&tokens) || tokens < 144,
        "Tokens {} out of range",
        tokens
    );
    assert!(tokens <= 384, "Tokens {} exceeded 384 SLA ceiling", tokens);
}

#[test]
fn test_large_diff_co_scale_fallback_720p() {
    let w = 1920u32;
    let h = 1080u32;
    let frame_a = Frame {
        width: w,
        height: h,
        format: PixelFormat::Rgba,
        data: vec![50; (w * h * 4) as usize],
    };
    let mut frame_b = frame_a.clone();

    // Modify a 1500x900 box -> area ratio = 1,350,000 / 2,073,600 = ~65% (> 35%)
    for y in 50..950 {
        for x in 100..1600 {
            let idx = (y * w as usize + x) * 4;
            frame_b.data[idx] = 230;
        }
    }

    let (cw, ch, rgb, patch_opt) = process_vision_frame(&frame_b, Some(&frame_a)).unwrap();
    assert!(patch_opt.is_some());
    let patch = patch_opt.unwrap();
    assert!(patch.is_co_scaled);
    assert!(patch.area_ratio > 0.35);
    assert_eq!((cw, ch), (1280, 720));
    assert_eq!(rgb.len(), 1280 * 720 * 3);
}

#[test]
fn test_identical_frames_fallback_720p() {
    let w = 1920u32;
    let h = 1080u32;
    let frame_a = Frame {
        width: w,
        height: h,
        format: PixelFormat::Rgba,
        data: vec![77; (w * h * 4) as usize],
    };
    let frame_b = frame_a.clone();

    let (cw, ch, rgb, patch_opt) = process_vision_frame(&frame_b, Some(&frame_a)).unwrap();
    assert!(patch_opt.is_none());
    assert_eq!((cw, ch), (1280, 720));
    assert_eq!(rgb.len(), 1280 * 720 * 3);
}

#[test]
fn test_normalized_center_coordinate_for_avatar() {
    let slot = Mutex::new(None);
    let capturer = MockScreenCapturer::new(1920, 1080, PixelFormat::Rgba);

    // Initial frame
    capturer.set_frame_data(vec![100; 1920 * 1080 * 4]);
    let (w1, h1, _rgb1, patch1) = capture_for_vision_from(&capturer, &slot).unwrap();
    assert_eq!((w1, h1), (1280, 720));
    assert!(patch1.is_none());

    // Subsequent frame with small diff at center-right: x: 1200..1400, y: 500..600
    let mut data2 = vec![100; 1920 * 1080 * 4];
    for y in 500..600 {
        for x in 1200..1400 {
            data2[(y * 1920 + x) * 4] = 250;
        }
    }
    capturer.set_frame_data(data2);
    let (w2, h2, _rgb2, patch2) = capture_for_vision_from(&capturer, &slot).unwrap();
    assert!(patch2.is_some());
    let p2 = patch2.unwrap();
    assert!(!p2.is_co_scaled);
    // Bounding box: 200 + 32 = 232, 100 + 32 = 132
    assert_eq!(w2, 232);
    assert_eq!(h2, 132);

    let (cx, cy) = p2.center_normalized(1920, 1080);
    // Center of 1200..1400 is 1300 -> 1300 / 1920 ≈ 0.677
    // Center of 500..600 is 550 -> 550 / 1080 ≈ 0.509
    assert!((cx - 1300.0 / 1920.0).abs() < 0.01);
    assert!((0.0..=1.0).contains(&cx));
    assert!((0.0..=1.0).contains(&cy));
}

#[test]
fn test_crop_frame_bbox_rgb_exact_pixels() {
    let frame = Frame {
        width: 4,
        height: 4,
        format: PixelFormat::Rgba,
        data: vec![
            // Row 0
            10, 20, 30, 255, 40, 50, 60, 255, 70, 80, 90, 255, 100, 110, 120, 255, // Row 1
            11, 21, 31, 255, 41, 51, 61, 255, 71, 81, 91, 255, 101, 111, 121, 255, // Row 2
            12, 22, 32, 255, 42, 52, 62, 255, 72, 82, 92, 255, 102, 112, 122, 255, // Row 3
            13, 23, 33, 255, 43, 53, 63, 255, 73, 83, 93, 255, 103, 113, 123, 255,
        ],
    };

    let bbox = BoundingBox {
        x: 1,
        y: 1,
        width: 2,
        height: 2,
    };
    let (cw, ch, rgb) = crop_frame_bbox_rgb(&frame, &bbox);
    assert_eq!((cw, ch), (2, 2));
    assert_eq!(rgb.len(), 2 * 2 * 3);
    // (x:1, y:1) should be [41, 51, 61]
    assert_eq!(&rgb[0..3], &[41, 51, 61]);
    // (x:2, y:1) should be [71, 81, 91]
    assert_eq!(&rgb[3..6], &[71, 81, 91]);
    // (x:1, y:2) should be [42, 52, 62]
    assert_eq!(&rgb[6..9], &[42, 52, 62]);
    // (x:2, y:2) should be [72, 82, 92]
    assert_eq!(&rgb[9..12], &[72, 82, 92]);
}

#[test]
fn test_square_roi_patch_clamps_to_token_ceiling_384() {
    let w = 1920u32;
    let h = 1080u32;
    let frame_a = Frame {
        width: w,
        height: h,
        format: PixelFormat::Rgba,
        data: vec![50; (w * h * 4) as usize],
    };
    let mut frame_b = frame_a.clone();

    // Modify a 700x700 square box -> area ratio = 490,000 / 2,073,600 = ~23.6% (<= 35%)
    for y in 100..800 {
        for x in 100..800 {
            let idx = (y * w as usize + x) * 4;
            frame_b.data[idx] = 180;
        }
    }

    let (cw, ch, rgb, patch_opt) = process_vision_frame(&frame_b, Some(&frame_a)).unwrap();
    assert!(patch_opt.is_some());
    let patch = patch_opt.unwrap();
    assert!(!patch.is_co_scaled);
    assert!(patch.area_ratio <= 0.35);

    // Bounding box was 700x700 with padding 732x732
    // Must be clamped to <= 384 tokens (532x532 for 1:1)
    let tokens = ((cw as f32 / 28.0).ceil() * (ch as f32 / 28.0).ceil()) as usize;
    assert!(
        tokens <= 384,
        "Tokens {} exceeded 384 SLA ceiling for square patch",
        tokens
    );
    assert_eq!((cw, ch), (532, 532));
    assert_eq!(rgb.len(), 532 * 532 * 3);
}
