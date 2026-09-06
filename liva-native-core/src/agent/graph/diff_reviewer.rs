use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use super::hitl::{ApprovalContext, ApprovalDecision};

/// Line type within a unified diff hunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffLineType {
    /// Unchanged context line (' ')
    Context,
    /// Added line ('+')
    Addition,
    /// Removed line ('-')
    Deletion,
}

/// An individual annotated line within a diff hunk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffLine {
    pub line_type: DiffLineType,
    pub content: String,
    pub old_line_no: Option<usize>,
    pub new_line_no: Option<usize>,
}

/// Status of an individual code change diff hunk under human review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum HunkStatus {
    Pending,
    Approved,
    Rejected {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    Modified {
        user_override: String,
    },
}

impl Default for HunkStatus {
    fn default() -> Self {
        Self::Pending
    }
}

/// Granular line-by-line diff hunk structure supporting Living Canvas interactive review.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiffHunk {
    pub hunk_id: String,
    pub file_path: String,
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    pub header: String,
    pub lines: Vec<DiffLine>,
    pub diff_content: String,
    pub status: HunkStatus,
}

/// A parsed file diff containing metadata and one or more hunks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileDiff {
    pub old_path: Option<String>,
    pub new_path: Option<String>,
    pub is_new: bool,
    pub is_deleted: bool,
    pub is_renamed: bool,
    pub hunks: Vec<DiffHunk>,
}

/// Overall review status for a diff review session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffReviewStatus {
    Pending,
    FullyApproved,
    PartiallyApproved,
    Rejected,
    Applied,
}

/// Active interactive diff review session associated with a Pregel HITL suspension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffReviewSession {
    pub session_id: String,
    pub thread_id: String,
    pub action_id: String,
    pub files: Vec<FileDiff>,
    pub status: DiffReviewStatus,
    pub created_at: i64,
    pub updated_at: i64,
}

impl DiffReviewSession {
    pub fn new(
        session_id: impl Into<String>,
        thread_id: impl Into<String>,
        action_id: impl Into<String>,
        files: Vec<FileDiff>,
    ) -> Self {
        let now = Utc::now().timestamp_millis();
        Self {
            session_id: session_id.into(),
            thread_id: thread_id.into(),
            action_id: action_id.into(),
            files,
            status: DiffReviewStatus::Pending,
            created_at: now,
            updated_at: now,
        }
    }

    /// Count total hunks across all files in session.
    pub fn total_hunks(&self) -> usize {
        self.files.iter().map(|f| f.hunks.len()).sum()
    }

    /// Count pending hunks across all files in session.
    pub fn pending_hunks_count(&self) -> usize {
        self.files
            .iter()
            .flat_map(|f| &f.hunks)
            .filter(|h| matches!(h.status, HunkStatus::Pending))
            .count()
    }

    /// Count approved hunks across all files in session.
    pub fn approved_hunks_count(&self) -> usize {
        self.files
            .iter()
            .flat_map(|f| &f.hunks)
            .filter(|h| matches!(h.status, HunkStatus::Approved))
            .count()
    }

    /// Count rejected hunks across all files in session.
    pub fn rejected_hunks_count(&self) -> usize {
        self.files
            .iter()
            .flat_map(|f| &f.hunks)
            .filter(|h| matches!(h.status, HunkStatus::Rejected { .. }))
            .count()
    }

    /// Count user-modified hunks across all files in session.
    pub fn modified_hunks_count(&self) -> usize {
        self.files
            .iter()
            .flat_map(|f| &f.hunks)
            .filter(|h| matches!(h.status, HunkStatus::Modified { .. }))
            .count()
    }

    /// Update a specific hunk status within this session.
    pub fn update_hunk_status(&mut self, hunk_id: &str, status: HunkStatus) -> Result<(), String> {
        for file in &mut self.files {
            for hunk in &mut file.hunks {
                if hunk.hunk_id == hunk_id {
                    hunk.status = status;
                    self.updated_at = Utc::now().timestamp_millis();
                    self.refresh_status();
                    return Ok(());
                }
            }
        }

        Err(format!("Hunk '{}' not found in session '{}'", hunk_id, self.session_id))
    }

    /// Recompute overall session status based on hunk states.
    pub fn refresh_status(&mut self) {
        let total = self.total_hunks();
        if total == 0 {
            self.status = DiffReviewStatus::FullyApproved;
            return;
        }

        let pending = self.pending_hunks_count();
        if pending > 0 {
            self.status = DiffReviewStatus::Pending;
            return;
        }

        let approved = self.approved_hunks_count() + self.modified_hunks_count();
        let rejected = self.rejected_hunks_count();

        if approved == total {
            self.status = DiffReviewStatus::FullyApproved;
        } else if rejected == total {
            self.status = DiffReviewStatus::Rejected;
        } else {
            self.status = DiffReviewStatus::PartiallyApproved;
        }
    }

    /// Check if all hunks have reached a decision.
    pub fn is_fully_decided(&self) -> bool {
        self.pending_hunks_count() == 0
    }
}

// ── Unified Diff Parser ───────────────────────────────────────────────────────

/// Parse a raw unified diff string (git diff or diff -u) into structured `Vec<FileDiff>`.
pub fn parse_unified_diff(raw_diff: &str) -> Result<Vec<FileDiff>, String> {
    let mut files: Vec<FileDiff> = Vec::new();
    let mut current_old_path: Option<String> = None;
    let mut current_new_path: Option<String> = None;
    let mut current_hunks: Vec<DiffHunk> = Vec::new();
    let mut current_hunk: Option<DiffHunk> = None;
    let mut hunk_counter = 0usize;
    let mut old_line_cursor = 0usize;
    let mut new_line_cursor = 0usize;

    let lines: Vec<&str> = raw_diff.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // 1. Detect diff header: diff --git a/path b/path
        if line.starts_with("diff --git ") {
            // Flush existing hunk and file
            if let Some(h) = current_hunk.take() {
                current_hunks.push(h);
            }
            if current_old_path.is_some() || current_new_path.is_some() || !current_hunks.is_empty() {
                let is_new = current_old_path.as_deref() == Some("/dev/null");
                let is_deleted = current_new_path.as_deref() == Some("/dev/null");
                let is_renamed = !is_new && !is_deleted && current_old_path != current_new_path;
                files.push(FileDiff {
                    old_path: current_old_path.take(),
                    new_path: current_new_path.take(),
                    is_new,
                    is_deleted,
                    is_renamed,
                    hunks: std::mem::take(&mut current_hunks),
                });
            }
            hunk_counter = 0;
            i += 1;
            continue;
        }

        // 2. Detect --- a/path or --- /dev/null
        if line.starts_with("--- ") {
            let path_part = line[4..].trim();
            current_old_path = if path_part == "/dev/null" {
                Some("/dev/null".to_string())
            } else {
                Some(path_part.strip_prefix("a/").unwrap_or(path_part).to_string())
            };
            i += 1;
            continue;
        }

        // 3. Detect +++ b/path or +++ /dev/null
        if line.starts_with("+++ ") {
            let path_part = line[4..].trim();
            current_new_path = if path_part == "/dev/null" {
                Some("/dev/null".to_string())
            } else {
                Some(path_part.strip_prefix("b/").unwrap_or(path_part).to_string())
            };
            i += 1;
            continue;
        }

        // 4. Detect Hunk Header: @@ -old_start,old_lines +new_start,new_lines @@ [header]
        if line.starts_with("@@ ") {
            if let Some(h) = current_hunk.take() {
                current_hunks.push(h);
            }

            hunk_counter += 1;
            let (old_start, old_lines, new_start, new_lines, header) = parse_hunk_header(line)?;
            old_line_cursor = old_start;
            new_line_cursor = new_start;

            let file_path = current_new_path
                .as_deref()
                .or(current_old_path.as_deref())
                .unwrap_or("unknown_file")
                .to_string();

            current_hunk = Some(DiffHunk {
                hunk_id: format!("hunk-{}-{}", file_path.replace(['/', '\\', '.'], "_"), hunk_counter),
                file_path,
                old_start,
                old_lines,
                new_start,
                new_lines,
                header,
                lines: Vec::new(),
                diff_content: String::new(),
                status: HunkStatus::Pending,
            });

            i += 1;
            continue;
        }

        // 5. Hunk Body Lines
        if let Some(ref mut hunk) = current_hunk {
            if line.starts_with('+') {
                let content = line[1..].to_string();
                hunk.lines.push(DiffLine {
                    line_type: DiffLineType::Addition,
                    content,
                    old_line_no: None,
                    new_line_no: Some(new_line_cursor),
                });
                if !hunk.diff_content.is_empty() {
                    hunk.diff_content.push('\n');
                }
                hunk.diff_content.push_str(line);
                new_line_cursor += 1;
            } else if line.starts_with('-') {
                let content = line[1..].to_string();
                hunk.lines.push(DiffLine {
                    line_type: DiffLineType::Deletion,
                    content,
                    old_line_no: Some(old_line_cursor),
                    new_line_no: None,
                });
                if !hunk.diff_content.is_empty() {
                    hunk.diff_content.push('\n');
                }
                hunk.diff_content.push_str(line);
                old_line_cursor += 1;
            } else if line.starts_with(' ') || line.is_empty() {
                let content = if line.starts_with(' ') {
                    line[1..].to_string()
                } else {
                    String::new()
                };
                hunk.lines.push(DiffLine {
                    line_type: DiffLineType::Context,
                    content,
                    old_line_no: Some(old_line_cursor),
                    new_line_no: Some(new_line_cursor),
                });
                if !hunk.diff_content.is_empty() {
                    hunk.diff_content.push('\n');
                }
                hunk.diff_content.push_str(line);
                old_line_cursor += 1;
                new_line_cursor += 1;
            } else if line.starts_with('\\') {
                // "\ No newline at end of file"
                if !hunk.diff_content.is_empty() {
                    hunk.diff_content.push('\n');
                }
                hunk.diff_content.push_str(line);
            }
        }

        i += 1;
    }

    // Flush last hunk and file
    if let Some(h) = current_hunk.take() {
        current_hunks.push(h);
    }
    if current_old_path.is_some() || current_new_path.is_some() || !current_hunks.is_empty() {
        let is_new = current_old_path.as_deref() == Some("/dev/null");
        let is_deleted = current_new_path.as_deref() == Some("/dev/null");
        let is_renamed = !is_new && !is_deleted && current_old_path != current_new_path;
        files.push(FileDiff {
            old_path: current_old_path.take(),
            new_path: current_new_path.take(),
            is_new,
            is_deleted,
            is_renamed,
            hunks: std::mem::take(&mut current_hunks),
        });
    }

    Ok(files)
}

/// Parse hunk header string like `@@ -1,5 +1,6 @@ optional section`
fn parse_hunk_header(header_line: &str) -> Result<(usize, usize, usize, usize, String), String> {
    let parts: Vec<&str> = header_line.split("@@").collect();
    if parts.len() < 3 {
        return Err(format!("Malformed hunk header: {}", header_line));
    }

    let coords = parts[1].trim();
    let section = parts[2..].join("@@").trim().to_string();

    let ranges: Vec<&str> = coords.split_whitespace().collect();
    if ranges.len() < 2 {
        return Err(format!("Invalid hunk coordinates: {}", coords));
    }

    let old_range = ranges[0].strip_prefix('-').ok_or_else(|| {
        format!("Missing '-' in old range: {}", ranges[0])
    })?;
    let new_range = ranges[1].strip_prefix('+').ok_or_else(|| {
        format!("Missing '+' in new range: {}", ranges[1])
    })?;

    let (old_start, old_lines) = parse_range_spec(old_range)?;
    let (new_start, new_lines) = parse_range_spec(new_range)?;

    Ok((old_start, old_lines, new_start, new_lines, section))
}

fn parse_range_spec(spec: &str) -> Result<(usize, usize), String> {
    if let Some((start_str, count_str)) = spec.split_once(',') {
        let start = start_str
            .parse::<usize>()
            .map_err(|e| format!("Invalid range start '{}': {}", start_str, e))?;
        let count = count_str
            .parse::<usize>()
            .map_err(|e| format!("Invalid range count '{}': {}", count_str, e))?;
        Ok((start, count))
    } else {
        let start = spec
            .parse::<usize>()
            .map_err(|e| format!("Invalid range start '{}': {}", spec, e))?;
        Ok((start, 1))
    }
}

// ── Offset-Adjusting Patch Reconstructor ──────────────────────────────────────

/// Reconstruct a valid unified diff from a slice of `FileDiff`s taking into account
/// hunk approvals, rejections, and modifications while adjusting line offsets dynamically.
pub fn reconstruct_approved_patch(files: &[FileDiff]) -> Result<String, String> {
    let mut output = String::new();

    for file in files {
        let mut approved_hunks_output = Vec::new();
        let mut cumulative_offset: isize = 0;

        for hunk in &file.hunks {
            match &hunk.status {
                HunkStatus::Rejected { .. } => {
                    // Rejected: Do not emit hunk. Offset delta for this hunk is 0 since old lines are retained.
                    continue;
                }
                HunkStatus::Approved => {
                    let adjusted_new_start = ((hunk.old_start as isize) + cumulative_offset).max(1) as usize;

                    let mut hunk_str = format!(
                        "@@ -{},{} +{},{} @@ {}\n",
                        hunk.old_start,
                        hunk.old_lines,
                        adjusted_new_start,
                        hunk.new_lines,
                        hunk.header
                    );

                    for line in &hunk.lines {
                        match line.line_type {
                            DiffLineType::Context => {
                                hunk_str.push(' ');
                                hunk_str.push_str(&line.content);
                                hunk_str.push('\n');
                            }
                            DiffLineType::Addition => {
                                hunk_str.push('+');
                                hunk_str.push_str(&line.content);
                                hunk_str.push('\n');
                            }
                            DiffLineType::Deletion => {
                                hunk_str.push('-');
                                hunk_str.push_str(&line.content);
                                hunk_str.push('\n');
                            }
                        }
                    }

                    approved_hunks_output.push(hunk_str);
                    cumulative_offset += (hunk.new_lines as isize) - (hunk.old_lines as isize);
                }
                HunkStatus::Modified { user_override } => {
                    // Split user override lines into additions
                    let user_lines: Vec<&str> = user_override.lines().collect();
                    let mod_new_lines = user_lines.len();
                    let adjusted_new_start = ((hunk.old_start as isize) + cumulative_offset).max(1) as usize;

                    let mut hunk_str = format!(
                        "@@ -{},{} +{},{} @@ {} (modified)\n",
                        hunk.old_start,
                        hunk.old_lines,
                        adjusted_new_start,
                        mod_new_lines,
                        hunk.header
                    );

                    // Retain old deletions or replace entirely with user content
                    for line in &hunk.lines {
                        if line.line_type == DiffLineType::Deletion {
                            hunk_str.push('-');
                            hunk_str.push_str(&line.content);
                            hunk_str.push('\n');
                        }
                    }
                    for uline in user_lines {
                        hunk_str.push('+');
                        hunk_str.push_str(uline);
                        hunk_str.push('\n');
                    }

                    approved_hunks_output.push(hunk_str);
                    cumulative_offset += (mod_new_lines as isize) - (hunk.old_lines as isize);
                }
                HunkStatus::Pending => {
                    // If still pending during reconstruction, treat as approved
                    let adjusted_new_start = ((hunk.old_start as isize) + cumulative_offset).max(1) as usize;
                    let mut hunk_str = format!(
                        "@@ -{},{} +{},{} @@ {}\n",
                        hunk.old_start,
                        hunk.old_lines,
                        adjusted_new_start,
                        hunk.new_lines,
                        hunk.header
                    );
                    for line in &hunk.lines {
                        match line.line_type {
                            DiffLineType::Context => {
                                hunk_str.push(' ');
                                hunk_str.push_str(&line.content);
                                hunk_str.push('\n');
                            }
                            DiffLineType::Addition => {
                                hunk_str.push('+');
                                hunk_str.push_str(&line.content);
                                hunk_str.push('\n');
                            }
                            DiffLineType::Deletion => {
                                hunk_str.push('-');
                                hunk_str.push_str(&line.content);
                                hunk_str.push('\n');
                            }
                        }
                    }
                    approved_hunks_output.push(hunk_str);
                    cumulative_offset += (hunk.new_lines as isize) - (hunk.old_lines as isize);
                }
            }
        }

        // If at least one hunk is approved for this file, emit file headers
        if !approved_hunks_output.is_empty() {
            let old_path = file.old_path.as_deref().unwrap_or("/dev/null");
            let new_path = file.new_path.as_deref().unwrap_or("/dev/null");

            output.push_str(&format!("--- a/{}\n", old_path.strip_prefix("a/").unwrap_or(old_path)));
            output.push_str(&format!("+++ b/{}\n", new_path.strip_prefix("b/").unwrap_or(new_path)));

            for h in approved_hunks_output {
                output.push_str(&h);
            }
        }
    }

    Ok(output)
}

// ── Thread-Safe Diff Review Registry ──────────────────────────────────────────

/// Central thread-safe registry holding active diff review sessions across Pregel executions.
pub struct DiffReviewRegistry {
    sessions: RwLock<HashMap<String, DiffReviewSession>>,
}

impl Default for DiffReviewRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl DiffReviewRegistry {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }

    pub fn global() -> &'static Arc<DiffReviewRegistry> {
        static REGISTRY: OnceLock<Arc<DiffReviewRegistry>> = OnceLock::new();
        REGISTRY.get_or_init(|| Arc::new(DiffReviewRegistry::new()))
    }

    pub fn create_session(&self, session: DiffReviewSession) {
        let mut map = self.sessions.write().unwrap_or_else(|e| e.into_inner());
        map.insert(session.session_id.clone(), session);
    }

    pub fn get_session(&self, session_id: &str) -> Option<DiffReviewSession> {
        let map = self.sessions.read().unwrap_or_else(|e| e.into_inner());
        map.get(session_id).cloned()
    }

    pub fn list_sessions(&self) -> Vec<DiffReviewSession> {
        let map = self.sessions.read().unwrap_or_else(|e| e.into_inner());
        map.values().cloned().collect()
    }

    pub fn list_pending(&self) -> Vec<DiffReviewSession> {
        let map = self.sessions.read().unwrap_or_else(|e| e.into_inner());
        map.values()
            .filter(|s| s.status == DiffReviewStatus::Pending)
            .cloned()
            .collect()
    }

    pub fn submit_decision(
        &self,
        session_id: &str,
        hunk_id: &str,
        status: HunkStatus,
    ) -> Result<DiffReviewSession, String> {
        let mut map = self.sessions.write().unwrap_or_else(|e| e.into_inner());
        let session = map
            .get_mut(session_id)
            .ok_or_else(|| format!("Session '{}' not found", session_id))?;

        session.update_hunk_status(hunk_id, status)?;
        Ok(session.clone())
    }

    pub fn submit_batch_decisions(
        &self,
        session_id: &str,
        decision_type: &str, // "approve_all", "reject_all"
    ) -> Result<DiffReviewSession, String> {
        let mut map = self.sessions.write().unwrap_or_else(|e| e.into_inner());
        let session = map
            .get_mut(session_id)
            .ok_or_else(|| format!("Session '{}' not found", session_id))?;

        for file in &mut session.files {
            for hunk in &mut file.hunks {
                if hunk.status == HunkStatus::Pending {
                    match decision_type {
                        "approve_all" => hunk.status = HunkStatus::Approved,
                        "reject_all" => {
                            hunk.status = HunkStatus::Rejected {
                                reason: Some("Batch rejected by user".to_string()),
                            }
                        }
                        _ => return Err(format!("Unknown batch decision type '{}'", decision_type)),
                    }
                }
            }
        }

        session.updated_at = Utc::now().timestamp_millis();
        session.refresh_status();
        Ok(session.clone())
    }

    pub fn remove_session(&self, session_id: &str) -> Option<DiffReviewSession> {
        let mut map = self.sessions.write().unwrap_or_else(|e| e.into_inner());
        map.remove(session_id)
    }

    pub fn clear(&self) {
        let mut map = self.sessions.write().unwrap_or_else(|e| e.into_inner());
        map.clear();
    }
}

// ── Pregel HITL Integration Helpers ──────────────────────────────────────────

/// Construct an `ApprovalContext` representing a suspended diff review session.
pub fn create_diff_review_context(session: &DiffReviewSession) -> ApprovalContext {
    ApprovalContext::new(
        session.action_id.clone(),
        "diff_reviewer",
        serde_json::json!({
            "session_id": session.session_id,
            "thread_id": session.thread_id,
            "total_hunks": session.total_hunks(),
            "files_count": session.files.len(),
            "files": session.files.iter().map(|f| f.new_path.as_deref().unwrap_or("unknown")).collect::<Vec<_>>()
        }),
        format!(
            "Code change patch containing {} hunk(s) across {} file(s) requires human review",
            session.total_hunks(),
            session.files.len()
        ),
        300, // 5-minute default timeout
    )
}

/// Evaluate whether all hunks in a session are decided, and compute the corresponding `ApprovalDecision`.
pub fn evaluate_session_decision(session: &DiffReviewSession) -> Option<ApprovalDecision> {
    if !session.is_fully_decided() {
        return None;
    }

    match session.status {
        DiffReviewStatus::FullyApproved | DiffReviewStatus::PartiallyApproved => {
            let filtered_patch = reconstruct_approved_patch(&session.files).unwrap_or_default();
            Some(ApprovalDecision::Approved {
                modified_args: Some(serde_json::json!({
                    "session_id": session.session_id,
                    "approved_patch": filtered_patch,
                    "approved_count": session.approved_hunks_count(),
                    "modified_count": session.modified_hunks_count(),
                    "rejected_count": session.rejected_hunks_count(),
                })),
            })
        }
        DiffReviewStatus::Rejected => Some(ApprovalDecision::Rejected {
            reason: Some(format!(
                "All {} diff hunk(s) were rejected by the operator",
                session.total_hunks()
            )),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_DIFF: &str = r#"--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,5 +1,6 @@
 use std::collections::HashMap;
-use std::fs;
+use std::fs::File;
+use std::io::Read;
 
 pub fn run() {
@@ -20,4 +21,5 @@
     let x = 10;
-    let y = 20;
+    let y = 30;
+    let z = 40;
 }
"#;

    #[test]
    fn test_parse_multi_hunk_diff() {
        let files = parse_unified_diff(SAMPLE_DIFF).expect("parse diff");
        assert_eq!(files.len(), 1);

        let file = &files[0];
        assert_eq!(file.old_path.as_deref(), Some("src/lib.rs"));
        assert_eq!(file.new_path.as_deref(), Some("src/lib.rs"));
        assert_eq!(file.hunks.len(), 2);

        // Hunk 1
        let h1 = &file.hunks[0];
        assert_eq!(h1.old_start, 1);
        assert_eq!(h1.old_lines, 5);
        assert_eq!(h1.new_start, 1);
        assert_eq!(h1.new_lines, 6);
        assert_eq!(h1.status, HunkStatus::Pending);

        // Hunk 2
        let h2 = &file.hunks[1];
        assert_eq!(h2.old_start, 20);
        assert_eq!(h2.old_lines, 4);
        assert_eq!(h2.new_start, 21);
        assert_eq!(h2.new_lines, 5);
    }

    #[test]
    fn test_offset_adjustment_on_selective_rejection() {
        let mut files = parse_unified_diff(SAMPLE_DIFF).expect("parse diff");

        // Reject hunk 1, approve hunk 2
        files[0].hunks[0].status = HunkStatus::Rejected {
            reason: Some("Keep old imports".to_string()),
        };
        files[0].hunks[1].status = HunkStatus::Approved;

        let reconstructed = reconstruct_approved_patch(&files).expect("reconstruct patch");

        // Hunk 1 was rejected (which had +1 line net growth).
        // Since hunk 1 is skipped, hunk 2's new_start should be aligned with old_start (20) instead of original 21.
        assert!(!reconstructed.contains("use std::fs::File;"));
        assert!(reconstructed.contains("let y = 30;"));
        assert!(reconstructed.contains("@@ -20,4 +20,5 @@"));
    }

    #[test]
    fn test_hunk_modification_and_decision_resolution() {
        let files = parse_unified_diff(SAMPLE_DIFF).expect("parse diff");
        let session = DiffReviewSession::new("sess-1", "thread-1", "act-1", files);

        let registry = DiffReviewRegistry::new();
        registry.create_session(session);

        // Submit decision for Hunk 1: Modified
        let h1_id = &registry.get_session("sess-1").unwrap().files[0].hunks[0].hunk_id;
        let s1 = registry
            .submit_decision(
                "sess-1",
                h1_id,
                HunkStatus::Modified {
                    user_override: "use std::sync::Arc;\nuse std::sync::Mutex;".to_string(),
                },
            )
            .unwrap();

        assert_eq!(s1.status, DiffReviewStatus::Pending);
        assert!(!s1.is_fully_decided());

        // Submit decision for Hunk 2: Approved
        let h2_id = &registry.get_session("sess-1").unwrap().files[0].hunks[1].hunk_id;
        let s2 = registry
            .submit_decision("sess-1", h2_id, HunkStatus::Approved)
            .unwrap();

        assert_eq!(s2.status, DiffReviewStatus::FullyApproved);
        assert!(s2.is_fully_decided());

        let decision = evaluate_session_decision(&s2).expect("decision ready");
        match decision {
            ApprovalDecision::Approved { modified_args } => {
                let args = modified_args.unwrap();
                let patch = args["approved_patch"].as_str().unwrap();
                assert!(patch.contains("+use std::sync::Arc;"));
                assert!(patch.contains("let y = 30;"));
            }
            _ => panic!("Expected approved decision"),
        }
    }

    #[test]
    fn test_batch_rejection() {
        let files = parse_unified_diff(SAMPLE_DIFF).expect("parse diff");
        let session = DiffReviewSession::new("sess-2", "thread-2", "act-2", files);

        let registry = DiffReviewRegistry::new();
        registry.create_session(session);

        let s = registry
            .submit_batch_decisions("sess-2", "reject_all")
            .unwrap();
        assert_eq!(s.status, DiffReviewStatus::Rejected);
        assert!(s.is_fully_decided());

        let decision = evaluate_session_decision(&s).expect("decision ready");
        match decision {
            ApprovalDecision::Rejected { reason } => {
                assert!(reason.unwrap().contains("rejected"));
            }
            _ => panic!("Expected rejected decision"),
        }
    }

    #[test]
    fn test_diff_review_registry_poison_recovery() {
        let registry = Arc::new(DiffReviewRegistry::new());
        let reg_clone = Arc::clone(&registry);

        // Intentionally poison the RwLock by panicking while holding the write lock
        let handle = std::thread::spawn(move || {
            let mut map = reg_clone.sessions.write().unwrap_or_else(|e| e.into_inner());
            let session = DiffReviewSession::new("sess-poison-setup", "thread-1", "act-1", vec![]);
            map.insert("sess-poison-setup".to_string(), session);
            panic!("Intentional worker panic to poison DiffReviewRegistry RwLock");
        });
        let _ = handle.join();

        // Verify that subsequent operations succeed without panic despite poisoned lock:
        // 1. Create a session (write lock)
        let session = DiffReviewSession::new("sess-recovered", "thread-2", "act-2", vec![]);
        registry.create_session(session.clone());

        // 2. Get session (read lock)
        let retrieved = registry.get_session("sess-recovered");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().session_id, "sess-recovered");

        // 3. Verify data inserted before panic is still accessible
        let setup_session = registry.get_session("sess-poison-setup");
        assert!(setup_session.is_some());
        assert_eq!(setup_session.unwrap().session_id, "sess-poison-setup");

        // 4. List sessions and pending (read lock)
        let sessions = registry.list_sessions();
        assert_eq!(sessions.len(), 2);
        let pending = registry.list_pending();
        assert_eq!(pending.len(), 2);

        // 5. Submit batch decision (write lock)
        let decision_res = registry.submit_batch_decisions("sess-recovered", "approve_all");
        assert!(decision_res.is_ok());

        // 6. Remove session (write lock)
        let removed = registry.remove_session("sess-recovered");
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().session_id, "sess-recovered");

        // 7. Clear (write lock)
        registry.clear();
        assert_eq!(registry.list_sessions().len(), 0);
    }
}
