//! Tests for OffscreenScrollRecovery engine.

use liva_cua::guards::OffscreenScrollRecovery;
use liva_cua::types::CuaRect;

#[test]
fn test_offscreen_element_detection() {
    let recovery = OffscreenScrollRecovery::default();
    let viewport = CuaRect::new(0, 0, 800, 600);

    // Element comfortably inside viewport
    let inside_elem = CuaRect::new(100, 100, 50, 50);
    assert!(!recovery.is_offscreen(&inside_elem, &viewport));

    // Element far below viewport (y = 800)
    let below_elem = CuaRect::new(100, 800, 50, 50);
    assert!(recovery.is_offscreen(&below_elem, &viewport));

    // Element to the right of viewport (x = 1000)
    let right_elem = CuaRect::new(1000, 100, 50, 50);
    assert!(recovery.is_offscreen(&right_elem, &viewport));
}

#[test]
fn test_scroll_delta_calculation() {
    let recovery = OffscreenScrollRecovery::default();
    let viewport = CuaRect::new(0, 0, 800, 600); // Center = (400, 300)

    // Element center = (400, 900)
    let target_elem = CuaRect::new(350, 850, 100, 100);
    let (delta_x, delta_y) = recovery.compute_scroll_delta(&target_elem, &viewport);

    assert_eq!(delta_x, 0, "No horizontal scroll required");
    assert_eq!(delta_y, 600, "Vertical scroll must scroll down 600px");
}
