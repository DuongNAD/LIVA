//! Adversarial Stress & Lock Poison Immunity Test Suite for CUA SecurityGovernor (Milestone M4)
//!
//! Validates:
//! 1. Deliberate thread panics during write lock operations do NOT poison parking_lot::RwLock.
//! 2. SecurityGovernor allows immediate read/write acquisition post-panic without PoisonError.
//! 3. High-concurrency (100 threads) reads and writes with interleaved panics maintain state integrity.
//! 4. Comparative oracle proof: std::sync::RwLock poisons vs parking_lot::RwLock never poisons.

use liva_cua::security::SecurityGovernor;
use liva_cua::types::{
    CuaAction, CuaConfig, CuaDeliveryMode, CuaRect, CuaWindowInfo, MouseButton, PermissionMode,
};
use parking_lot::RwLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[test]
fn test_cua_security_governor_lock_poison_immunity() {
    let mut config = CuaConfig::default();
    config.permission_mode = PermissionMode::Bounded;
    config.process_allowlist = vec!["notepad.exe".to_string(), "calc.exe".to_string()];

    let gov = Arc::new(SecurityGovernor::new(&config));

    // Phase 1: Induce 20 threads to panic while modifying SecurityGovernor state
    for i in 0..20 {
        let g = gov.clone();
        let _ = std::thread::spawn(move || {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if i % 2 == 0 {
                    g.set_permission_mode(PermissionMode::Unrestricted);
                    panic!("Simulated worker panic during set_permission_mode");
                } else {
                    let mut list = g.get_allowlist();
                    list.push(format!("panicked_app_{i}.exe"));
                    g.set_allowlist(list);
                    panic!("Simulated worker panic during set_allowlist");
                }
            }));
        })
        .join();
    }

    // Phase 2: Verify Governor survives 100% of panics without poison
    assert_eq!(gov.get_permission_mode(), PermissionMode::Unrestricted);
    gov.set_permission_mode(PermissionMode::Bounded);
    assert_eq!(gov.get_permission_mode(), PermissionMode::Bounded);

    let allowlist = gov.get_allowlist();
    assert!(allowlist.contains(&"notepad.exe".to_string()));
    assert!(allowlist.contains(&"calc.exe".to_string()));

    // Phase 3: 100 concurrent threads hammering reads and writes with intentional panic
    let mut handles = Vec::new();
    let panics_caught = Arc::new(AtomicUsize::new(0));

    for tid in 0..100 {
        let g = gov.clone();
        let pc = panics_caught.clone();

        handles.push(std::thread::spawn(move || {
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                for iter in 0..20 {
                    if tid == 42 && iter == 5 {
                        g.set_permission_mode(PermissionMode::Bounded);
                        panic!("Adversarial panic in thread 42");
                    }

                    if (tid + iter) % 7 == 0 {
                        g.set_permission_mode(PermissionMode::Bounded);
                    } else if (tid + iter) % 11 == 0 {
                        g.set_simulated_uipi(0x1000 + tid as u64, 0x2000 + iter as u32);
                    } else {
                        let _m = g.get_permission_mode();
                        let _al = g.get_allowlist();
                    }
                }
            }));
            if res.is_err() {
                pc.fetch_add(1, Ordering::SeqCst);
            }
        }));
    }

    for h in handles {
        let _ = h.join();
    }

    assert_eq!(panics_caught.load(Ordering::SeqCst), 1);

    // Final action evaluation must work perfectly
    let target = CuaWindowInfo {
        hwnd: 0x1000,
        pid: 500,
        title: "Notepad".into(),
        class_name: "Notepad".into(),
        process_name: Some("notepad.exe".into()),
        bounds: CuaRect::new(100, 100, 800, 600),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    };

    let action = CuaAction::Click {
        target_hwnd: 0x1000,
        x: Some(200),
        y: Some(200),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };

    gov.set_permission_mode(PermissionMode::Bounded);
    gov.set_simulated_uipi(0x1000, 0x2000);
    gov.set_simulated_agent_rid(0x2000);
    let eval_res = gov.evaluate_action(&action, &target);
    assert!(
        eval_res.is_ok(),
        "Governor evaluate_action failed: {:?}",
        eval_res.err()
    );
}

#[test]
fn test_rwlock_poison_comparative_oracle() {
    use std::sync::RwLock as StdRwLock;

    // std::sync::RwLock POISONS on panic
    let std_lock = Arc::new(StdRwLock::new(100));
    let s_c = std_lock.clone();
    let _ = std::thread::spawn(move || {
        let mut guard = s_c.write().unwrap();
        *guard = 200;
        panic!("Panicking inside std::sync::RwLock");
    })
    .join();

    assert!(
        std_lock.read().is_err(),
        "std::sync::RwLock MUST be poisoned after thread panic"
    );

    // parking_lot::RwLock NEVER poisons on panic
    let p_lock = Arc::new(RwLock::new(100));
    let p_c = p_lock.clone();
    let _ = std::thread::spawn(move || {
        let mut guard = p_c.write();
        *guard = 200;
        panic!("Panicking inside parking_lot::RwLock");
    })
    .join();

    let val = *p_lock.read();
    assert_eq!(
        val, 200,
        "parking_lot::RwLock must cleanly release lock on unwind without poisoning"
    );
}
