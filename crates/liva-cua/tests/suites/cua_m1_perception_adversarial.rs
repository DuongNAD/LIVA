//! Empirical Adversarial Test Suite for Dual-Targeting Perception (M1) in crates/liva-cua.
//!
//! Stress-tests:
//! 1. PerceptionEngine::perceive under degenerate topologies (empty tree, non-interactive containers,
//!    all-invisible, all-disabled, zero/subminimal dimensions, offscreen sentinels, minimized windows).
//! 2. Clean triggering of PerceptionResult::VisualFallback without panic, error, or memory corruption.
//! 3. Out-of-range, zero, negative-equivalent, and extreme SoM index resolution across
//!    PerceptionResolver, CuaEngine, and TargetResolver.
//! 4. SoMIndexer idempotency, deterministic sorting tie-breaking, and scale stress (100+ nodes).

use async_trait::async_trait;
use liva_cua::mock::MockCuaDriver;
use liva_cua::perception::{
    PerceptionEngine, PerceptionMockFixtures, PerceptionResolver, PerceptionResult, SoMIndexer,
    UiElement, UiRole, UiState, UiTree,
};
use liva_cua::router::{
    PreCheckError, TargetCoordinate, TargetDescriptor, TargetResolver, WindowSelector,
};
use liva_cua::traits::CuaDriverTrait;
use liva_cua::types::{
    CuaAction, CuaActionResult, CuaConfig, CuaDeliveryMode, CuaError, CuaPoint, CuaRect,
    CuaWindowInfo, MouseButton, WindowStateVerdict,
};
use liva_cua::vision::mock::MockScreenCapturer;
use liva_cua::vision::types::CuaPixelFormat;
use liva_cua::CuaEngine;
use std::sync::Arc;

// ============================================================================
// 1. Degenerate Topologies & Visual Fallback in PerceptionEngine::perceive
// ============================================================================

#[tokio::test]
async fn test_adversarial_perceive_empty_tree_clean_fallback() {
    let driver = Arc::new(MockCuaDriver::new());
    let capturer = Arc::new(MockScreenCapturer::new(1920, 1080, CuaPixelFormat::Rgba, 0));

    let window_bounds = CuaRect::new(100, 100, 500, 400);
    let window_info = CuaWindowInfo {
        hwnd: 0x7001,
        pid: 1111,
        title: "Empty Window Host".to_string(),
        class_name: "HostApp".to_string(),
        process_name: Some("host.exe".to_string()),
        bounds: window_bounds,
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    };
    driver.insert_window(window_info);

    // Empty tree with no interactive nodes
    let empty_tree = UiTree::empty(0x7001);
    driver.insert_ui_tree(0x7001, empty_tree);

    let engine = PerceptionEngine::new();
    let result = engine
        .perceive(0x7001, driver.as_ref(), Some(capturer.as_ref()))
        .await
        .expect("perceive on empty tree should succeed with fallback");

    assert!(result.is_visual_fallback());
    assert!(!result.is_semantic());
    assert_eq!(result.som_count(), 0);
    assert!(result.tree().is_none());

    match result {
        PerceptionResult::VisualFallback {
            window_bounds: b,
            frame,
            reason,
        } => {
            assert_eq!(b, window_bounds);
            assert!(frame.is_some(), "Expected frame captured via fallback");
            let f = frame.unwrap();
            assert_eq!(f.width, 500);
            assert_eq!(f.height, 400);
            assert!(
                reason.contains("Zero interactive semantic elements found"),
                "Reason must document zero elements: {}",
                reason
            );
        }
        PerceptionResult::Semantic { .. } => panic!("Expected VisualFallback, got Semantic"),
    }
}

#[tokio::test]
async fn test_adversarial_perceive_noninteractive_containers_only() {
    let driver = Arc::new(MockCuaDriver::new());
    let capturer = Arc::new(MockScreenCapturer::new(1920, 1080, CuaPixelFormat::Rgba, 0));

    let window_bounds = CuaRect::new(50, 50, 600, 500);
    let window_info = CuaWindowInfo {
        hwnd: 0x7002,
        pid: 2222,
        title: "Containers Host".to_string(),
        class_name: "ContainerApp".to_string(),
        process_name: Some("container.exe".to_string()),
        bounds: window_bounds,
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    };
    driver.insert_window(window_info);

    // Deep hierarchy of layout containers with NO clickable or checkable elements
    let outer_pane = UiElement::new("pane_1", UiRole::Pane, "Pane 1", window_bounds)
        .with_state(UiState::static_display());
    let group = UiElement::new("group_1", UiRole::Group, "Group 1", window_bounds)
        .with_state(UiState::static_display());
    let custom_surface = UiElement::new("custom_1", UiRole::Custom, "Custom 1", window_bounds)
        .with_state(UiState::static_display());
    let dialog_inner = UiElement::new("dlg_1", UiRole::Dialog, "Dialog 1", window_bounds)
        .with_state(UiState::static_display())
        .with_child(custom_surface);

    let root = outer_pane.with_child(group.with_child(dialog_inner));
    let mut tree = UiTree::new(0x7002, root);

    let marked = SoMIndexer::index_tree(&mut tree);
    assert_eq!(
        marked, 0,
        "Non-interactive containers must produce 0 marked candidates"
    );

    driver.insert_ui_tree(0x7002, tree);

    let engine = PerceptionEngine::new();
    let result = engine
        .perceive(0x7002, driver.as_ref(), Some(capturer.as_ref()))
        .await
        .expect("perceive on container-only tree must succeed");

    assert!(result.is_visual_fallback());
    assert_eq!(result.som_count(), 0);
}

#[tokio::test]
async fn test_adversarial_perceive_all_invisible_or_disabled_elements() {
    let driver = Arc::new(MockCuaDriver::new());
    let capturer = Arc::new(MockScreenCapturer::new(1920, 1080, CuaPixelFormat::Rgba, 0));

    let window_bounds = CuaRect::new(0, 0, 800, 600);
    let window_info = CuaWindowInfo {
        hwnd: 0x7003,
        pid: 3333,
        title: "Ghost Controls".to_string(),
        class_name: "GhostApp".to_string(),
        process_name: Some("ghost.exe".to_string()),
        bounds: window_bounds,
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    };
    driver.insert_window(window_info);

    // 6 interactive control types, but 3 are invisible and 3 are disabled
    let btn_hidden = UiElement::new(
        "btn_h",
        UiRole::Button,
        "Hidden Btn",
        CuaRect::new(10, 10, 80, 30),
    )
    .with_state(UiState::hidden());
    let edit_hidden = UiElement::new(
        "edit_h",
        UiRole::Edit,
        "Hidden Edit",
        CuaRect::new(10, 50, 150, 30),
    )
    .with_state(UiState::hidden());
    let chk_hidden = UiElement::new(
        "chk_h",
        UiRole::CheckBox,
        "Hidden Chk",
        CuaRect::new(10, 90, 120, 24),
    )
    .with_state(UiState::hidden());

    let btn_disabled = UiElement::new(
        "btn_d",
        UiRole::Button,
        "Disabled Btn",
        CuaRect::new(200, 10, 80, 30),
    )
    .with_state(UiState::disabled());
    let edit_disabled = UiElement::new(
        "edit_d",
        UiRole::Edit,
        "Disabled Edit",
        CuaRect::new(200, 50, 150, 30),
    )
    .with_state(UiState::disabled());
    let chk_disabled = UiElement::new(
        "chk_d",
        UiRole::CheckBox,
        "Disabled Chk",
        CuaRect::new(200, 90, 120, 24),
    )
    .with_state(UiState::disabled());

    let root =
        UiElement::new("root", UiRole::Window, "Ghost Window", window_bounds).with_children(vec![
            btn_hidden,
            edit_hidden,
            chk_hidden,
            btn_disabled,
            edit_disabled,
            chk_disabled,
        ]);

    let mut tree = UiTree::new(0x7003, root);
    let count = SoMIndexer::index_tree(&mut tree);
    assert_eq!(
        count, 0,
        "Invisible and disabled controls must all be pruned"
    );

    driver.insert_ui_tree(0x7003, tree);

    let engine = PerceptionEngine::new();
    let result = engine
        .perceive(0x7003, driver.as_ref(), Some(capturer.as_ref()))
        .await
        .expect("perceive on ghost controls");

    assert!(result.is_visual_fallback());
}

#[tokio::test]
async fn test_adversarial_perceive_all_zero_or_subminimal_dimensions() {
    let window_bounds = CuaRect::new(0, 0, 640, 480);

    let zero_wh = UiElement::new(
        "zero_wh",
        UiRole::Button,
        "Zero WH",
        CuaRect::new(10, 10, 0, 0),
    )
    .with_state(UiState::clickable());
    let zero_w = UiElement::new(
        "zero_w",
        UiRole::Button,
        "Zero W",
        CuaRect::new(10, 10, 0, 30),
    )
    .with_state(UiState::clickable());
    let zero_h = UiElement::new(
        "zero_h",
        UiRole::Button,
        "Zero H",
        CuaRect::new(10, 10, 80, 0),
    )
    .with_state(UiState::clickable());
    let subminimal_1x1 = UiElement::new(
        "sub_1x1",
        UiRole::Button,
        "Subminimal 1x1",
        CuaRect::new(10, 10, 1, 1),
    )
    .with_state(UiState::clickable());

    let root = UiElement::new("root", UiRole::Window, "Root", window_bounds).with_children(vec![
        zero_wh,
        zero_w,
        zero_h,
        subminimal_1x1,
    ]);

    let mut tree = UiTree::new(0x7004, root);
    let count = SoMIndexer::index_tree(&mut tree);
    assert_eq!(
        count, 0,
        "Zero and 1x1 subminimal controls must not be indexed"
    );
}

#[tokio::test]
async fn test_adversarial_perceive_all_offscreen_or_minimized_sentinels() {
    let window_bounds = CuaRect::new(0, 0, 800, 600);

    let offscreen_extreme = UiElement::new(
        "off_extreme",
        UiRole::Button,
        "Extreme Neg",
        CuaRect::new(-32000, -32000, 80, 30),
    )
    .with_state(UiState::clickable());
    let offscreen_threshold_x = UiElement::new(
        "off_tx",
        UiRole::Button,
        "Off X Threshold",
        CuaRect::new(-10001, 100, 80, 30),
    )
    .with_state(UiState::clickable());
    let offscreen_threshold_y = UiElement::new(
        "off_ty",
        UiRole::Button,
        "Off Y Threshold",
        CuaRect::new(100, -10001, 80, 30),
    )
    .with_state(UiState::clickable());

    let root = UiElement::new("root", UiRole::Window, "Root", window_bounds).with_children(vec![
        offscreen_extreme,
        offscreen_threshold_x,
        offscreen_threshold_y,
    ]);

    let mut tree = UiTree::new(0x7005, root);
    let count = SoMIndexer::index_tree(&mut tree);
    assert_eq!(
        count, 0,
        "All offscreen sentinels (<= -10000) must be rejected"
    );
}

#[tokio::test]
async fn test_adversarial_perceive_minimized_window_avoids_invalid_roi_capture() {
    let driver = Arc::new(MockCuaDriver::new());
    let capturer = Arc::new(MockScreenCapturer::new(1920, 1080, CuaPixelFormat::Rgba, 0));

    // Minimized window sentinel in Win32 CUA
    let minimized_bounds = CuaRect::new(-32000, -32000, 0, 0);
    assert!(minimized_bounds.is_minimized_sentinel());

    let window_info = CuaWindowInfo {
        hwnd: 0x7006,
        pid: 4444,
        title: "Minimized App".to_string(),
        class_name: "MinAppClass".to_string(),
        process_name: Some("min.exe".to_string()),
        bounds: minimized_bounds,
        is_on_screen: false,
        is_minimized: true,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    };
    driver.insert_window(window_info);

    let empty_tree = UiTree::empty(0x7006);
    driver.insert_ui_tree(0x7006, empty_tree);

    let engine = PerceptionEngine::new();
    let result = engine
        .perceive(0x7006, driver.as_ref(), Some(capturer.as_ref()))
        .await
        .expect("perceive on minimized window must not crash or error");

    match result {
        PerceptionResult::VisualFallback {
            window_bounds,
            frame,
            ..
        } => {
            assert!(window_bounds.is_minimized_sentinel());
            // Must have called full-screen capture rather than attempting an ROI capture of (-32000, -32000)
            assert!(frame.is_some());
            let f = frame.unwrap();
            assert_eq!(
                f.width, 1920,
                "Minimized window fallback must capture full screen"
            );
            assert_eq!(
                f.height, 1080,
                "Minimized window fallback must capture full screen"
            );
        }
        _ => panic!("Expected VisualFallback"),
    }
}

struct FailingTreeDriver;

#[async_trait]
impl CuaDriverTrait for FailingTreeDriver {
    async fn list_windows(&self, _filter_pid: Option<u32>) -> Result<Vec<CuaWindowInfo>, CuaError> {
        Ok(Vec::new())
    }
    async fn inspect_window(&self, hwnd: u64) -> Result<WindowStateVerdict, CuaError> {
        Ok(WindowStateVerdict::NotFound { hwnd })
    }
    async fn dispatch_action(&self, _action: CuaAction) -> Result<CuaActionResult, CuaError> {
        Err(CuaError::WindowNotFound(0))
    }
    async fn restore_window(&self, _hwnd: u64) -> Result<(), CuaError> {
        Ok(())
    }
    async fn scroll_container(
        &self,
        _hwnd: u64,
        _delta_x: i32,
        _delta_y: i32,
    ) -> Result<(), CuaError> {
        Ok(())
    }
    async fn inspect_ui_tree(&self, hwnd: u64) -> Result<UiTree, CuaError> {
        Err(CuaError::WindowNotFound(hwnd))
    }
}

#[tokio::test]
async fn test_adversarial_perceive_failing_driver_propagates_error() {
    let driver = FailingTreeDriver;
    let capturer = Arc::new(MockScreenCapturer::new(1920, 1080, CuaPixelFormat::Rgba, 0));

    let engine = PerceptionEngine::new();
    let result = engine
        .perceive(0xDEAD_BEEF, &driver, Some(capturer.as_ref()))
        .await;

    assert!(
        result.is_err(),
        "Perception must propagate CuaError when inspect_ui_tree fails"
    );
    match result.unwrap_err() {
        CuaError::WindowNotFound(hwnd) => assert_eq!(hwnd, 0xDEAD_BEEF),
        other => panic!("Expected WindowNotFound, got {:?}", other),
    }
}

#[tokio::test]
async fn test_adversarial_perceive_unregistered_window_graceful_fallback() {
    let driver = Arc::new(MockCuaDriver::new());
    let capturer = Arc::new(MockScreenCapturer::new(1920, 1080, CuaPixelFormat::Rgba, 0));

    let engine = PerceptionEngine::new();
    // HWND 0xDEAD_BEEF is unregistered in MockCuaDriver; driver returns empty tree
    let result = engine
        .perceive(0xDEAD_BEEF, driver.as_ref(), Some(capturer.as_ref()))
        .await
        .expect("Unregistered window with default empty tree must trigger fallback gracefully");

    assert!(result.is_visual_fallback());
    match result {
        PerceptionResult::VisualFallback {
            window_bounds,
            frame,
            reason,
        } => {
            assert!(window_bounds.is_empty());
            assert!(frame.is_some()); // Fullscreen capture triggered
            assert!(reason.contains("Zero interactive semantic elements found"));
        }
        _ => panic!("Expected VisualFallback"),
    }
}

#[tokio::test]
async fn test_adversarial_perceive_without_capturer_clean_fallback() {
    let driver = Arc::new(MockCuaDriver::new());

    let window_bounds = CuaRect::new(100, 100, 400, 300);
    let window_info = CuaWindowInfo {
        hwnd: 0x7007,
        pid: 5555,
        title: "Headless Canvas".to_string(),
        class_name: "HeadlessApp".to_string(),
        process_name: Some("headless.exe".to_string()),
        bounds: window_bounds,
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    };
    driver.insert_window(window_info);

    let empty_tree = UiTree::empty(0x7007);
    driver.insert_ui_tree(0x7007, empty_tree);

    let engine = PerceptionEngine::new();
    // capturer is None
    let result = engine
        .perceive(0x7007, driver.as_ref(), None)
        .await
        .expect("perceive without capturer must succeed");

    match result {
        PerceptionResult::VisualFallback {
            window_bounds: b,
            frame,
            reason,
        } => {
            assert_eq!(b, window_bounds);
            assert!(
                frame.is_none(),
                "Frame must be None when no capturer provided"
            );
            assert!(reason.contains("Zero interactive semantic elements found"));
        }
        _ => panic!("Expected VisualFallback"),
    }
}

// ============================================================================
// 2. Out-of-Range, Zero, Negative-Equivalent, and Extreme SoM Indices
// ============================================================================

#[tokio::test]
async fn test_adversarial_resolve_som_index_zero() {
    let window_bounds = CuaRect::new(100, 100, 500, 400);
    let tree = PerceptionMockFixtures::standard_dialog(0x8001, window_bounds);
    assert_eq!(tree.interactive_count, 4);

    // 1. PerceptionResolver::resolve_coordinate with index 0
    let res = PerceptionResolver::resolve_coordinate(&tree, 0);
    assert!(res.is_err(), "Index 0 must be rejected");
    match res.unwrap_err() {
        CuaError::InvalidParameter(msg) => {
            assert!(msg.contains("SoM mark [0] not found in UI tree"));
        }
        other => panic!("Expected InvalidParameter, got {:?}", other),
    }

    // 2. PerceptionResolver::resolve_action with index 0
    let action_template = CuaAction::Click {
        target_hwnd: 0x8001,
        x: None,
        y: None,
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };
    let act_res = PerceptionResolver::resolve_action(&tree, 0, action_template);
    assert!(
        act_res.is_err(),
        "resolve_action with index 0 must be rejected"
    );

    // 3. TargetResolver::resolve with index 0
    let driver = Arc::new(MockCuaDriver::new());
    let window_info = CuaWindowInfo {
        hwnd: 0x8001,
        pid: 6666,
        title: "Dialog 8001".to_string(),
        class_name: "DialogApp".to_string(),
        process_name: Some("dialog.exe".to_string()),
        bounds: window_bounds,
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    };
    driver.insert_window(window_info);
    driver.insert_ui_tree(0x8001, tree);

    let descriptor =
        TargetDescriptor::new(WindowSelector::Hwnd(0x8001), TargetCoordinate::som_index(0));
    let target_res = TargetResolver::resolve(&descriptor, driver.as_ref()).await;
    assert!(
        target_res.is_err(),
        "TargetResolver with SomIndex(0) must fail"
    );
    match target_res.unwrap_err() {
        PreCheckError::WindowNotFound { detail, .. } => {
            assert!(
                detail.contains("[0] not found"),
                "Detail must note index 0 not found: {}",
                detail
            );
        }
        other => panic!("Expected WindowNotFound, got {:?}", other),
    }
}

#[tokio::test]
async fn test_adversarial_resolve_som_index_out_of_bounds_high() {
    let window_bounds = CuaRect::new(100, 100, 500, 400);
    let tree = PerceptionMockFixtures::standard_dialog(0x8002, window_bounds);
    assert_eq!(tree.interactive_count, 4);

    // Index N+1 = 5
    let res5 = PerceptionResolver::resolve_coordinate(&tree, 5);
    assert!(res5.is_err(), "Index 5 (N+1) must fail");
    assert!(matches!(
        res5.unwrap_err(),
        CuaError::InvalidParameter(ref msg) if msg.contains("[5]")
    ));

    // Arbitrary high index 999
    let res999 = PerceptionResolver::resolve_coordinate(&tree, 999);
    assert!(res999.is_err(), "Index 999 must fail");
    assert!(matches!(
        res999.unwrap_err(),
        CuaError::InvalidParameter(ref msg) if msg.contains("[999]")
    ));
}

#[tokio::test]
async fn test_adversarial_resolve_som_index_usize_max_no_panic() {
    let window_bounds = CuaRect::new(100, 100, 500, 400);
    let tree = PerceptionMockFixtures::standard_dialog(0x8003, window_bounds);

    // Test with usize::MAX without panic or overflow
    let res = PerceptionResolver::resolve_coordinate(&tree, usize::MAX);
    assert!(res.is_err(), "Index usize::MAX must fail gracefully");
    assert!(matches!(res.unwrap_err(), CuaError::InvalidParameter(_)));
}

#[tokio::test]
async fn test_adversarial_resolve_all_action_variants() {
    let window_bounds = CuaRect::new(100, 100, 500, 400);
    let tree = PerceptionMockFixtures::standard_dialog(0x8004, window_bounds);

    // [3] is btn_cancel at CuaRect::new(100 + 210, 100 + 180, 80, 32) = (310, 280, 80, 32)
    // Center: (310 + 40, 280 + 16) = (350, 296)
    let som_idx = 3;

    // 1. MoveCursor
    let act = CuaAction::MoveCursor {
        target_hwnd: 0x8004,
        x: 0,
        y: 0,
        delivery_mode: CuaDeliveryMode::Background,
    };
    let resolved =
        PerceptionResolver::resolve_action(&tree, som_idx, act).expect("resolve MoveCursor");
    match resolved {
        CuaAction::MoveCursor { x, y, .. } => {
            assert_eq!(x, 350);
            assert_eq!(y, 296);
        }
        _ => panic!("Expected MoveCursor"),
    }

    // 2. Drag
    let act = CuaAction::Drag {
        target_hwnd: 0x8004,
        start_x: 0,
        start_y: 0,
        end_x: 500,
        end_y: 500,
        button: MouseButton::Left,
        steps: 5,
        delivery_mode: CuaDeliveryMode::Background,
    };
    let resolved = PerceptionResolver::resolve_action(&tree, som_idx, act).expect("resolve Drag");
    match resolved {
        CuaAction::Drag {
            start_x,
            start_y,
            end_x,
            end_y,
            ..
        } => {
            assert_eq!(start_x, 350);
            assert_eq!(start_y, 296);
            assert_eq!(end_x, 500);
            assert_eq!(end_y, 500);
        }
        _ => panic!("Expected Drag"),
    }

    // 3. Scroll
    let act = CuaAction::Scroll {
        target_hwnd: 0x8004,
        delta_x: 0,
        delta_y: 120,
        x: None,
        y: None,
    };
    let resolved = PerceptionResolver::resolve_action(&tree, som_idx, act).expect("resolve Scroll");
    match resolved {
        CuaAction::Scroll { x, y, .. } => {
            assert_eq!(x, Some(350));
            assert_eq!(y, Some(296));
        }
        _ => panic!("Expected Scroll"),
    }

    // 4. TypeText
    let act = CuaAction::TypeText {
        target_hwnd: 0x8004,
        text: "Testing".to_string(),
        delivery_mode: CuaDeliveryMode::Background,
    };
    let resolved =
        PerceptionResolver::resolve_action(&tree, som_idx, act).expect("resolve TypeText");
    assert!(matches!(resolved, CuaAction::TypeText { .. }));

    // 5. PressKey
    let act = CuaAction::PressKey {
        target_hwnd: 0x8004,
        key: "Enter".to_string(),
        down: true,
        delivery_mode: CuaDeliveryMode::Background,
    };
    let resolved =
        PerceptionResolver::resolve_action(&tree, som_idx, act).expect("resolve PressKey");
    assert!(matches!(resolved, CuaAction::PressKey { .. }));

    // 6. Hotkey
    let act = CuaAction::Hotkey {
        target_hwnd: 0x8004,
        keys: vec!["Ctrl".to_string(), "A".to_string()],
        delivery_mode: CuaDeliveryMode::Background,
    };
    let resolved = PerceptionResolver::resolve_action(&tree, som_idx, act).expect("resolve Hotkey");
    assert!(matches!(resolved, CuaAction::Hotkey { .. }));

    // 7. BringToFront
    let act = CuaAction::BringToFront {
        target_hwnd: 0x8004,
    };
    let resolved =
        PerceptionResolver::resolve_action(&tree, som_idx, act).expect("resolve BringToFront");
    assert!(matches!(resolved, CuaAction::BringToFront { .. }));

    // 8. RestoreWindow
    let act = CuaAction::RestoreWindow {
        target_hwnd: 0x8004,
    };
    let resolved =
        PerceptionResolver::resolve_action(&tree, som_idx, act).expect("resolve RestoreWindow");
    assert!(matches!(resolved, CuaAction::RestoreWindow { .. }));
}

#[tokio::test]
async fn test_adversarial_containment_validation_negative_or_disjoint_elements() {
    let window_rect = CuaRect::new(100, 100, 500, 400);

    // Inside
    let inside_elem = CuaRect::new(120, 120, 80, 30); // center (160, 135) -> inside [100..600, 100..500]
    assert!(PerceptionResolver::validate_containment(
        &inside_elem,
        &window_rect
    ));

    // Outside (top-left)
    let outside_tl = CuaRect::new(20, 20, 50, 50); // center (45, 45) -> outside
    assert!(!PerceptionResolver::validate_containment(
        &outside_tl,
        &window_rect
    ));

    // Outside (negative coordinates)
    let outside_neg = CuaRect::new(-100, -100, 50, 50);
    assert!(!PerceptionResolver::validate_containment(
        &outside_neg,
        &window_rect
    ));
}

// ============================================================================
// 3. SoMIndexer Idempotency, Scale Stress, and Tie-Breaking
// ============================================================================

#[tokio::test]
async fn test_adversarial_indexer_idempotency_multiple_passes() {
    let window_bounds = CuaRect::new(100, 100, 500, 400);
    let mut tree = PerceptionMockFixtures::standard_dialog(0x9001, window_bounds);

    let prompt_initial = tree.format_som_prompt();

    // Re-index 10 consecutive times
    for _ in 0..10 {
        let count = SoMIndexer::index_tree(&mut tree);
        assert_eq!(
            count, 4,
            "Every re-indexing pass must yield exactly 4 marks"
        );
        assert_eq!(tree.interactive_count, 4);
    }

    let prompt_final = tree.format_som_prompt();
    assert_eq!(
        prompt_initial, prompt_final,
        "Prompt formatting must be strictly deterministic across re-indexing passes"
    );
}

#[tokio::test]
async fn test_adversarial_large_tree_stress_100_elements() {
    let window_bounds = CuaRect::new(0, 0, 1000, 1000);
    let mut children = Vec::with_capacity(100);

    // 10 rows of 10 buttons each
    for row in 0..10 {
        let y = 50 + (row * 60);
        for col in 0..10 {
            let x = 50 + (col * 80);
            let id = format!("btn_r{}_c{}", row, col);
            let name = format!("Button R{}C{}", row, col);
            let btn = UiElement::new(id, UiRole::Button, name, CuaRect::new(x, y, 70, 40))
                .with_state(UiState::clickable());
            children.push(btn);
        }
    }

    let root = UiElement::new("grid_root", UiRole::Window, "100-Grid", window_bounds)
        .with_children(children);
    let mut tree = UiTree::new(0x9002, root);

    let count = SoMIndexer::index_tree(&mut tree);
    assert_eq!(count, 100, "Must index all 100 interactive buttons");
    assert_eq!(tree.interactive_count, 100);

    // Verify all marks 1..100 exist and are sequential
    for mark in 1..=100 {
        let found = tree.find_by_som(mark);
        assert!(found.is_some(), "Element with mark [{}] must exist", mark);
        assert_eq!(found.unwrap().som_index, Some(mark));
    }

    // Boundary checks
    assert!(tree.find_by_som(0).is_none());
    assert!(tree.find_by_som(101).is_none());
}

#[tokio::test]
async fn test_adversarial_duplicate_coordinates_stable_tie_breaking() {
    let window_bounds = CuaRect::new(0, 0, 800, 600);

    // 3 buttons at exact identical bounding box (100, 100, 80, 30)
    let btn_c = UiElement::new(
        "id_charlie",
        UiRole::Button,
        "Charlie",
        CuaRect::new(100, 100, 80, 30),
    )
    .with_state(UiState::clickable());
    let btn_a = UiElement::new(
        "id_alpha",
        UiRole::Button,
        "Alpha",
        CuaRect::new(100, 100, 80, 30),
    )
    .with_state(UiState::clickable());
    let btn_b = UiElement::new(
        "id_bravo",
        UiRole::Button,
        "Bravo",
        CuaRect::new(100, 100, 80, 30),
    )
    .with_state(UiState::clickable());

    let root = UiElement::new("dup_root", UiRole::Window, "Dups", window_bounds)
        .with_children(vec![btn_c, btn_a, btn_b]);

    let mut tree = UiTree::new(0x9003, root);
    let count = SoMIndexer::index_tree(&mut tree);
    assert_eq!(count, 3);

    // Tie-breaking is by ID ascending: id_alpha -> id_bravo -> id_charlie
    let interactive = tree.interactive_elements();
    assert_eq!(interactive[0].id, "id_alpha");
    assert_eq!(interactive[0].som_index, Some(1));
    assert_eq!(interactive[1].id, "id_bravo");
    assert_eq!(interactive[1].som_index, Some(2));
    assert_eq!(interactive[2].id, "id_charlie");
    assert_eq!(interactive[2].som_index, Some(3));
}

// ============================================================================
// 4. Router End-to-End Resolution with SomIndex Coordinate Variant
// ============================================================================

#[tokio::test]
async fn test_adversarial_router_som_index_missing_window() {
    let driver = Arc::new(MockCuaDriver::new());

    let descriptor = TargetDescriptor::new(
        WindowSelector::Hwnd(0x9999_DEAD),
        TargetCoordinate::som_index(1),
    );

    let res = TargetResolver::resolve(&descriptor, driver.as_ref()).await;
    assert!(res.is_err());
    assert!(matches!(
        res.unwrap_err(),
        PreCheckError::WindowNotFound { .. }
    ));
}

#[tokio::test]
async fn test_adversarial_router_som_index_in_empty_tree() {
    let driver = Arc::new(MockCuaDriver::new());
    let window_bounds = CuaRect::new(100, 100, 500, 400);

    let window_info = CuaWindowInfo {
        hwnd: 0xA001,
        pid: 7777,
        title: "Empty Surface".to_string(),
        class_name: "SurfaceApp".to_string(),
        process_name: Some("surf.exe".to_string()),
        bounds: window_bounds,
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    };
    driver.insert_window(window_info);

    let empty_tree = UiTree::empty(0xA001);
    driver.insert_ui_tree(0xA001, empty_tree);

    let descriptor =
        TargetDescriptor::new(WindowSelector::Hwnd(0xA001), TargetCoordinate::som_index(1));

    let res = TargetResolver::resolve(&descriptor, driver.as_ref()).await;
    assert!(res.is_err());
    match res.unwrap_err() {
        PreCheckError::WindowNotFound { detail, .. } => {
            assert!(
                detail.contains("[1] not found in UI tree"),
                "Detail must explain element not found in tree: {}",
                detail
            );
        }
        other => panic!("Expected WindowNotFound, got {:?}", other),
    }
}

#[tokio::test]
async fn test_adversarial_router_som_index_override_with_expected_roi() {
    let driver = Arc::new(MockCuaDriver::new());
    let window_bounds = CuaRect::new(100, 100, 600, 400);

    let window_info = CuaWindowInfo {
        hwnd: 0xA002,
        pid: 8888,
        title: "Roi Override Host".to_string(),
        class_name: "OverrideApp".to_string(),
        process_name: Some("override.exe".to_string()),
        bounds: window_bounds,
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    };
    driver.insert_window(window_info);

    // If expected_roi is explicitly provided, it overrides and resolves to expected_roi center
    let explicit_roi = CuaRect::new(200, 200, 100, 50); // center: (250, 225)
    let mut descriptor = TargetDescriptor::new(
        WindowSelector::Hwnd(0xA002),
        TargetCoordinate::som_index(999), // out-of-range index, but expected_roi is supplied
    );
    descriptor.expected_roi = Some(explicit_roi);

    let resolved = TargetResolver::resolve(&descriptor, driver.as_ref())
        .await
        .expect("Explicit expected_roi resolution");

    assert_eq!(resolved.screen_point, CuaPoint::new(250, 225));
    assert_eq!(resolved.expected_roi, Some(explicit_roi));
}

#[tokio::test]
async fn test_adversarial_cua_engine_facade_som_error_handling() {
    let driver = Arc::new(MockCuaDriver::new());
    let window_bounds = CuaRect::new(100, 100, 500, 400);

    let window_info = CuaWindowInfo {
        hwnd: 0xA003,
        pid: 9999,
        title: "Facade Error Host".to_string(),
        class_name: "FacadeClass".to_string(),
        process_name: Some("facade.exe".to_string()),
        bounds: window_bounds,
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    };
    driver.insert_window(window_info);

    let tree = PerceptionMockFixtures::standard_dialog(0xA003, window_bounds);
    driver.insert_ui_tree(0xA003, tree.clone());

    let engine = CuaEngine::new_with_driver(CuaConfig::default(), driver.clone());

    let action_template = CuaAction::Click {
        target_hwnd: 0xA003,
        x: None,
        y: None,
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };

    // Valid index 1
    let ok_res = engine.resolve_som_action(&tree, 1, action_template.clone());
    assert!(ok_res.is_ok());

    // Invalid index 0
    let err_zero = engine.resolve_som_action(&tree, 0, action_template.clone());
    assert!(err_zero.is_err());
    assert!(matches!(
        err_zero.unwrap_err(),
        CuaError::InvalidParameter(ref msg) if msg.contains("[0]")
    ));

    // Invalid index 10
    let err_ten = engine.resolve_som_action(&tree, 10, action_template);
    assert!(err_ten.is_err());
    assert!(matches!(
        err_ten.unwrap_err(),
        CuaError::InvalidParameter(ref msg) if msg.contains("[10]")
    ));
}
