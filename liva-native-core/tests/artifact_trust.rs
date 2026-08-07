use liva_native_core::{
    embedded_file_hash, embedded_model_hash, embedded_runtime_artifact_hash, verify_trusted_file,
};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

fn temp_dir() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "liva_artifact_trust_{}_{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn sha256(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn chi_nhan_file_canonical_nam_trong_trust_root_va_dung_hash() {
    let root = temp_dir();
    fs::create_dir_all(root.join("models")).unwrap();
    fs::write(root.join("models/router.gguf"), b"trusted-model").unwrap();

    let verified = verify_trusted_file(
        &root,
        PathBuf::from("models/router.gguf").as_path(),
        &sha256(b"trusted-model"),
    )
    .unwrap();
    assert_eq!(
        verified,
        root.join("models/router.gguf").canonicalize().unwrap()
    );

    assert!(
        verify_trusted_file(
            &root,
            PathBuf::from("../outside.gguf").as_path(),
            &sha256(b"trusted-model")
        )
        .is_err()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn tu_choi_duong_dan_tuyet_doi_ngoai_root_va_hash_sai() {
    let root = temp_dir();
    let outside = temp_dir();
    fs::write(root.join("router.gguf"), b"trusted-model").unwrap();
    fs::write(outside.join("evil.gguf"), b"trusted-model").unwrap();

    assert!(
        verify_trusted_file(&root, &outside.join("evil.gguf"), &sha256(b"trusted-model")).is_err()
    );
    assert!(verify_trusted_file(&root, &root.join("router.gguf"), &sha256(b"tampered")).is_err());

    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(outside).unwrap();
}

#[test]
fn embedded_manifest_la_trust_anchor_cho_model_va_vec0() {
    assert_eq!(
        embedded_model_hash("Qwen3-VL-2B-Instruct-GGUF/Qwen3-VL-2B-Instruct-Q4_K_M.gguf").unwrap(),
        "858fcf2a39dc73b26dd86592cb0a5f949b59d1edb365d1dea98e46b02e955e56"
    );
    assert_eq!(
        embedded_runtime_artifact_hash("vec0").unwrap(),
        "fcf98662a7ad9dce394b96a88f91032047823831b951c76636787c312a6476e6"
    );
    assert!(embedded_model_hash("unknown.gguf").is_err());
    assert!(embedded_runtime_artifact_hash("unknown").is_err());
}

#[test]
fn embedded_manifest_pin_dung_wake_v2_artifact() {
    assert_eq!(
        embedded_file_hash("models/wake_liva_en_v2.onnx").unwrap(),
        "459d3a803f199c6b228fe711b4a1eeb9dc0da2cd51ea36c5ff7e6a2c45cf0202"
    );
}

#[test]
fn canonicalization_chan_symlink_hoac_junction_escape() {
    let root = temp_dir();
    let outside = temp_dir();
    fs::write(outside.join("evil.gguf"), b"trusted-model").unwrap();
    let link = root.join("linked");

    #[cfg(windows)]
    {
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&outside)
            .status()
            .expect("không chạy được mklink");
        assert!(status.success(), "không tạo được junction test");
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, &link).unwrap();

    assert!(
        verify_trusted_file(
            &root,
            PathBuf::from("linked/evil.gguf").as_path(),
            &sha256(b"trusted-model")
        )
        .is_err(),
        "canonical target thoát root phải bị từ chối"
    );

    fs::remove_dir(&link).unwrap();
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(outside).unwrap();
}
