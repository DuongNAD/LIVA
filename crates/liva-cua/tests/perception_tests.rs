use liva_cua::mock::MockCuaDriver;
use liva_cua::perception::{
    PerceptionEngine, PerceptionMockFixtures, PerceptionResolver, PerceptionResult, SoMIndexer,
    SoMIndexerConfig,
};
use liva_cua::router::{TargetCoordinate, TargetDescriptor, TargetResolver, WindowSelector};
use liva_cua::types::{
    CuaAction, CuaConfig, CuaDeliveryMode, CuaPoint, CuaRect, CuaWindowInfo, MouseButton,
};
use liva_cua::vision::mock::MockScreenCapturer;
use liva_cua::vision::types::CuaPixelFormat;
use liva_cua::CuaEngine;
use std::sync::Arc;

#[tokio::test]
async fn test_som_indexing_deterministic_assignment() {
    let window_bounds = CuaRect::new(100, 100, 500, 400);
    let mut tree = PerceptionMockFixtures::standard_dialog(0x1001, window_bounds);

    let count = SoMIndexer::index_tree(&mut tree);
    assert_eq!(
        count, 4,
        "Standard dialog should mark 4 interactive elements (Edit, CheckBox, Cancel, OK)"
    );
    assert_eq!(tree.interactive_count, 4);

    let interactive = tree.interactive_elements();
    assert_eq!(interactive.len(), 4);

    // Verify monotonic 1-based indexing [1, 2, 3, 4]
    for (i, elem) in interactive.iter().enumerate() {
        assert_eq!(elem.som_index, Some(i + 1));
    }

    // Verify visual reading order:
    // [1] Edit "Optional Notes" (y = 190)
    // [2] CheckBox "Do not ask again" (y = 230)
    // [3] Button "Cancel" (y = 280, x = 310)
    // [4] Button "OK" (y = 280, x = 400)
    assert_eq!(interactive[0].id, "input_notes");
    assert_eq!(interactive[1].id, "chk_remember");
    assert_eq!(interactive[2].id, "btn_cancel");
    assert_eq!(interactive[3].id, "btn_ok");
}

#[tokio::test]
async fn test_som_noise_filtering() {
    let window_bounds = CuaRect::new(0, 0, 800, 600);
    let mut tree = PerceptionMockFixtures::noisy_layout(0x1002, window_bounds);

    let count = SoMIndexer::index_tree(&mut tree);
    // Should filter out:
    // - outer_pane (container without clickability)
    // - group_inner (container)
    // - zero_dim_elem (0x0 bounds)
    // - invisible_btn (is_visible: false)
    // - disabled_btn (is_enabled: false)
    // - offscreen_btn (x = -32000, y = -32000)
    // - static_lbl (Text without clickability)
    // Only 3 valid buttons should receive marks: btn_submit, btn_reset, btn_help
    assert_eq!(
        count, 3,
        "Noisy layout should only mark the 3 valid interactive buttons"
    );
    assert_eq!(tree.interactive_count, 3);

    let interactive = tree.interactive_elements();
    assert_eq!(interactive[0].id, "btn_submit");
    assert_eq!(interactive[0].som_index, Some(1));
    assert_eq!(interactive[1].id, "btn_reset");
    assert_eq!(interactive[1].som_index, Some(2));
    assert_eq!(interactive[2].id, "btn_help");
    assert_eq!(interactive[2].som_index, Some(3));
}

#[tokio::test]
async fn test_som_index_to_cua_action_resolution() {
    let window_bounds = CuaRect::new(100, 100, 500, 400);
    let tree = PerceptionMockFixtures::standard_dialog(0x1003, window_bounds);

    // [4] is btn_ok at CuaRect::new(100 + 300, 100 + 180, 80, 32) = (400, 280, 80, 32)
    // Center is (400 + 40, 280 + 16) = (440, 296)
    let (center_pt, bounds) =
        PerceptionResolver::resolve_coordinate(&tree, 4).expect("resolve OK button");
    assert_eq!(center_pt.x, 440);
    assert_eq!(center_pt.y, 296);
    assert_eq!(bounds, CuaRect::new(400, 280, 80, 32));

    // Resolve into concrete CuaAction::Click
    let action_template = CuaAction::Click {
        target_hwnd: 0x1003,
        x: None,
        y: None,
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };

    let resolved_action = PerceptionResolver::resolve_action(&tree, 4, action_template)
        .expect("resolve_action into Click");

    match resolved_action {
        CuaAction::Click {
            target_hwnd,
            x,
            y,
            button,
            click_count,
            delivery_mode,
        } => {
            assert_eq!(target_hwnd, 0x1003);
            assert_eq!(x, Some(440));
            assert_eq!(y, Some(296));
            assert_eq!(button, MouseButton::Left);
            assert_eq!(click_count, 1);
            assert_eq!(delivery_mode, CuaDeliveryMode::Background);
        }
        _ => panic!("Expected CuaAction::Click"),
    }
}

#[tokio::test]
async fn test_automatic_visual_roi_fallback_on_empty_tree() {
    let driver = Arc::new(MockCuaDriver::new());
    let capturer = Arc::new(MockScreenCapturer::new(1920, 1080, CuaPixelFormat::Rgba, 0));

    let window_bounds = CuaRect::new(200, 200, 640, 480);
    let window_info = CuaWindowInfo {
        hwnd: 0x2001,
        pid: 4321,
        title: "WebGL Canvas Window".to_string(),
        class_name: "CustomSurface".to_string(),
        process_name: Some("game.exe".to_string()),
        bounds: window_bounds,
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    };
    driver.insert_window(window_info);

    // Insert an empty canvas UI tree (0 interactive elements)
    let empty_tree = PerceptionMockFixtures::empty_canvas(0x2001, window_bounds);
    driver.insert_ui_tree(0x2001, empty_tree);

    let engine = PerceptionEngine::new();
    let result = engine
        .perceive(0x2001, driver.as_ref(), Some(capturer.as_ref()))
        .await
        .expect("perceive canvas");

    match result {
        PerceptionResult::VisualFallback {
            window_bounds: b,
            frame,
            reason,
        } => {
            assert_eq!(b, window_bounds);
            assert!(frame.is_some(), "Expected frame captured via ROI fallback");
            let f = frame.unwrap();
            assert_eq!(f.width, 640);
            assert_eq!(f.height, 480);
            assert!(reason.contains("Zero interactive semantic elements found"));
        }
        PerceptionResult::Semantic { .. } => {
            panic!("Expected VisualFallback on empty canvas tree, got Semantic");
        }
    }
}

#[tokio::test]
async fn test_som_prompt_formatting() {
    let window_bounds = CuaRect::new(50, 50, 400, 300);
    let tree = PerceptionMockFixtures::standard_dialog(0x3001, window_bounds);

    let prompt = tree.format_som_prompt();
    assert!(prompt.contains("[1] Edit \"Optional Notes\" value=\"Pre-filled notes\""));
    assert!(prompt.contains("[2] CheckBox \"Do not ask again\""));
    assert!(prompt.contains("[3] Button \"Cancel\""));
    assert!(prompt.contains("[4] Button \"OK\""));
}

#[tokio::test]
async fn test_router_som_index_coordinate_resolution() {
    let driver = Arc::new(MockCuaDriver::new());
    let window_bounds = CuaRect::new(100, 100, 600, 400);

    let window_info = CuaWindowInfo {
        hwnd: 0x4001,
        pid: 5678,
        title: "Test Form".to_string(),
        class_name: "FormApp".to_string(),
        process_name: Some("form.exe".to_string()),
        bounds: window_bounds,
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    };
    driver.insert_window(window_info);

    // Insert standard dialog tree where [4] is "OK" button
    let tree = PerceptionMockFixtures::standard_dialog(0x4001, window_bounds);
    driver.insert_ui_tree(0x4001, tree);

    // Resolve target by SoM index 4
    let descriptor =
        TargetDescriptor::new(WindowSelector::Hwnd(0x4001), TargetCoordinate::som_index(4));

    let resolved = TargetResolver::resolve(&descriptor, driver.as_ref())
        .await
        .expect("TargetResolver resolves SomIndex(4)");

    assert_eq!(resolved.hwnd, 0x4001);
    // OK button bounds: x = 100 + 300 = 400, y = 100 + 180 = 280, w = 80, h = 32
    // Center: (440, 296)
    assert_eq!(resolved.screen_point, CuaPoint::new(440, 296));
    assert_eq!(resolved.expected_roi, Some(CuaRect::new(400, 280, 80, 32)));
    assert!(resolved.semantic_label.is_some());
}

#[tokio::test]
async fn test_multi_row_form_reading_order_with_jitter() {
    let window_bounds = CuaRect::new(50, 50, 600, 500);
    let mut tree = PerceptionMockFixtures::multi_row_form(0x5001, window_bounds);

    let count = SoMIndexer::index_tree_with_config(
        &mut tree,
        &SoMIndexerConfig {
            band_tolerance_px: 6,
            min_dimension: 2,
            include_static_text: false,
        },
    );
    assert_eq!(count, 5);

    // Verify reading order:
    // Row 1 (y ~ 70): [1] First Name (x = 70), [2] Last Name (x = 240)
    // Row 2 (y ~ 120): [3] Email (x = 70)
    // Row 3 (y ~ 180): [4] Back button (x = 220), [5] Next button (x = 310)
    let interactive = tree.interactive_elements();
    assert_eq!(interactive[0].id, "in_first_name");
    assert_eq!(interactive[1].id, "in_last_name");
    assert_eq!(interactive[2].id, "in_email");
    assert_eq!(interactive[3].id, "btn_back");
    assert_eq!(interactive[4].id, "btn_next");
}

#[tokio::test]
async fn test_cua_engine_perception_facade() {
    let driver = Arc::new(MockCuaDriver::new());
    let window_bounds = CuaRect::new(100, 100, 500, 400);

    let window_info = CuaWindowInfo {
        hwnd: 0x6001,
        pid: 9999,
        title: "Dialog Facade".to_string(),
        class_name: "DialogClass".to_string(),
        process_name: Some("facade.exe".to_string()),
        bounds: window_bounds,
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    };
    driver.insert_window(window_info);

    let tree = PerceptionMockFixtures::standard_dialog(0x6001, window_bounds);
    driver.insert_ui_tree(0x6001, tree);

    let engine = CuaEngine::new_with_driver(CuaConfig::default(), driver.clone());

    // Test inspect_ui_tree
    let inspected = engine
        .inspect_ui_tree(0x6001)
        .await
        .expect("inspect_ui_tree");
    assert_eq!(inspected.interactive_count, 4);

    // Test perceive_target
    let perception = engine
        .perceive_target(0x6001, None)
        .await
        .expect("perceive_target");
    assert!(perception.is_semantic());
    assert_eq!(perception.som_count(), 4);

    // Test resolve_som_action
    let action_template = CuaAction::Click {
        target_hwnd: 0x6001,
        x: None,
        y: None,
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };
    let resolved = engine
        .resolve_som_action(&inspected, 1, action_template)
        .expect("resolve_som_action");

    match resolved {
        CuaAction::Click { x, y, .. } => {
            assert!(x.is_some());
            assert!(y.is_some());
        }
        _ => panic!("Expected Click action"),
    }
}
