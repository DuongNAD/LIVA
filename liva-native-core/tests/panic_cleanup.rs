use std::fs;
use std::path::PathBuf;

struct TempDirGuard {
    path: PathBuf,
}
impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn test_panic_cleanup_check() {
    let rand_val = rand::random::<u32>();
    let temp_dir = std::env::temp_dir().join(format!("test_panic_guard_{}", rand_val));
    fs::create_dir_all(&temp_dir).unwrap();

    // Create a marker file inside
    let marker_file = temp_dir.join("marker.txt");
    fs::write(&marker_file, "temp").unwrap();

    // Check it exists
    assert!(temp_dir.exists());
    assert!(marker_file.exists());

    // Run the panic code inside catch_unwind to simulate panic unwinding
    let temp_dir_clone = temp_dir.clone();
    let result = std::panic::catch_unwind(move || {
        let _guard = TempDirGuard {
            path: temp_dir_clone,
        };
        // Force assertion panic
        assert_eq!(
            1, 2,
            "Intentional panic for testing TempDirGuard drop cleanup"
        );
    });

    assert!(result.is_err(), "Expected panic did not occur!");
    // The guard should have dropped, and the directory should be gone.
    assert!(
        !temp_dir.exists(),
        "Temporary directory was not cleaned up on panic unwind!"
    );
}
