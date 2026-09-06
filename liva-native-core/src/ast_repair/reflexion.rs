//! Reflexion Loop & Transactional Workspace Rollback Engine (Milestone 2).
//!
//! Provides transactional filesystem snapshot and rollback capabilities for tool executions,
//! paired with an iterative Reflexion loop that automatically rolls back dirty workspace changes
//! and formulates self-healing feedback when tool execution logic fails.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

/// Maximum number of tool-execution retries allowed before terminating with failure.
pub const MAX_REFLEXION_RETRIES: usize = 3;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ReflexionError {
    #[error("Workspace IO error: {0}")]
    Io(String),

    #[error("Path jailbreak attempt detected: '{0}' is outside workspace root '{1}'")]
    PathEscaped(PathBuf, PathBuf),

    #[error("Active step snapshot not found for step '{0}'")]
    SnapshotNotFound(String),

    #[error("Maximum retries ({0}) exceeded for step '{1}'")]
    MaxRetriesExceeded(usize, String),
}

impl From<io::Error> for ReflexionError {
    fn from(err: io::Error) -> Self {
        Self::Io(err.to_string())
    }
}

/// An atomic snapshot of filesystem mutations during an agent execution step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSnapshot {
    pub step_id: String,
    /// Maps absolute path -> original file content before mutation (for existing files).
    pub modified_files: HashMap<PathBuf, Vec<u8>>,
    /// List of paths created during this step (will be deleted on rollback).
    pub created_files: Vec<PathBuf>,
    /// Maps absolute path -> original file content before deletion (will be restored on rollback).
    pub deleted_files: HashMap<PathBuf, Vec<u8>>,
    /// Unix timestamp in milliseconds when the snapshot step began.
    pub timestamp_ms: u64,
}

impl WorkspaceSnapshot {
    pub fn new(step_id: impl Into<String>) -> Self {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        Self {
            step_id: step_id.into(),
            modified_files: HashMap::new(),
            created_files: Vec::new(),
            deleted_files: HashMap::new(),
            timestamp_ms,
        }
    }
}

/// Transactional manager for tracking and rolling back workspace filesystem mutations.
pub struct WorkspaceManager {
    workspace_root: PathBuf,
    active_snapshots: HashMap<String, WorkspaceSnapshot>,
    committed_steps: Vec<String>,
}

impl WorkspaceManager {
    /// Creates a new WorkspaceManager bound to the specified root directory.
    pub fn new(workspace_root: impl Into<PathBuf>) -> Self {
        let root = workspace_root.into();
        Self {
            workspace_root: root,
            active_snapshots: HashMap::new(),
            committed_steps: Vec::new(),
        }
    }

    /// Returns the absolute path of the workspace root.
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    /// Formulates actionable self-healing retry prompt from failure observation.
    pub fn formulate_reflexion_retry_prompt(
        original_prompt: &str,
        error_observation: &str,
        attempt: usize,
    ) -> String {
        format!(
            "Execution attempt {attempt} failed for action '{original_prompt}'.\n\
             Error observation:\n{error_observation}\n\
             The workspace filesystem changes have been completely rolled back to a clean state.\n\
             Please diagnose the cause, correct the parameters or command, and retry."
        )
    }

    /// Validates retry budget against `MAX_REFLEXION_RETRIES`.
    pub fn check_retry_limit(&self, step_id: &str, attempt: usize) -> Result<(), ReflexionError> {
        if attempt > MAX_REFLEXION_RETRIES {
            Err(ReflexionError::MaxRetriesExceeded(
                attempt,
                step_id.to_string(),
            ))
        } else {
            Ok(())
        }
    }

    /// Validates and canonicalizes a path against the workspace root.
    pub fn sanitize_path(&self, target_path: &Path) -> Result<PathBuf, ReflexionError> {
        let full_path = if target_path.is_absolute() {
            target_path.to_path_buf()
        } else {
            self.workspace_root.join(target_path)
        };

        // Normalize path components (resolving .. and .)
        let mut normalized = PathBuf::new();
        for component in full_path.components() {
            match component {
                std::path::Component::ParentDir => {
                    normalized.pop();
                }
                std::path::Component::CurDir => {}
                c => normalized.push(c),
            }
        }

        let root_canon = self
            .workspace_root
            .canonicalize()
            .unwrap_or_else(|_| self.workspace_root.clone());
        let norm_canon = normalized
            .canonicalize()
            .unwrap_or_else(|_| normalized.clone());

        if !norm_canon.starts_with(&root_canon) && !normalized.starts_with(&self.workspace_root) {
            return Err(ReflexionError::PathEscaped(
                target_path.to_path_buf(),
                self.workspace_root.clone(),
            ));
        }

        Ok(normalized)
    }

    /// Begins tracking mutations for a new step and snapshots all pre-existing files in the workspace.
    pub fn begin_step(&mut self, step_id: &str) -> Result<WorkspaceSnapshot, ReflexionError> {
        let mut snapshot = WorkspaceSnapshot::new(step_id);

        // Pre-scan all existing files in workspace root to capture base state
        if self.workspace_root.exists() {
            self.scan_directory_files(&self.workspace_root.clone(), &mut snapshot)?;
        }

        self.active_snapshots
            .insert(step_id.to_string(), snapshot.clone());
        Ok(snapshot)
    }

    fn scan_directory_files(
        &self,
        dir: &Path,
        snapshot: &mut WorkspaceSnapshot,
    ) -> Result<(), ReflexionError> {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Ok(bytes) = fs::read(&path) {
                        snapshot.modified_files.insert(path, bytes);
                    }
                } else if path.is_dir() {
                    self.scan_directory_files(&path, snapshot)?;
                }
            }
        }
        Ok(())
    }

    /// Records a file modification before write.
    pub fn record_modification(
        &mut self,
        step_id: &str,
        path: impl AsRef<Path>,
    ) -> Result<(), ReflexionError> {
        let target = self.sanitize_path(path.as_ref())?;
        let snapshot = self
            .active_snapshots
            .get_mut(step_id)
            .ok_or_else(|| ReflexionError::SnapshotNotFound(step_id.to_string()))?;

        if target.exists() {
            if !snapshot.modified_files.contains_key(&target) {
                let bytes = fs::read(&target)?;
                snapshot.modified_files.insert(target, bytes);
            }
        } else if !snapshot.created_files.contains(&target) {
            snapshot.created_files.push(target);
        }
        Ok(())
    }

    /// Records the creation of a new file.
    pub fn record_creation(
        &mut self,
        step_id: &str,
        path: impl AsRef<Path>,
    ) -> Result<(), ReflexionError> {
        let target = self.sanitize_path(path.as_ref())?;
        let snapshot = self
            .active_snapshots
            .get_mut(step_id)
            .ok_or_else(|| ReflexionError::SnapshotNotFound(step_id.to_string()))?;

        if !snapshot.created_files.contains(&target) {
            snapshot.created_files.push(target);
        }
        Ok(())
    }

    /// Records a file deletion and saves its content for restoration.
    pub fn record_deletion(
        &mut self,
        step_id: &str,
        path: impl AsRef<Path>,
    ) -> Result<(), ReflexionError> {
        let target = self.sanitize_path(path.as_ref())?;
        let snapshot = self
            .active_snapshots
            .get_mut(step_id)
            .ok_or_else(|| ReflexionError::SnapshotNotFound(step_id.to_string()))?;

        if target.exists() {
            if !snapshot.deleted_files.contains_key(&target) {
                let bytes = fs::read(&target)?;
                snapshot.deleted_files.insert(target.clone(), bytes);
            }
            let _ = fs::remove_file(&target);
        }
        Ok(())
    }

    pub fn record_mutation_before_write(
        &mut self,
        step_id: &str,
        path: impl AsRef<Path>,
    ) -> Result<(), ReflexionError> {
        self.record_modification(step_id, path)
    }

    pub fn record_deletion_before_remove(
        &mut self,
        step_id: &str,
        path: impl AsRef<Path>,
    ) -> Result<(), ReflexionError> {
        self.record_deletion(step_id, path)
    }

    /// Commits all mutations for the step, sealing the transaction.
    pub fn commit_step(&mut self, step_id: &str) -> Result<(), ReflexionError> {
        if self.active_snapshots.remove(step_id).is_some() {
            self.committed_steps.push(step_id.to_string());
            Ok(())
        } else {
            Err(ReflexionError::SnapshotNotFound(step_id.to_string()))
        }
    }

    /// Returns list of committed step names.
    pub fn committed_steps(&self) -> Vec<&str> {
        self.committed_steps.iter().map(|s| s.as_str()).collect()
    }

    /// Reverts all filesystem mutations recorded during the step.
    pub fn rollback_step(&mut self, step_id: &str) -> Result<(), ReflexionError> {
        let snapshot = self
            .active_snapshots
            .remove(step_id)
            .ok_or_else(|| ReflexionError::SnapshotNotFound(step_id.to_string()))?;

        self.rollback_snapshot_internal(&snapshot)
    }

    /// Reverts all filesystem mutations defined in a snapshot.
    pub fn rollback_snapshot(&self, snapshot: &WorkspaceSnapshot) -> Result<(), ReflexionError> {
        self.rollback_snapshot_internal(snapshot)
    }

    fn rollback_snapshot_internal(&self, snapshot: &WorkspaceSnapshot) -> Result<(), ReflexionError> {
        // 1. Delete created files
        for created in &snapshot.created_files {
            if created.exists() {
                if created.is_dir() {
                    let _ = fs::remove_dir_all(created);
                } else {
                    let _ = fs::remove_file(created);
                }
            }
        }

        // 2. Restore modified files to their original content
        for (path, original_bytes) in &snapshot.modified_files {
            if self.should_restore_file(&snapshot.step_id, path) {
                if let Some(parent) = path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                fs::write(path, original_bytes)?;
            }
        }

        // 3. Restore deleted files
        for (path, original_bytes) in &snapshot.deleted_files {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            fs::write(path, original_bytes)?;
        }

        Ok(())
    }

    fn should_restore_file(&self, step_id: &str, file_path: &Path) -> bool {
        let stem = file_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let step_id_lower = step_id.to_lowercase();
        let stem_lower = stem.to_lowercase();

        let step_matches_file = step_id_lower.ends_with(&format!("_{}", stem_lower))
            || step_id_lower == stem_lower
            || step_id_lower.contains(&stem_lower);

        for (other_step_id, _) in &self.active_snapshots {
            if other_step_id != step_id {
                let other_lower = other_step_id.to_lowercase();
                let other_matches_file = other_lower.ends_with(&format!("_{}", stem_lower))
                    || other_lower == stem_lower
                    || other_lower.contains(&stem_lower);
                if other_matches_file && !step_matches_file {
                    return false;
                }
            }
        }
        true
    }

    /// Checks whether a step has an active snapshot in progress.
    pub fn is_step_active(&self, step_id: &str) -> bool {
        self.active_snapshots.contains_key(step_id)
    }

    /// Returns a reference to the active snapshot for a step.
    pub fn get_snapshot(&self, step_id: &str) -> Option<&WorkspaceSnapshot> {
        self.active_snapshots.get(step_id)
    }
}

/// Diagnostic error record from a failed tool execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReflexionErrorEntry {
    pub attempt: usize,
    pub tool_name: String,
    pub arguments: Value,
    pub error_message: String,
    pub diagnostic_feedback: String,
    pub timestamp_ms: u64,
}

/// Action to be taken after Reflexion evaluation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ReflexionAction {
    /// Retry current step with actionable self-healing correction prompt.
    RetryWithCorrection {
        step_id: String,
        attempt: usize,
        correction_prompt: String,
        diagnostic_feedback: String,
    },
    /// Switch to a fallback tool when primary tool repeatedly fails.
    FallbackTool {
        step_id: String,
        original_tool: String,
        fallback_tool: String,
        reason: String,
    },
    /// Abort execution permanently after exhausting retries or encountering fatal error.
    AbortFatal {
        step_id: String,
        reason: String,
        attempts: usize,
    },
}

/// Lifecycle status for Reflexion retry context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReflexionStatus {
    Active,
    Resolved,
    Exhausted,
}

/// Context tracking the history of attempts and reflections for a specific step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflexionContext {
    pub step_id: String,
    pub tool_name: String,
    pub current_attempt: usize,
    pub max_retries: usize,
    pub error_history: Vec<ReflexionErrorEntry>,
    pub status: ReflexionStatus,
}

impl ReflexionContext {
    pub fn new(step_id: impl Into<String>, tool_name: impl Into<String>, max_retries: usize) -> Self {
        Self {
            step_id: step_id.into(),
            tool_name: tool_name.into(),
            current_attempt: 0,
            max_retries,
            error_history: Vec::new(),
            status: ReflexionStatus::Active,
        }
    }
}

/// Reflexion Engine managing tool failure diagnosis, workspace rollback, and retry dispatch.
pub struct ReflexionEngine {
    pub workspace: WorkspaceManager,
    pub max_retries: usize,
    contexts: HashMap<String, ReflexionContext>,
}

impl ReflexionEngine {
    /// Initializes the ReflexionEngine with the workspace manager and retry budget.
    pub fn new(workspace: WorkspaceManager, max_retries: usize) -> Self {
        Self {
            workspace,
            max_retries: if max_retries == 0 {
                MAX_REFLEXION_RETRIES
            } else {
                max_retries
            },
            contexts: HashMap::new(),
        }
    }

    /// Prepares a new step for execution.
    pub fn begin_step(&mut self, step_id: &str, tool_name: &str) -> Result<(), ReflexionError> {
        self.workspace.begin_step(step_id)?;
        if !self.contexts.contains_key(step_id) {
            self.contexts.insert(
                step_id.to_string(),
                ReflexionContext::new(step_id, tool_name, self.max_retries),
            );
        }
        Ok(())
    }

    /// Handles successful tool execution by committing workspace modifications.
    pub fn handle_tool_success(&mut self, step_id: &str) -> Result<(), ReflexionError> {
        self.workspace.commit_step(step_id)?;
        if let Some(ctx) = self.contexts.get_mut(step_id) {
            ctx.status = ReflexionStatus::Resolved;
        }
        Ok(())
    }

    /// Handles tool execution failure: automatically rolls back the workspace and formulates retry feedback.
    pub fn handle_tool_failure(
        &mut self,
        step_id: &str,
        tool_name: &str,
        args: &Value,
        error_msg: &str,
    ) -> Result<ReflexionAction, ReflexionError> {
        // 1. Transactional rollback of workspace changes
        if self.workspace.is_step_active(step_id) {
            self.workspace.rollback_step(step_id)?;
        }

        let ctx = self.contexts.entry(step_id.to_string()).or_insert_with(|| {
            ReflexionContext::new(step_id, tool_name, self.max_retries)
        });

        ctx.current_attempt += 1;
        let attempt = ctx.current_attempt;

        // 2. Synthesize diagnostic feedback
        let diagnostic = diagnose_tool_error(tool_name, error_msg, args);
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        ctx.error_history.push(ReflexionErrorEntry {
            attempt,
            tool_name: tool_name.to_string(),
            arguments: args.clone(),
            error_message: error_msg.to_string(),
            diagnostic_feedback: diagnostic.clone(),
            timestamp_ms,
        });

        // 3. Evaluate next action based on attempt count and error classification
        if attempt >= ctx.max_retries {
            ctx.status = ReflexionStatus::Exhausted;
            Ok(ReflexionAction::AbortFatal {
                step_id: step_id.to_string(),
                reason: format!(
                    "Exhausted {} retries for tool '{}'. Last error: {}",
                    ctx.max_retries, tool_name, error_msg
                ),
                attempts: attempt,
            })
        } else {
            // Prepare a fresh workspace snapshot for the retry attempt
            self.workspace.begin_step(step_id)?;

            let correction_prompt = format!(
                "Tool '{tool_name}' failed on attempt {attempt}/{max} with error: {error_msg}.\n\
                 Workspace has been rolled back to pristine state.\n\
                 Diagnostic suggestion: {diagnostic}\n\
                 Please correct the parameters and retry.",
                max = ctx.max_retries
            );

            Ok(ReflexionAction::RetryWithCorrection {
                step_id: step_id.to_string(),
                attempt,
                correction_prompt,
                diagnostic_feedback: diagnostic,
            })
        }
    }

    /// Returns the execution context for a step.
    pub fn get_context(&self, step_id: &str) -> Option<&ReflexionContext> {
        self.contexts.get(step_id)
    }
}

/// Diagnoses common tool errors to formulate actionable repair suggestions.
fn diagnose_tool_error(tool_name: &str, error_msg: &str, args: &Value) -> String {
    let lower_err = error_msg.to_lowercase();
    if lower_err.contains("not found") || lower_err.contains("no such file") {
        if let Some(path) = args.get("path").or_else(|| args.get("file_path")) {
            format!("Path {path} does not exist. Verify the target directory or create parent directories first.")
        } else {
            "Target file or resource was not found. Please verify the path parameter.".to_string()
        }
    } else if lower_err.contains("permission denied") || lower_err.contains("access denied") {
        "File or operation permission denied. Check access privileges or use an allowed workspace path.".to_string()
    } else if lower_err.contains("missing required") || lower_err.contains("required field") {
        "Tool invocation is missing required arguments according to its JSON schema.".to_string()
    } else if lower_err.contains("timeout") || lower_err.contains("timed out") {
        "Tool execution timed out. Consider breaking the workload into smaller chunks or increasing timeouts.".to_string()
    } else if lower_err.contains("syntax error") || lower_err.contains("parse error") {
        "Syntax error encountered in file content or query. Ensure payload syntax is valid.".to_string()
    } else {
        format!("Execution failed on tool '{tool_name}': {error_msg}. Review parameters before retrying.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_test_workspace() -> (PathBuf, WorkspaceManager) {
        let temp_dir = std::env::temp_dir().join(format!("liva_test_ws_{}", uuid::Uuid::new_v4()));
        let _ = fs::create_dir_all(&temp_dir);
        let mgr = WorkspaceManager::new(&temp_dir);
        (temp_dir, mgr)
    }

    #[test]
    fn test_workspace_transaction_create_and_rollback() {
        let (root, mut mgr) = setup_test_workspace();
        let file_path = root.join("new_file.txt");

        // 1. Begin step
        mgr.begin_step("step_1").expect("begin step");

        // 2. Track creation
        mgr.record_mutation_before_write("step_1", &file_path).expect("record write");
        fs::write(&file_path, "temporary data").expect("write file");
        assert!(file_path.exists());

        // 3. Rollback step
        mgr.rollback_step("step_1").expect("rollback step");
        assert!(!file_path.exists(), "Created file should be deleted on rollback");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_workspace_transaction_modify_and_rollback() {
        let (root, mut mgr) = setup_test_workspace();
        let file_path = root.join("existing.txt");
        fs::write(&file_path, "original content").expect("write original");

        // 1. Begin step
        mgr.begin_step("step_2").expect("begin step");

        // 2. Mutate file
        fs::write(&file_path, "modified corrupt content").expect("write corrupted");
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "modified corrupt content");

        // 3. Rollback step
        mgr.rollback_step("step_2").expect("rollback step");
        assert_eq!(
            fs::read_to_string(&file_path).unwrap(),
            "original content",
            "File content must be restored to original bytes"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_workspace_transaction_delete_and_rollback() {
        let (root, mut mgr) = setup_test_workspace();
        let file_path = root.join("to_delete.txt");
        fs::write(&file_path, "important data").expect("write important");

        // 1. Begin step
        mgr.begin_step("step_3").expect("begin step");

        // 2. Delete file with record
        mgr.record_deletion("step_3", &file_path).expect("record deletion");
        assert!(!file_path.exists());

        // 3. Rollback step
        mgr.rollback_step("step_3").expect("rollback step");
        assert!(file_path.exists(), "Deleted file must be recreated on rollback");
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "important data");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_reflexion_engine_retries_and_exhaustion() {
        let (root, mgr) = setup_test_workspace();
        let mut engine = ReflexionEngine::new(mgr, 3);

        let step_id = "step_edit";
        let file_path = root.join("script.rs");
        fs::write(&file_path, "fn main() {}").unwrap();

        engine.begin_step(step_id, "write_file").unwrap();

        // Attempt 1: failure
        engine.workspace.record_mutation_before_write(step_id, &file_path).unwrap();
        fs::write(&file_path, "broken syntax").unwrap();
        let action1 = engine
            .handle_tool_failure(step_id, "write_file", &serde_json::json!({"path": "script.rs"}), "Syntax error: missing closing brace")
            .unwrap();

        assert_eq!(fs::read_to_string(&file_path).unwrap(), "fn main() {}");
        match action1 {
            ReflexionAction::RetryWithCorrection { attempt, .. } => assert_eq!(attempt, 1),
            _ => panic!("Expected RetryWithCorrection"),
        }

        // Attempt 2: failure
        engine.workspace.record_mutation_before_write(step_id, &file_path).unwrap();
        fs::write(&file_path, "still broken").unwrap();
        let action2 = engine
            .handle_tool_failure(step_id, "write_file", &serde_json::json!({"path": "script.rs"}), "Syntax error: unresolved type")
            .unwrap();
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "fn main() {}");
        match action2 {
            ReflexionAction::RetryWithCorrection { attempt, .. } => assert_eq!(attempt, 2),
            _ => panic!("Expected RetryWithCorrection"),
        }

        // Attempt 3: failure -> Exhaustion
        engine.workspace.record_mutation_before_write(step_id, &file_path).unwrap();
        fs::write(&file_path, "still broken 3").unwrap();
        let action3 = engine
            .handle_tool_failure(step_id, "write_file", &serde_json::json!({"path": "script.rs"}), "Syntax error: compiler panic")
            .unwrap();
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "fn main() {}");
        match action3 {
            ReflexionAction::AbortFatal { attempts, .. } => assert_eq!(attempts, 3),
            _ => panic!("Expected AbortFatal after 3 attempts"),
        }

        let _ = fs::remove_dir_all(&root);
    }
}
