//! Shared E2E Test Harness for LIVA System Overhaul
//! Provides common mock factories, crypto fixtures, session managers, and assertion helpers.

use chrono::Utc;
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub struct E2ETestContext {
    pub session_id: String,
    pub created_at: chrono::DateTime<Utc>,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Default for E2ETestContext {
    fn default() -> Self {
        Self::new()
    }
}

impl E2ETestContext {
    pub fn new() -> Self {
        Self {
            session_id: Uuid::new_v4().to_string(),
            created_at: Utc::now(),
            metadata: HashMap::from([("environment".to_string(), json!("e2e_test_opaque_box"))]),
        }
    }

    pub fn make_temp_dir(prefix: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("{}_{}", prefix, rand::random::<u32>()));
        std::fs::create_dir_all(&path).expect("failed to create temp dir");
        path
    }
}

pub struct E2ETempDirGuard {
    pub path: PathBuf,
}

impl Drop for E2ETempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
