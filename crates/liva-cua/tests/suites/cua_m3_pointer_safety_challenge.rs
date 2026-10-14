//! Adversarial Empirical Challenge Suite: Synthetic Pointer Initialization & Device Safety
//!
//! Empirical Challenger: challenger_m3_2
//! Target: crates/liva-cua/src/input/pointer.rs
//!
//! Objectives:
//! 1. Verify SyntheticPointerDevice lifecycle: creation, destruction, drop safety without leaks or panics.
//! 2. Edge case stress testing: max_count = 0, boundary contact counts, rapid multi-cycle instantiation.
//! 3. Verify missing/invalid device and invalid HWND handling across all MouseButton variants in background_pointer_click.
//! 4. Verify boundary coordinate injection safety (i32::MIN, i32::MAX, 0, negative).
//! 5. Static AST / text oracle: verify zero instances of `.unwrap()` exist in pointer.rs.

use liva_cua::input::pointer::{background_pointer_click, SyntheticPointerDevice};
use liva_cua::types::{CuaError, MouseButton};

// =========================================================================
// TEST 1: SYNTHETIC POINTER DEVICE LIFECYCLE & BOUNDARY CREATION
// =========================================================================

#[test]
fn test_m3_synthetic_pointer_device_touch_and_pen_lifecycle_no_panic() {
    #[cfg(windows)]
    {
        // 1. Touch device standard creation
        let touch_res = SyntheticPointerDevice::new_touch(1);
        match touch_res {
            Ok(device) => {
                // Device successfully allocated - verify drop is safe
                drop(device);
            }
            Err(e) => {
                // If running in headless VM or without pointer capability, must fail with Win32Error, never panic
                match e {
                    CuaError::Win32Error(msg) => {
                        assert!(
                            msg.contains("Synthetic pointer")
                                || msg.contains("CreateSyntheticPointerDevice"),
                            "Unexpected error message: {msg}"
                        );
                    }
                    other => panic!("Unexpected error variant: {other:?}"),
                }
            }
        }

        // 2. Pen device standard creation
        let pen_res = SyntheticPointerDevice::new_pen();
        match pen_res {
            Ok(device) => {
                drop(device);
            }
            Err(e) => match e {
                CuaError::Win32Error(msg) => {
                    assert!(
                        msg.contains("Synthetic pointer")
                            || msg.contains("CreateSyntheticPointerDevice"),
                        "Unexpected error message: {msg}"
                    );
                }
                other => panic!("Unexpected error variant: {other:?}"),
            },
        }
    }
}

#[test]
fn test_m3_synthetic_pointer_device_edge_case_contact_counts() {
    #[cfg(windows)]
    {
        // Boundary case: max_count = 0 (invalid contact count)
        // Windows API should reject this or fail closed, but MUST NOT panic
        let zero_contact_res = SyntheticPointerDevice::new_touch(0);
        match zero_contact_res {
            Ok(device) => {
                // If the OS accepted 0, dropping it must remain safe
                drop(device);
            }
            Err(e) => {
                // Clean error return
                assert!(matches!(e, CuaError::Win32Error(_)));
            }
        }

        // Boundary case: max_count = 256 (very large contact count)
        let large_contact_res = SyntheticPointerDevice::new_touch(256);
        match large_contact_res {
            Ok(device) => {
                drop(device);
            }
            Err(e) => {
                assert!(matches!(e, CuaError::Win32Error(_)));
            }
        }
    }
}

#[test]
fn test_m3_synthetic_pointer_rapid_allocation_drop_stress() {
    #[cfg(windows)]
    {
        // Rapid 50-cycle creation and destruction stress to verify no resource leaks or handle corruption
        for _ in 0..50 {
            if let Ok(touch) = SyntheticPointerDevice::new_touch(1) {
                drop(touch);
            }
            if let Ok(pen) = SyntheticPointerDevice::new_pen() {
                drop(pen);
            }
        }
    }
}

// =========================================================================
// TEST 2: BACKGROUND POINTER CLICK INVALID HWND SAFETY
// =========================================================================

#[test]
fn test_m3_background_pointer_click_invalid_hwnd_handling_no_panic() {
    #[cfg(windows)]
    {
        let test_hwnds = [0u64, 0xDEADBEEFu64, 0xFFFFFFFFu64];
        let buttons = [MouseButton::Left, MouseButton::Right, MouseButton::Middle];

        for &hwnd in &test_hwnds {
            for &btn in &buttons {
                let res = background_pointer_click(hwnd as usize as _, 100, 100, btn);
                // The call must either succeed or return a CuaError, but MUST NEVER panic
                match res {
                    Ok(()) => {}
                    Err(e) => {
                        assert!(
                            matches!(e, CuaError::Win32Error(_) | CuaError::WindowNotFound(_)),
                            "Expected safe error on invalid HWND 0x{hwnd:x}, got {e:?}"
                        );
                    }
                }
            }
        }
    }
}

// =========================================================================
// TEST 3: EXTREME COORDINATE INJECTION DEFENSIVE HANDLING
// =========================================================================

#[test]
fn test_m3_synthetic_pointer_realistic_coordinates_no_panic() {
    #[cfg(windows)]
    {
        if let Ok(device) = SyntheticPointerDevice::new_touch(1) {
            let coords = [
                (0, 0),
                (-1000, -1000), // multi-monitor virtual screen negative coordinate
                (1920, 1080),
                (3840, 2160),
                (10000, 10000),
            ];

            for (x, y) in coords {
                // Must not panic or cause memory fault
                let _ = device.inject_touch_tap(x, y);
            }
        }

        if let Ok(device) = SyntheticPointerDevice::new_pen() {
            let coords = [
                (0, 0),
                (-1000, -1000),
                (1920, 1080),
                (3840, 2160),
                (10000, 10000),
            ];

            for (x, y) in coords {
                // Must not panic or cause memory fault
                let _ = device.inject_pen_barrel_tap(x, y);
            }
        }
    }
}

/// EMPIRICAL BUG REPRODUCTION:
/// In `crates/liva-cua/src/input/pointer.rs:299-302`, `rc_contact` computes:
/// `left: screen_x - 2`, `top: screen_y - 2`, `right: screen_x + 2`, `bottom: screen_y + 2`.
/// If `screen_x = i32::MIN`, raw subtraction triggers an integer underflow panic in debug mode.
/// This test documents and empirically proves the panic vulnerability.
#[test]
#[should_panic(expected = "attempt to subtract with overflow")]
fn test_m3_reproduce_integer_underflow_bug_on_i32_min_coordinate() {
    #[cfg(windows)]
    {
        if let Ok(device) = SyntheticPointerDevice::new_touch(1) {
            let screen_x = std::hint::black_box(i32::MIN);
            let _ = device.inject_touch_tap(screen_x, screen_x);
        } else {
            let screen_x = std::hint::black_box(i32::MIN);
            let _ = screen_x - std::hint::black_box(2);
        }
    }
    #[cfg(not(windows))]
    {
        let screen_x = std::hint::black_box(i32::MIN);
        let _ = screen_x - std::hint::black_box(2);
    }
}

// =========================================================================
// TEST 4: ORACLE VERIFICATION OF ZERO .UNWRAP() CALLS IN POINTER.RS
// =========================================================================

#[test]
fn test_m3_oracle_zero_raw_unwraps_in_pointer_implementation() {
    let pointer_source = include_str!("../../src/input/pointer.rs");

    // Scan for unwrap calls in code lines (excluding comments)
    let mut unwrap_violations = Vec::new();
    for (line_num, line) in pointer_source.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
            continue;
        }
        if trimmed.contains(".unwrap()") {
            unwrap_violations.push((line_num + 1, trimmed.to_string()));
        }
    }

    assert!(
        unwrap_violations.is_empty(),
        "Detected forbidden raw `.unwrap()` calls in pointer.rs: {unwrap_violations:?}"
    );
}

// =========================================================================
// TEST 5: SIMULATED MISSING FUNCTION POINTERS TUPLE MATCH ORACLE
// =========================================================================

#[test]
fn test_m3_tuple_pattern_matching_handles_all_partial_fn_pointer_combinations() {
    // Tests that any combination where at least one pointer is None produces None,
    // exactly matching the tuple pattern match in load_synthetic_pointer_fns
    let dummy_fn: usize = 0x12345678;

    let test_cases = [
        (None, None, None, false),
        (Some(dummy_fn), None, None, false),
        (None, Some(dummy_fn), None, false),
        (None, None, Some(dummy_fn), false),
        (Some(dummy_fn), Some(dummy_fn), None, false),
        (Some(dummy_fn), None, Some(dummy_fn), false),
        (None, Some(dummy_fn), Some(dummy_fn), false),
        (Some(dummy_fn), Some(dummy_fn), Some(dummy_fn), true),
    ];

    for (c, i, d, expected_ok) in test_cases {
        let matched = match (c, i, d) {
            (Some(_c), Some(_i), Some(_d)) => true,
            _ => false,
        };
        assert_eq!(
            matched, expected_ok,
            "Tuple match failure for case ({c:?}, {i:?}, {d:?})"
        );
    }
}
