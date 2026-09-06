//! Skills Live Hot-Reload Watcher & SHA-256 Fingerprint Diffing Engine (Milestone 3 / Feature 11).
//!
//! Event-driven filesystem watcher using `notify` with 150ms debouncing, SHA-256 fingerprint diffing,
//! and atomic hot-swapping into the active skill store for live zero-downtime hot-reloading.

use super::manifest::{LoadedSkillPackage, SkillError, parse_skill_markdown};
use super::store::SkillPackageStore;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::{RwLock, broadcast, mpsc};

/// Default debounce window (150ms) for filesystem modify bursts.
pub const DEFAULT_DEBOUNCE_DURATION: Duration = Duration::from_millis(150);

/// Live hot-reload event emitted when skills are added, modified, or removed.
#[derive(Debug, Clone, PartialEq)]
pub enum SkillChangeEvent {
    Added(LoadedSkillPackage),
    Modified {
        old_hash: String,
        new_package: LoadedSkillPackage,
    },
    Removed {
        skill_name: String,
        file_path: PathBuf,
    },
}

/// Asynchronous filesystem watcher for live hot-reloading skill packages.
pub struct SkillWatcher {
    watch_paths: Vec<PathBuf>,
    known_hashes: Arc<RwLock<HashMap<PathBuf, (String, String)>>>, // Path -> (content_hash, skill_name)
    event_tx: broadcast::Sender<SkillChangeEvent>,
    package_store: Option<Arc<RwLock<SkillPackageStore>>>,
    debounce_duration: Duration,
    is_running: Arc<AtomicBool>,
}

impl SkillWatcher {
    /// Create a new SkillWatcher with specified watch paths and debounce duration.
    pub fn new(watch_paths: Vec<PathBuf>, debounce_duration: Duration) -> Self {
        let (event_tx, _) = broadcast::channel(256);
        Self {
            watch_paths,
            known_hashes: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            package_store: None,
            debounce_duration,
            is_running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Create with default 150ms debounce duration.
    pub fn with_default_debounce(watch_paths: Vec<PathBuf>) -> Self {
        Self::new(watch_paths, DEFAULT_DEBOUNCE_DURATION)
    }

    /// Attach a SkillPackageStore for automatic in-memory synchronization upon changes.
    pub fn with_package_store(mut self, store: Arc<RwLock<SkillPackageStore>>) -> Self {
        self.package_store = Some(store);
        self
    }

    /// Update debounce duration.
    pub fn with_debounce(mut self, debounce: Duration) -> Self {
        self.debounce_duration = debounce;
        self
    }

    /// Subscribe to live hot-reload change events.
    pub fn subscribe(&self) -> broadcast::Receiver<SkillChangeEvent> {
        self.event_tx.subscribe()
    }

    /// Get current debounce duration.
    pub fn debounce_duration(&self) -> Duration {
        self.debounce_duration
    }

    /// Perform a single scan pass across all watched paths and compute SHA-256 diffs.
    pub async fn scan_once(&self) -> Result<Vec<SkillChangeEvent>, SkillError> {
        let mut events = Vec::new();
        let mut current_discovered = HashMap::new();

        for root in &self.watch_paths {
            if root.is_file() {
                if let Some(file_name) = root.file_name() {
                    if file_name == super::SKILL_FILE {
                        self.process_skill_file(root, &mut current_discovered, &mut events).await;
                    }
                }
            } else if root.is_dir() {
                self.scan_directory_recursive(root, 0, &mut current_discovered, &mut events).await;
            }
        }

        // Detect removals
        let mut known = self.known_hashes.write().await;
        let mut removed_paths = Vec::new();

        for (path, (_hash, skill_name)) in known.iter() {
            if !current_discovered.contains_key(path) && !path.exists() {
                removed_paths.push((path.clone(), skill_name.clone()));
            }
        }

        for (path, skill_name) in removed_paths {
            known.remove(&path);
            if let Some(ref store_arc) = self.package_store {
                let mut store = store_arc.write().await;
                store.remove(&skill_name);
            }
            let ev = SkillChangeEvent::Removed {
                skill_name,
                file_path: path,
            };
            let _ = self.event_tx.send(ev.clone());
            events.push(ev);
        }

        Ok(events)
    }

    async fn scan_directory_recursive(
        &self,
        dir: &Path,
        depth: usize,
        current_discovered: &mut HashMap<PathBuf, String>,
        events: &mut Vec<SkillChangeEvent>,
    ) {
        if depth > 6 {
            return;
        }

        let skill_file = dir.join(super::SKILL_FILE);
        if skill_file.is_file() {
            self.process_skill_file(&skill_file, current_discovered, events).await;
            return; // Do not recurse inside an identified skill directory
        }

        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    Box::pin(self.scan_directory_recursive(&path, depth + 1, current_discovered, events)).await;
                }
            }
        }
    }

    async fn process_skill_file(
        &self,
        skill_file: &Path,
        current_discovered: &mut HashMap<PathBuf, String>,
        events: &mut Vec<SkillChangeEvent>,
    ) {
        let content = match std::fs::read_to_string(skill_file) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Failed to read skill file {}: {e}", skill_file.display());
                return;
            }
        };

        let parent_dir = skill_file.parent().unwrap_or_else(|| Path::new("."));
        let pkg = match parse_skill_markdown(&content, parent_dir) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("Failed to parse skill in {}: {e}", skill_file.display());
                return;
            }
        };

        current_discovered.insert(skill_file.to_path_buf(), pkg.content_hash.clone());

        let mut known = self.known_hashes.write().await;
        match known.get(skill_file) {
            None => {
                // New skill discovered
                known.insert(skill_file.to_path_buf(), (pkg.content_hash.clone(), pkg.manifest.name.clone()));
                if let Some(ref store_arc) = self.package_store {
                    let mut store = store_arc.write().await;
                    store.upsert(pkg.clone());
                }
                let ev = SkillChangeEvent::Added(pkg);
                let _ = self.event_tx.send(ev.clone());
                events.push(ev);
            }
            Some((old_hash, _)) => {
                if old_hash != &pkg.content_hash {
                    // Skill modified
                    let old_h = old_hash.clone();
                    known.insert(skill_file.to_path_buf(), (pkg.content_hash.clone(), pkg.manifest.name.clone()));
                    if let Some(ref store_arc) = self.package_store {
                        let mut store = store_arc.write().await;
                        store.upsert(pkg.clone());
                    }
                    let ev = SkillChangeEvent::Modified {
                        old_hash: old_h,
                        new_package: pkg,
                    };
                    let _ = self.event_tx.send(ev.clone());
                    events.push(ev);
                }
            }
        }
    }

    /// Start a live event-driven filesystem watcher using `notify` with debouncing (150ms).
    pub fn start_live_watcher(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        self.is_running.store(true, Ordering::SeqCst);
        let watcher_self = Arc::clone(&self);
        let debounce_dur = self.debounce_duration;

        tokio::spawn(async move {
            let (raw_tx, mut raw_rx) = mpsc::channel::<()>(100);

            // Set up `notify` watcher
            let raw_tx_clone = raw_tx.clone();
            let mut watcher: Option<RecommendedWatcher> = match RecommendedWatcher::new(
                move |res: Result<Event, notify::Error>| {
                    if let Ok(event) = res {
                        if !event.paths.is_empty() {
                            let _ = raw_tx_clone.try_send(());
                        }
                    }
                },
                notify::Config::default(),
            ) {
                Ok(mut w) => {
                    for path in &watcher_self.watch_paths {
                        if path.exists() {
                            let _ = w.watch(path, RecursiveMode::Recursive);
                        }
                    }
                    Some(w)
                }
                Err(e) => {
                    tracing::warn!("Failed to initialize notify watcher: {e}. Falling back to debounced loop.");
                    None
                }
            };

            // Initial scan pass
            let _ = watcher_self.scan_once().await;

            // Debounced event loop
            while watcher_self.is_running.load(Ordering::Relaxed) {
                tokio::select! {
                    Some(()) = raw_rx.recv() => {
                        // Debounce burst: drain any rapid pending signals within debounce_dur
                        tokio::time::sleep(debounce_dur).await;
                        while raw_rx.try_recv().is_ok() {}

                        if watcher_self.is_running.load(Ordering::Relaxed) {
                            let _ = watcher_self.scan_once().await;
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_secs(5)) => {
                        // Periodic heartbeat scan to catch unnotified changes
                        if watcher_self.is_running.load(Ordering::Relaxed) {
                            let _ = watcher_self.scan_once().await;
                        }
                    }
                }
            }

            // Drop watcher when stopped
            drop(watcher.take());
        })
    }

    /// Alias for backwards compatibility with background async monitoring loops.
    pub fn start_background_watcher(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        self.start_live_watcher()
    }

    /// Stop the running filesystem watcher.
    pub fn stop(&self) {
        self.is_running.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_watcher_sha256_diffing_and_events() {
        let temp_dir = std::env::temp_dir().join(format!("liva_test_watcher_m3_{}", rand::random::<u32>()));
        tokio::fs::create_dir_all(&temp_dir).await.unwrap();

        let store = Arc::new(RwLock::new(SkillPackageStore::new()));
        let watcher = SkillWatcher::with_default_debounce(vec![temp_dir.clone()])
            .with_package_store(Arc::clone(&store));
        let mut rx = watcher.subscribe();

        // 1. Create skill v1
        let skill_sub = temp_dir.join("code-helper");
        tokio::fs::create_dir_all(&skill_sub).await.unwrap();
        let skill_file = skill_sub.join("SKILL.md");

        let skill_v1 = r#"---
name: "code-helper"
version: "1.0.0"
description: "Assists with code review"
runtime_type: "native_rust"
---
# Code Helper v1
"#;
        tokio::fs::write(&skill_file, skill_v1).await.unwrap();

        // Scan pass 1: Added
        let events = watcher.scan_once().await.unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], SkillChangeEvent::Added(_)));
        assert_eq!(store.read().await.count(), 1);

        let broadcast_ev = rx.recv().await.unwrap();
        assert!(matches!(broadcast_ev, SkillChangeEvent::Added(_)));

        // Scan pass 1.1 (no change -> 0 events)
        let events_repeat = watcher.scan_once().await.unwrap();
        assert_eq!(events_repeat.len(), 0);

        // 2. Modify skill to v2
        let skill_v2 = r#"---
name: "code-helper"
version: "2.0.0"
description: "Assists with code review and linting"
runtime_type: "native_rust"
---
# Code Helper v2
"#;
        tokio::fs::write(&skill_file, skill_v2).await.unwrap();

        // Scan pass 2: Modified
        let events2 = watcher.scan_once().await.unwrap();
        assert_eq!(events2.len(), 1);
        match &events2[0] {
            SkillChangeEvent::Modified { old_hash, new_package } => {
                assert_eq!(new_package.manifest.version, "2.0.0");
                assert_ne!(old_hash, &new_package.content_hash);
            }
            _ => panic!("Expected modified event"),
        }

        // 3. Remove skill
        tokio::fs::remove_file(&skill_file).await.unwrap();
        tokio::fs::remove_dir_all(&skill_sub).await.unwrap();

        // Scan pass 3: Removed
        let events3 = watcher.scan_once().await.unwrap();
        assert_eq!(events3.len(), 1);
        match &events3[0] {
            SkillChangeEvent::Removed { skill_name, .. } => {
                assert_eq!(skill_name, "code-helper");
            }
            _ => panic!("Expected removed event"),
        }
        assert_eq!(store.read().await.count(), 0);

        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    }
}
