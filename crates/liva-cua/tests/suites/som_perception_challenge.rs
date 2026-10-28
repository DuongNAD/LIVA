//! Empirical Challenger M1: Adversarial stress testing for Set-of-Mark (SoM)
//! perception indexing, noise filtering, and coordinate resolution in crates/liva-cua.
//!
//! Covers:
//! 1. Duplicate and empty element identifiers (ID collision vs SoM assignment).
//! 2. Extreme and negative coordinates (overflow in row-band grouping math).
//! 3. 1000+ deeply nested UI element nodes (recursion limits and stack safety).
//! 4. Zero, negative, 1x1, and minimized sentinel bounds noise filtering.
//! 5. Identical bounding boxes reading-order determinism and tie-breaking.
//! 6. Jittered horizontal row-band clustering.
//! 7. Pathological labels (Vietnamese unicode, emojis, unescaped quotes, newlines).
//! 8. Coordinate resolution on boundary and out-of-bounds indices.
//! 9. Visual ROI fallback triggers under degenerate trees (invisible, disabled, offscreen).

use liva_cua::mock::MockCuaDriver;
use liva_cua::perception::{
    PerceptionEngine, PerceptionResolver, SoMIndexer, SoMIndexerConfig, UiElement, UiRole, UiState,
    UiTree,
};
use liva_cua::types::{CuaAction, CuaDeliveryMode, CuaPoint, CuaRect, CuaWindowInfo, MouseButton};
use liva_cua::vision::mock::MockScreenCapturer;
use liva_cua::vision::types::CuaPixelFormat;
use std::sync::Arc;

// ============================================================================
// 1. DUPLICATE AND EMPTY IDENTIFIERS (COLLISION RESILIENCE)
// ============================================================================

#[test]
fn test_pathological_duplicate_and_empty_ids() {
    let window_bounds = CuaRect::new(0, 0, 800, 600);

    // 4 interactive buttons where pairs share identical IDs or empty strings
    // In Android XML, UIA, or Web DOM, IDs can be non-unique or empty.
    let btn1 = UiElement::new(
        "dup_btn",
        UiRole::Button,
        "Button A",
        CuaRect::new(50, 50, 100, 30),
    )
    .with_state(UiState::clickable());
    let btn2 = UiElement::new(
        "dup_btn",
        UiRole::Button,
        "Button B",
        CuaRect::new(160, 50, 100, 30),
    )
    .with_state(UiState::clickable());
    let btn3 = UiElement::new(
        "",
        UiRole::Button,
        "Button C",
        CuaRect::new(50, 100, 100, 30),
    )
    .with_state(UiState::clickable());
    let btn4 = UiElement::new(
        "",
        UiRole::Button,
        "Button D",
        CuaRect::new(160, 100, 100, 30),
    )
    .with_state(UiState::clickable());

    let root = UiElement::new("root", UiRole::Window, "Test Window", window_bounds)
        .with_children(vec![btn1, btn2, btn3, btn4]);

    let mut tree = UiTree::new(0x8001, root);
    let count = SoMIndexer::index_tree(&mut tree);

    // Record empirical behavior for verification
    println!(
        "[EMPIRICAL] Duplicate IDs test: indexed count={}, interactive_count={}",
        count, tree.interactive_count
    );

    let interactive = tree.interactive_elements();
    println!(
        "[EMPIRICAL] Interactive elements count: {}",
        interactive.len()
    );
    for elem in &interactive {
        println!(
            "[EMPIRICAL] Element id='{}', name='{}', som_index={:?}",
            elem.id, elem.name, elem.som_index
        );
    }

    // Verify whether duplicate/empty IDs receive strictly monotonic marks [1, 2, 3, 4]
    let marks: Vec<usize> = interactive.iter().filter_map(|e| e.som_index).collect();
    println!("[EMPIRICAL] Assigned marks: {:?}", marks);

    // Defect remediation verification:
    // With 4 distinct interactive buttons, indexer assigns monotonic marks [1, 2, 3, 4]
    // without dropping duplicate or empty IDs.
    assert_eq!(
        marks,
        vec![1, 2, 3, 4],
        "[REMEDIATED] Duplicate and empty IDs all receive monotonic [1, 2, 3, 4] marks!"
    );
    assert_eq!(count, 4, "All 4 buttons must be counted as interactive");
    assert_eq!(tree.interactive_count, 4);

    // Verify resolving each mark succeeds
    for mark in 1..=4 {
        let (pt, bounds) = PerceptionResolver::resolve_coordinate(&tree, mark)
            .expect("All assigned marks must resolve successfully");
        assert!(bounds.width > 0 && bounds.height > 0);
        assert_ne!(pt, CuaPoint::default());
    }
}

// ============================================================================
// 2. EXTREME COORDINATES & INTEGER ARITHMETIC SAFETY
// ============================================================================

#[test]
fn test_pathological_extreme_coordinates_overflow() {
    let window_bounds = CuaRect::new(0, 0, 1000, 1000);

    // Multi-monitor coordinate (e.g. secondary screen on left: x = -1920, y = -500)
    let left_monitor_btn = UiElement::new(
        "left_btn",
        UiRole::Button,
        "Left Monitor",
        CuaRect::new(-1920, -500, 100, 30),
    )
    .with_state(UiState::clickable());

    // Coordinate near -9999 (just above the -10000 offscreen filter)
    let edge_negative_btn = UiElement::new(
        "edge_neg_btn",
        UiRole::Button,
        "Edge Negative",
        CuaRect::new(-9999, -9999, 100, 30),
    )
    .with_state(UiState::clickable());

    // Coordinate near i32::MAX
    let extreme_positive_btn = UiElement::new(
        "extreme_pos_btn",
        UiRole::Button,
        "Extreme Positive",
        CuaRect::new(2_000_000_000, 2_000_000_000, 100, 30),
    )
    .with_state(UiState::clickable());

    let root =
        UiElement::new("root", UiRole::Window, "Extreme Window", window_bounds).with_children(
            vec![left_monitor_btn, edge_negative_btn, extreme_positive_btn],
        );

    let mut tree = UiTree::new(0x8002, root);

    // Check if row banding arithmetic triggers integer overflow panic:
    // (item.y - band_anchor_y) where item.y = 2_000_000_000 and band_anchor_y = -9999
    // In debug mode, 2_000_000_000 - (-9999) = 2_000_009_999 fits in i32,
    // but what if item.y = i32::MAX and band_anchor_y = -9999?
    // 2147483647 - (-9999) = 2147493646 > i32::MAX -> PANIC!
    let count = SoMIndexer::index_tree(&mut tree);
    println!("[EMPIRICAL] Extreme coordinates index count: {}", count);
    assert_eq!(count, 3, "All 3 extreme valid buttons should be indexed");
}

#[test]
fn test_pathological_extreme_i32_subtraction_overflow_in_row_band() {
    let window_bounds = CuaRect::new(0, 0, 1000, 1000);

    // Negative coordinate button (just within -10000 limit)
    let neg_btn = UiElement::new(
        "neg_btn",
        UiRole::Button,
        "Negative Y",
        CuaRect::new(100, -9999, 50, 50),
    )
    .with_state(UiState::clickable());

    // Button near i32::MAX (e.g. i32::MAX - 100)
    let max_btn = UiElement::new(
        "max_btn",
        UiRole::Button,
        "Max Y",
        CuaRect::new(100, i32::MAX - 100, 50, 50),
    )
    .with_state(UiState::clickable());

    let root = UiElement::new("root", UiRole::Window, "Overflow Window", window_bounds)
        .with_children(vec![neg_btn, max_btn]);

    let mut tree = UiTree::new(0x8003, root);

    // This specifically tests `(item.y as i64 - band_anchor_y as i64).abs()`:
    // band_anchor_y = -9999, item.y = 2147483547.
    // Difference is 2147483547 - (-9999) = 2147493546.
    // With 64-bit integer arithmetic, this executes safely without integer overflow panic.
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        SoMIndexer::index_tree(&mut tree)
    }));

    assert!(
        res.is_ok(),
        "[REMEDIATED] Integer subtraction overflow eliminated; no panic occurred!"
    );
    let count = res.unwrap();
    assert_eq!(count, 2, "Both extreme buttons must be indexed");
    assert_eq!(tree.interactive_count, 2);
    println!("[REMEDIATED] Integer subtraction overflow eliminated in SoMIndexer row banding!");
}

// ============================================================================
// 3. 1000+ DEEPLY NESTED UI ELEMENT NODES (STACK SAFETY & RECURSION)
// ============================================================================

#[test]
fn test_pathological_1000_deeply_nested_nodes() {
    let depth = 1000;
    let window_bounds = CuaRect::new(0, 0, 1024, 768);

    // Build a 1000-deep nested linear tree: node_0 -> child node_1 -> ... -> leaf button
    let mut current = UiElement::new(
        format!("leaf_btn_{}", depth),
        UiRole::Button,
        "Deep Button",
        CuaRect::new(100, 100, 80, 30),
    )
    .with_state(UiState::clickable());

    for i in (0..depth).rev() {
        let parent = UiElement::new(
            format!("pane_level_{}", i),
            UiRole::Pane,
            format!("Container Level {}", i),
            window_bounds,
        )
        .with_state(UiState::static_display())
        .with_child(current);

        current = parent;
    }

    let mut tree = UiTree::new(0x8004, current);
    assert_eq!(tree.total_nodes, depth + 1);

    // Test indexing under 1000 recursion frames
    let count = SoMIndexer::index_tree(&mut tree);
    assert_eq!(count, 1, "Deepest leaf button must be indexed");
    assert_eq!(tree.interactive_count, 1);

    // Test find_by_som under 1000 depth
    let leaf = tree.find_by_som(1);
    assert!(leaf.is_some());
    assert_eq!(leaf.unwrap().id, format!("leaf_btn_{}", depth));

    // Test resolve_coordinate under 1000 depth
    let (pt, bounds) =
        PerceptionResolver::resolve_coordinate(&tree, 1).expect("resolve deep button");
    assert_eq!(pt, CuaPoint::new(140, 115));
    assert_eq!(bounds, CuaRect::new(100, 100, 80, 30));

    // Test prompt formatting
    let prompt = tree.format_som_prompt();
    assert!(prompt.contains("[1] Button \"Deep Button\""));
}

#[test]
fn test_pathological_wide_tree_1000_elements() {
    let count_elements = 1000;
    let window_bounds = CuaRect::new(0, 0, 2000, 2000);

    let mut children = Vec::with_capacity(count_elements);
    for i in 0..count_elements {
        let row = (i / 20) as i32;
        let col = (i % 20) as i32;
        let btn = UiElement::new(
            format!("btn_{:04}", i),
            UiRole::Button,
            format!("Button {:04}", i),
            CuaRect::new(col * 90 + 10, row * 35 + 10, 80, 28),
        )
        .with_state(UiState::clickable());
        children.push(btn);
    }

    let root = UiElement::new("wide_root", UiRole::Window, "Wide Window", window_bounds)
        .with_children(children);

    let mut tree = UiTree::new(0x8005, root);
    let start = std::time::Instant::now();
    let indexed = SoMIndexer::index_tree(&mut tree);
    let elapsed = start.elapsed();

    println!(
        "[EMPIRICAL] Wide tree (1000 interactive elements) indexed {} elements in {:?}",
        indexed, elapsed
    );
    assert_eq!(indexed, 1000);
    assert_eq!(tree.interactive_count, 1000);

    // Verify monotonicity across all 1000 elements
    let interactive = tree.interactive_elements();
    assert_eq!(interactive.len(), 1000);
    for (i, elem) in interactive.iter().enumerate() {
        assert_eq!(
            elem.som_index,
            Some(i + 1),
            "Element mark must equal 1-based index"
        );
    }
}

// ============================================================================
// 4. ZERO, NEGATIVE, AND BOUNDARY DIMENSION PRUNING
// ============================================================================

#[test]
fn test_pathological_degenerate_bounds_pruning() {
    let window_bounds = CuaRect::new(0, 0, 800, 600);

    // 0x0
    let zero_size = UiElement::new(
        "zero_size",
        UiRole::Button,
        "Zero",
        CuaRect::new(50, 50, 0, 0),
    )
    .with_state(UiState::clickable());

    // 0 width, valid height
    let zero_w = UiElement::new(
        "zero_w",
        UiRole::Button,
        "Zero W",
        CuaRect::new(50, 50, 0, 30),
    )
    .with_state(UiState::clickable());

    // Valid width, 0 height
    let zero_h = UiElement::new(
        "zero_h",
        UiRole::Button,
        "Zero H",
        CuaRect::new(50, 50, 100, 0),
    )
    .with_state(UiState::clickable());

    // Negative dimensions
    let neg_dim = UiElement::new(
        "neg_dim",
        UiRole::Button,
        "Neg Dim",
        CuaRect::new(50, 50, -50, -20),
    )
    .with_state(UiState::clickable());

    // Sub-threshold 1x1 (min_dimension = 2)
    let sub_threshold_1x1 =
        UiElement::new("sub_1x1", UiRole::Button, "1x1", CuaRect::new(50, 50, 1, 1))
            .with_state(UiState::clickable());

    // Exactly 2x2 (boundary condition: meets min_dimension = 2)
    let exact_2x2 = UiElement::new(
        "exact_2x2",
        UiRole::Button,
        "2x2",
        CuaRect::new(50, 50, 2, 2),
    )
    .with_state(UiState::clickable());

    // Classic Win32 Minimized Sentinel (-32000, -32000)
    let minimized_sentinel = UiElement::new(
        "minimized_sentinel",
        UiRole::Button,
        "Minimized",
        CuaRect::new(-32000, -32000, 80, 30),
    )
    .with_state(UiState::clickable());

    // Off-screen X sentinel (<= -10000)
    let offscreen_x = UiElement::new(
        "offscreen_x",
        UiRole::Button,
        "Off X",
        CuaRect::new(-10000, 50, 80, 30),
    )
    .with_state(UiState::clickable());

    // Off-screen Y sentinel (<= -10000)
    let offscreen_y = UiElement::new(
        "offscreen_y",
        UiRole::Button,
        "Off Y",
        CuaRect::new(50, -10000, 80, 30),
    )
    .with_state(UiState::clickable());

    // Valid control
    let valid_btn = UiElement::new(
        "valid_btn",
        UiRole::Button,
        "Valid",
        CuaRect::new(100, 100, 80, 30),
    )
    .with_state(UiState::clickable());

    let root = UiElement::new("root", UiRole::Window, "Pruning Window", window_bounds)
        .with_children(vec![
            zero_size,
            zero_w,
            zero_h,
            neg_dim,
            sub_threshold_1x1,
            exact_2x2,
            minimized_sentinel,
            offscreen_x,
            offscreen_y,
            valid_btn,
        ]);

    let mut tree = UiTree::new(0x8006, root);
    let count = SoMIndexer::index_tree(&mut tree);

    println!("[EMPIRICAL] Degenerate bounds indexed count: {}", count);
    let interactive = tree.interactive_elements();
    for elem in &interactive {
        println!(
            "[EMPIRICAL] Survived pruning: id='{}', bounds={:?}, som={:?}",
            elem.id, elem.bounds, elem.som_index
        );
    }

    // Only exact_2x2 and valid_btn should pass
    assert_eq!(
        count, 2,
        "Only exact_2x2 and valid_btn should survive noise pruning"
    );
    assert_eq!(interactive[0].id, "exact_2x2");
    assert_eq!(interactive[1].id, "valid_btn");
}

// ============================================================================
// 5. IDENTICAL BOUNDS & READING-ORDER DETERMINISM
// ============================================================================

#[test]
fn test_pathological_identical_bounds_determinism() {
    let window_bounds = CuaRect::new(0, 0, 800, 600);
    let shared_rect = CuaRect::new(200, 200, 80, 30);

    // Three controls sharing exact same bounds but different IDs
    let btn_gamma = UiElement::new("btn_gamma", UiRole::Button, "Gamma", shared_rect)
        .with_state(UiState::clickable());
    let btn_alpha = UiElement::new("btn_alpha", UiRole::Button, "Alpha", shared_rect)
        .with_state(UiState::clickable());
    let btn_beta = UiElement::new("btn_beta", UiRole::Button, "Beta", shared_rect)
        .with_state(UiState::clickable());

    // Test run 1: insertion order Gamma, Alpha, Beta
    let root1 = UiElement::new("root1", UiRole::Window, "Window", window_bounds)
        .with_children(vec![btn_gamma.clone(), btn_alpha.clone(), btn_beta.clone()]);
    let mut tree1 = UiTree::new(0x8007, root1);
    SoMIndexer::index_tree(&mut tree1);
    let order1: Vec<String> = tree1
        .interactive_elements()
        .iter()
        .map(|e| e.id.clone())
        .collect();

    // Test run 2: insertion order Beta, Gamma, Alpha
    let root2 = UiElement::new("root2", UiRole::Window, "Window", window_bounds)
        .with_children(vec![btn_beta, btn_gamma, btn_alpha]);
    let mut tree2 = UiTree::new(0x8007, root2);
    SoMIndexer::index_tree(&mut tree2);
    let order2: Vec<String> = tree2
        .interactive_elements()
        .iter()
        .map(|e| e.id.clone())
        .collect();

    println!("[EMPIRICAL] Identical bounds order1: {:?}", order1);
    println!("[EMPIRICAL] Identical bounds order2: {:?}", order2);

    // Determinism check: both runs must produce identical order sorted by ID tie-breaker
    assert_eq!(
        order1, order2,
        "Reading order must be deterministic regardless of insertion order"
    );
    assert_eq!(order1, vec!["btn_alpha", "btn_beta", "btn_gamma"]);
}

// ============================================================================
// 6. JITTERED HORIZONTAL ALIGNMENT CLUSTERING
// ============================================================================

#[test]
fn test_pathological_jittered_horizontal_clustering() {
    let window_bounds = CuaRect::new(0, 0, 800, 600);
    let config = SoMIndexerConfig {
        band_tolerance_px: 8,
        min_dimension: 2,
        include_static_text: false,
    };

    // Row 1 anchor at y = 100:
    // b1 at y = 100, x = 300
    // b2 at y = 107 (delta 7 <= 8, same band), x = 100
    // b3 at y = 108 (delta 8 <= 8, same band), x = 200
    // b4 at y = 109 (delta 9 > 8 from anchor 100, new band!), x = 50
    let b1 = UiElement::new("b1", UiRole::Button, "B1", CuaRect::new(300, 100, 60, 25))
        .with_state(UiState::clickable());
    let b2 = UiElement::new("b2", UiRole::Button, "B2", CuaRect::new(100, 107, 60, 25))
        .with_state(UiState::clickable());
    let b3 = UiElement::new("b3", UiRole::Button, "B3", CuaRect::new(200, 108, 60, 25))
        .with_state(UiState::clickable());
    let b4 = UiElement::new("b4", UiRole::Button, "B4", CuaRect::new(50, 109, 60, 25))
        .with_state(UiState::clickable());

    let root = UiElement::new("root", UiRole::Window, "Jitter Window", window_bounds)
        .with_children(vec![b1, b2, b3, b4]);

    let mut tree = UiTree::new(0x8008, root);
    SoMIndexer::index_tree_with_config(&mut tree, &config);

    let interactive = tree.interactive_elements();
    let order: Vec<String> = interactive.iter().map(|e| e.id.clone()).collect();
    println!("[EMPIRICAL] Jittered clustering order: {:?}", order);

    // Band 1 (anchor 100): b1 (x=300), b2 (x=100), b3 (x=200) -> sorted by x: b2 (100), b3 (200), b1 (300)
    // Band 2 (anchor 109): b4 (x=50) -> b4
    // Expected order: b2, b3, b1, b4
    assert_eq!(order, vec!["b2", "b3", "b1", "b4"]);
}

// ============================================================================
// 7. PATHOLOGICAL UNICODE & FORMATTING LABELS
// ============================================================================

#[test]
fn test_pathological_labels_and_unicode_prompt() {
    let window_bounds = CuaRect::new(0, 0, 800, 600);

    // Empty name
    let empty_name = UiElement::new(
        "lbl_empty",
        UiRole::Button,
        "",
        CuaRect::new(10, 10, 80, 25),
    )
    .with_state(UiState::clickable());

    // Vietnamese diacritics
    let vn_name = UiElement::new(
        "lbl_vn",
        UiRole::Button,
        "Đăng nhập & Xác thực hệ thống",
        CuaRect::new(10, 45, 120, 25),
    )
    .with_state(UiState::clickable());

    // Emojis
    let emoji_name = UiElement::new(
        "lbl_emoji",
        UiRole::Button,
        "🚀 Phóng tác vụ 🔥",
        CuaRect::new(10, 80, 120, 25),
    )
    .with_state(UiState::clickable());

    // Embedded quotes
    let quotes_name = UiElement::new(
        "lbl_quotes",
        UiRole::Button,
        "Click \"Submit\" now",
        CuaRect::new(10, 115, 120, 25),
    )
    .with_state(UiState::clickable());

    // Embedded newlines
    let newline_name = UiElement::new(
        "lbl_newline",
        UiRole::Button,
        "Line 1\nLine 2\r\nLine 3",
        CuaRect::new(10, 150, 120, 25),
    )
    .with_state(UiState::clickable());

    let root =
        UiElement::new("root", UiRole::Window, "Unicode Window", window_bounds).with_children(
            vec![empty_name, vn_name, emoji_name, quotes_name, newline_name],
        );

    let mut tree = UiTree::new(0x8009, root);
    SoMIndexer::index_tree(&mut tree);

    let prompt = tree.format_som_prompt();
    println!("[EMPIRICAL] Formatted prompt output:\n{}", prompt);

    assert!(prompt.contains("[1] Button \"\" bounds="));
    assert!(prompt.contains("Đăng nhập & Xác thực hệ thống"));
    assert!(prompt.contains("🚀 Phóng tác vụ 🔥"));
    assert!(prompt.contains("Click \"Submit\" now"));
    assert!(prompt.contains("Line 1 Line 2  Line 3"));
    assert_eq!(
        prompt.lines().count(),
        5,
        "Each marked element must occupy exactly one line in format_som_prompt"
    );
}

// ============================================================================
// 8. COORDINATE RESOLVER BOUNDARY & OUT-OF-BOUNDS
// ============================================================================

#[test]
fn test_pathological_resolver_boundary_and_out_of_bounds() {
    let window_bounds = CuaRect::new(100, 100, 600, 400);

    let btn1 = UiElement::new(
        "btn1",
        UiRole::Button,
        "Button 1",
        CuaRect::new(120, 150, 80, 30),
    )
    .with_state(UiState::clickable());
    let btn2 = UiElement::new(
        "btn2",
        UiRole::Button,
        "Button 2",
        CuaRect::new(220, 150, 80, 30),
    )
    .with_state(UiState::clickable());

    let root = UiElement::new("root", UiRole::Window, "Resolver Window", window_bounds)
        .with_children(vec![btn1, btn2]);

    let mut tree = UiTree::new(0x8010, root);
    let count = SoMIndexer::index_tree(&mut tree);
    assert_eq!(count, 2);

    // som_index = 0 is invalid (1-based index)
    let res_zero = PerceptionResolver::resolve_coordinate(&tree, 0);
    assert!(res_zero.is_err(), "Index 0 must fail");
    assert!(matches!(
        res_zero.unwrap_err(),
        liva_cua::types::CuaError::InvalidParameter(ref msg) if msg.contains("SoM mark [0] not found")
    ));

    // som_index = 3 is out of range ([1, 2])
    let res_three = PerceptionResolver::resolve_coordinate(&tree, 3);
    assert!(res_three.is_err(), "Index 3 must fail");
    assert!(matches!(
        res_three.unwrap_err(),
        liva_cua::types::CuaError::InvalidParameter(ref msg) if msg.contains("SoM mark [3] not found")
    ));

    // som_index = usize::MAX
    let res_max = PerceptionResolver::resolve_coordinate(&tree, usize::MAX);
    assert!(res_max.is_err(), "Index usize::MAX must fail");
    assert!(matches!(
        res_max.unwrap_err(),
        liva_cua::types::CuaError::InvalidParameter(_)
    ));

    // Valid indices [1] and [2]
    let (pt1, bounds1) = PerceptionResolver::resolve_coordinate(&tree, 1).unwrap();
    assert_eq!(pt1, CuaPoint::new(120 + 40, 150 + 15));
    assert_eq!(bounds1, CuaRect::new(120, 150, 80, 30));

    let (pt2, bounds2) = PerceptionResolver::resolve_coordinate(&tree, 2).unwrap();
    assert_eq!(pt2, CuaPoint::new(220 + 40, 150 + 15));
    assert_eq!(bounds2, CuaRect::new(220, 150, 80, 30));

    // Containment validation
    assert!(PerceptionResolver::validate_containment(
        &bounds1,
        &window_bounds
    ));
    assert!(PerceptionResolver::validate_containment(
        &bounds2,
        &window_bounds
    ));

    // Element partially outside window
    let outside_bounds = CuaRect::new(650, 150, 100, 30); // center is 700, window ends at 700
    assert!(!PerceptionResolver::validate_containment(
        &outside_bounds,
        &window_bounds
    ));

    // Resolve action for all CuaAction variants
    let click_action = PerceptionResolver::resolve_action(
        &tree,
        1,
        CuaAction::Click {
            target_hwnd: 0x8010,
            x: None,
            y: None,
            button: MouseButton::Right,
            click_count: 2,
            delivery_mode: CuaDeliveryMode::Foreground,
        },
    )
    .unwrap();

    match click_action {
        CuaAction::Click {
            target_hwnd,
            x,
            y,
            button,
            click_count,
            delivery_mode,
        } => {
            assert_eq!(target_hwnd, 0x8010);
            assert_eq!(x, Some(160));
            assert_eq!(y, Some(165));
            assert_eq!(button, MouseButton::Right);
            assert_eq!(click_count, 2);
            assert_eq!(delivery_mode, CuaDeliveryMode::Foreground);
        }
        _ => panic!("Expected Click"),
    }

    let drag_action = PerceptionResolver::resolve_action(
        &tree,
        2,
        CuaAction::Drag {
            target_hwnd: 0x8010,
            start_x: 0,
            start_y: 0,
            end_x: 500,
            end_y: 500,
            button: MouseButton::Left,
            steps: 10,
            delivery_mode: CuaDeliveryMode::Background,
        },
    )
    .unwrap();

    match drag_action {
        CuaAction::Drag {
            start_x,
            start_y,
            end_x,
            end_y,
            ..
        } => {
            assert_eq!(start_x, 260);
            assert_eq!(start_y, 165);
            assert_eq!(end_x, 500);
            assert_eq!(end_y, 500);
        }
        _ => panic!("Expected Drag"),
    }
}

// ============================================================================
// 9. AUTOMATIC VISUAL ROI FALLBACK DEGENERATE SCENARIOS
// ============================================================================

#[tokio::test]
async fn test_pathological_visual_fallback_degenerate_trees() {
    let driver = Arc::new(MockCuaDriver::new());
    let capturer = Arc::new(MockScreenCapturer::new(1280, 720, CuaPixelFormat::Rgba, 0));
    let window_bounds = CuaRect::new(50, 50, 400, 300);

    let window_info = CuaWindowInfo {
        hwnd: 0x9001,
        pid: 1234,
        title: "Fallback Test".to_string(),
        class_name: "FallbackWin".to_string(),
        process_name: Some("fallback.exe".to_string()),
        bounds: window_bounds,
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    };
    driver.insert_window(window_info);

    let engine = PerceptionEngine::new();

    // Scenario A: Tree with ONLY disabled controls
    let disabled_root = UiElement::new("root_a", UiRole::Window, "A", window_bounds).with_child(
        UiElement::new(
            "dis_btn",
            UiRole::Button,
            "Disabled",
            CuaRect::new(60, 60, 80, 30),
        )
        .with_state(UiState::disabled()),
    );
    let tree_a = UiTree::new(0x9001, disabled_root);
    driver.insert_ui_tree(0x9001, tree_a);

    let res_a = engine
        .perceive(0x9001, driver.as_ref(), Some(capturer.as_ref()))
        .await
        .unwrap();
    assert!(
        res_a.is_visual_fallback(),
        "Disabled-only tree must trigger visual fallback"
    );
    assert_eq!(res_a.som_count(), 0);

    // Scenario B: Tree with ONLY invisible controls
    let hidden_root = UiElement::new("root_b", UiRole::Window, "B", window_bounds).with_child(
        UiElement::new(
            "hid_btn",
            UiRole::Button,
            "Hidden",
            CuaRect::new(60, 60, 80, 30),
        )
        .with_state(UiState::hidden()),
    );
    let tree_b = UiTree::new(0x9001, hidden_root);
    driver.insert_ui_tree(0x9001, tree_b);

    let res_b = engine
        .perceive(0x9001, driver.as_ref(), Some(capturer.as_ref()))
        .await
        .unwrap();
    assert!(
        res_b.is_visual_fallback(),
        "Hidden-only tree must trigger visual fallback"
    );

    // Scenario C: Tree with ONLY off-screen controls
    let offscreen_root = UiElement::new("root_c", UiRole::Window, "C", window_bounds).with_child(
        UiElement::new(
            "off_btn",
            UiRole::Button,
            "Off",
            CuaRect::new(-32000, -32000, 80, 30),
        )
        .with_state(UiState::clickable()),
    );
    let tree_c = UiTree::new(0x9001, offscreen_root);
    driver.insert_ui_tree(0x9001, tree_c);

    let res_c = engine
        .perceive(0x9001, driver.as_ref(), Some(capturer.as_ref()))
        .await
        .unwrap();
    assert!(
        res_c.is_visual_fallback(),
        "Offscreen-only tree must trigger visual fallback"
    );

    // Scenario D: Tree with ONLY non-interactive containers and static text
    let static_root =
        UiElement::new("root_d", UiRole::Window, "D", window_bounds).with_children(vec![
            UiElement::new("pane", UiRole::Pane, "Pane", window_bounds)
                .with_state(UiState::static_display()),
            UiElement::new(
                "text",
                UiRole::Text,
                "Static Text",
                CuaRect::new(60, 60, 200, 20),
            )
            .with_state(UiState::static_display()),
        ]);
    let tree_d = UiTree::new(0x9001, static_root);
    driver.insert_ui_tree(0x9001, tree_d);

    let res_d = engine
        .perceive(0x9001, driver.as_ref(), Some(capturer.as_ref()))
        .await
        .unwrap();
    assert!(
        res_d.is_visual_fallback(),
        "Static-only tree must trigger visual fallback"
    );
}

// ============================================================================
// ============================================================================
// 10. SAFE TREE TRAVERSAL AND SOM INDICES CLEARING (ELIMINATED UNSOUND ALIASING)
// ============================================================================

#[test]
fn test_pathological_safe_tree_traversal_and_clear_som_indices() {
    let child = UiElement::new(
        "child",
        UiRole::Button,
        "Child",
        CuaRect::new(10, 10, 50, 20),
    )
    .with_state(UiState::clickable());
    let parent = UiElement::new(
        "parent",
        UiRole::Window,
        "Parent",
        CuaRect::new(0, 0, 100, 100),
    )
    .with_child(child);
    let mut tree = UiTree::new(0x8011, parent);

    SoMIndexer::index_tree(&mut tree);
    assert_eq!(tree.interactive_count, 1);
    assert_eq!(tree.find_by_som(1).map(|e| e.id.as_str()), Some("child"));

    // Verify safe in-place tree mutation via for_each_mut without unsafe aliasing
    let mut visited_ids = Vec::new();
    tree.for_each_mut(|node| {
        visited_ids.push(node.id.clone());
    });
    assert_eq!(visited_ids, vec!["parent", "child"]);

    // Verify clear_som_indices cleanly clears all marks safely
    tree.clear_som_indices();
    assert_eq!(tree.interactive_count, 0);
    assert!(tree.find_by_som(1).is_none());
    println!("[REMEDIATED] Safe for_each_mut and clear_som_indices verified without unsafe code!");
}
