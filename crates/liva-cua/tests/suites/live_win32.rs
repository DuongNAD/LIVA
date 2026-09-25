//! Live Win32 system verification tests.

use liva_cua::geometry::*;
use liva_cua::input::uipi::*;
use liva_cua::window::*;

#[test]
fn test_live_dpi_and_virtual_desktop_metrics() {
    let metrics = VirtualDesktopMetrics::query_from_system();
    assert!(metrics.width > 0, "Virtual desktop width must be > 0");
    assert!(metrics.height > 0, "Virtual desktop height must be > 0");

    let sys_dpi = get_dpi_for_system();
    assert!(
        sys_dpi >= 96,
        "System DPI must be >= 96 (standard baseline)"
    );
}

#[test]
fn test_live_agent_integrity_level() {
    let level = get_agent_integrity_level();
    // In normal user execution, agent runs at Medium or High (if elevated)
    assert!(
        level == IntegrityLevel::Medium || level == IntegrityLevel::High,
        "Agent integrity level is {:?}",
        level
    );
}

#[test]
fn test_live_window_enumeration() {
    let windows = list_windows(None).expect("Live window enumeration must succeed");
    // Any active desktop session has at least 1 top-level window (e.g. Explorer, Shell, or console)
    // In headless runner it may be empty, but must not error.
    println!("Enumerated {} live top-level windows", windows.len());
    for w in windows.iter().take(5) {
        println!(" - HWND {:#x}, PID {}, title '{}'", w.hwnd, w.pid, w.title);
        assert!(w.hwnd > 0);
    }
}

#[test]
#[ignore = "live interactive desktop required"]
fn test_live_interactive_click() {
    // Reserved for interactive human testing
}
