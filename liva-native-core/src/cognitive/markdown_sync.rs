//! Bidirectional OpenClaw Markdown Sync Engine (Feature F18).
//!
//! Provides:
//! - Direct file mapping for:
//!   - `USER.md`: Core user profile & invariant instructions
//!   - `MEMORY.md`: Global knowledge & preferences
//!   - `people/<slug>.md`: People entities
//!   - `projects/<slug>.md`: Project entities
//!   - `memory/<YYYY-MM-DD>.md`: Daily turn logs
//! - Export (DB -> Markdown):
//!   - Queries active facts (`expired_at IS NULL AND valid_to IS NULL`).
//!   - Partitions facts across root files and entity subdirectories (`people/`, `projects/`, `memory/`).
//!   - Formats bullet points with embedded HTML metadata comments:
//!     `<!-- liva:id=<fact_id> sub=<subject> pred=<predicate> scope=<scope> -->`
//!   - Atomic writes via temporary swap file `<file>.tmp` + rename.
//!   - Records BLAKE3 hash in `markdown_sync_state`.
//! - Ingest (Markdown -> DB):
//!   - Scans root files and entity subdirectories (`people/`, `projects/`, `memory/`).
//!   - Compares BLAKE3 hashes to skip unchanged files.
//!   - Detects removed bullet lines: soft-invalidates zombie facts in SQLite
//!     (`valid_to = now, expired_at = now, reason = 'user_deleted_from_markdown'`)
//!     and increments `report.facts_expired += 1`.
//!   - Reverse parses delta:
//!     - Edited line: closes old fact, creates new fact with `origin = FactOrigin::UserEdited`, `confidence = 1.0`.
//!     - Added line: creates new fact with `origin = FactOrigin::UserEdited`, `confidence = 1.0`.
//!   - Enforces Active Singleton Invariant in `BEGIN IMMEDIATE` transaction.

use crate::db::bitemporal::{
    BitemporalFact, FactOrigin, FactSensitivity, atomic_supersede_fact, insert_active_fact,
    list_active_facts,
};
use regex::Regex;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use unicode_normalization::UnicodeNormalization;

/// Regex to extract embedded metadata comments in bullet lines:
/// `- **predicate**: value <!-- liva:id=fact_123 sub=user pred=language scope=personal -->`
static RE_METADATA_COMMENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<!--\s*liva:id=([^\s]+)\s+sub=([^\s]+)\s+pred=([^\s]+)\s+scope=([^\s]+)\s*-->")
        .expect("valid metadata regex")
});

/// Configuration for Markdown Sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkdownSyncConfig {
    pub workspace_root: PathBuf,
}

impl MarkdownSyncConfig {
    pub fn new<P: AsRef<Path>>(workspace_root: P) -> Self {
        Self {
            workspace_root: workspace_root.as_ref().to_path_buf(),
        }
    }
}

/// Sync summary report.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncReport {
    pub files_scanned: usize,
    pub facts_exported: usize,
    pub facts_ingested: usize,
    pub facts_updated: usize,
    pub facts_expired: usize,
    pub errors: Vec<String>,
}

pub type MarkdownSyncReport = SyncReport;

/// Slugify a string identifier safely for filesystem paths.
pub fn slugify(name: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;
    for ch in name.chars() {
        let folded = crate::db::fts_vietnamese::fold_vietnamese_char(ch);
        if folded.is_ascii_alphanumeric() {
            slug.push(folded.to_ascii_lowercase());
            last_was_dash = false;
        } else if (folded == ' ' || folded == '-' || folded == '_' || folded == '/')
            && !last_was_dash
            && !slug.is_empty()
        {
            slug.push('-');
            last_was_dash = true;
        }
    }
    let trimmed = slug.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "item".to_string()
    } else {
        trimmed
    }
}

fn is_valid_date_format(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() != 10 {
        return false;
    }
    bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[0..4].iter().all(|b| b.is_ascii_digit())
        && bytes[5..7].iter().all(|b| b.is_ascii_digit())
        && bytes[8..10].iter().all(|b| b.is_ascii_digit())
}

fn current_time_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Convert unix timestamp milliseconds to calendar date YYYY-MM-DD without external crates.
pub fn epoch_millis_to_date(ms: i64) -> String {
    let effective_ms = if ms <= 0 { current_time_ms() } else { ms };
    let mut days = effective_ms / 86_400_000;
    let mut year = 1970i64;
    loop {
        let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
        let days_in_year = if leap { 366 } else { 365 };
        if days >= days_in_year {
            days -= days_in_year;
            year += 1;
        } else if days < 0 {
            year -= 1;
            let leap_prev = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
            days += if leap_prev { 366 } else { 365 };
        } else {
            break;
        }
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let month_days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1;
    for &md in &month_days {
        if days < md {
            break;
        }
        days -= md;
        month += 1;
    }
    let day = days + 1;
    format!("{:04}-{:02}-{:02}", year, month, day)
}

fn file_header_for_path(path: &Path) -> String {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name == "USER.md" {
        "# User Profile\n\n".to_string()
    } else if name == "MEMORY.md" {
        "# Global Memory\n\n".to_string()
    } else if let Some(parent) = path.parent() {
        let parent_name = parent.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        match parent_name {
            "people" => format!("# Person: {stem}\n\n"),
            "projects" => format!("# Project: {stem}\n\n"),
            "memory" => format!("# Daily Memory: {stem}\n\n"),
            _ => format!("# {stem}\n\n"),
        }
    } else {
        "# Memory\n\n".to_string()
    }
}

pub struct MarkdownSyncManager {
    config: MarkdownSyncConfig,
}

impl MarkdownSyncManager {
    pub fn new(config: MarkdownSyncConfig) -> Self {
        Self { config }
    }

    /// Determines the canonical target file path for a fact.
    pub fn destination_file(&self, fact: &BitemporalFact) -> PathBuf {
        let root = &self.config.workspace_root;

        // 1. Episodic / date-stamped turn logs -> memory/<YYYY-MM-DD>.md
        if fact.scope == "episodic"
            || fact.scope == "daily"
            || fact.scope.starts_with("memory:")
            || fact.scope.starts_with("daily:")
            || fact.category.as_deref() == Some("episodic")
            || fact.category.as_deref() == Some("daily")
            || fact.category.as_deref() == Some("turn_log")
        {
            let date_str = if fact.scope.starts_with("memory:") {
                let s = fact.scope.trim_start_matches("memory:").trim();
                if is_valid_date_format(s) {
                    s.to_string()
                } else {
                    epoch_millis_to_date(if fact.valid_from > 0 {
                        fact.valid_from
                    } else {
                        fact.recorded_at
                    })
                }
            } else if fact.scope.starts_with("daily:") {
                let s = fact.scope.trim_start_matches("daily:").trim();
                if is_valid_date_format(s) {
                    s.to_string()
                } else {
                    epoch_millis_to_date(if fact.valid_from > 0 {
                        fact.valid_from
                    } else {
                        fact.recorded_at
                    })
                }
            } else {
                let ts = if fact.valid_from > 0 {
                    fact.valid_from
                } else if fact.recorded_at > 0 {
                    fact.recorded_at
                } else {
                    current_time_ms()
                };
                epoch_millis_to_date(ts)
            };
            return root.join("memory").join(format!("{date_str}.md"));
        }

        // 2. Project entities -> projects/<slug>.md
        if fact.scope == "project"
            || fact.scope.starts_with("project:")
            || fact.category.as_deref() == Some("project")
            || (fact.category.as_deref() == Some("architecture") && fact.subject != "user")
        {
            let slug = if fact.scope.starts_with("project:") {
                slugify(fact.scope.trim_start_matches("project:"))
            } else if fact.subject != "user" && fact.subject != "system" && !fact.subject.is_empty()
            {
                slugify(&fact.subject)
            } else if let Some(ref cat) = fact.category {
                if cat == "project" && fact.subject != "user" {
                    slugify(&fact.subject)
                } else {
                    slugify(&fact.predicate)
                }
            } else {
                slugify(&fact.predicate)
            };
            return root.join("projects").join(format!("{slug}.md"));
        }

        // 3. People entities -> people/<slug>.md
        if fact.scope == "person"
            || fact.scope == "people"
            || fact.scope.starts_with("person:")
            || fact.scope.starts_with("people:")
            || fact.category.as_deref() == Some("person")
            || fact.category.as_deref() == Some("people")
            || (fact.subject != "user"
                && fact.subject != "system"
                && fact.scope != "personal"
                && fact.scope != "global")
        {
            let slug = if fact.scope.starts_with("person:") {
                slugify(fact.scope.trim_start_matches("person:"))
            } else if fact.scope.starts_with("people:") {
                slugify(fact.scope.trim_start_matches("people:"))
            } else if fact.subject != "user" && fact.subject != "system" && !fact.subject.is_empty()
            {
                slugify(&fact.subject)
            } else {
                slugify(&fact.predicate)
            };
            return root.join("people").join(format!("{slug}.md"));
        }

        // 4. Core User Profile -> USER.md
        if fact.scope == "personal" || fact.subject == "user" {
            return root.join("USER.md");
        }

        // 5. Global Memory -> MEMORY.md
        root.join("MEMORY.md")
    }

    /// Export all active facts from DB to OpenClaw Markdown files partitioned across subdirectories.
    pub fn export_all_active_facts(
        &self,
        conn: &Connection,
    ) -> Result<SyncReport, rusqlite::Error> {
        let mut report = SyncReport::default();
        let active_facts = list_active_facts(conn, None)?;

        // Ensure directories exist
        let root = &self.config.workspace_root;
        let _ = fs::create_dir_all(root);
        let _ = fs::create_dir_all(root.join("people"));
        let _ = fs::create_dir_all(root.join("projects"));
        let _ = fs::create_dir_all(root.join("memory"));

        // Partition facts by destination file path
        let mut partitions: std::collections::BTreeMap<PathBuf, Vec<BitemporalFact>> =
            std::collections::BTreeMap::new();

        // Always ensure USER.md and MEMORY.md exist as partition targets
        partitions.entry(root.join("USER.md")).or_default();
        partitions.entry(root.join("MEMORY.md")).or_default();

        for fact in active_facts {
            let dest = self.destination_file(&fact);
            partitions.entry(dest).or_default().push(fact);
        }

        for (dest_path, facts) in partitions {
            let header = file_header_for_path(&dest_path);
            if let Err(e) = self.export_facts_to_file(conn, &dest_path, &header, &facts) {
                report
                    .errors
                    .push(format!("Export error for {dest_path:?}: {e}"));
            } else {
                report.files_scanned += 1;
                report.facts_exported += facts.len();
            }
        }

        Ok(report)
    }

    /// Ingest user edits from root files and entity subdirectories into DB.
    pub fn ingest_user_edits(&self, conn: &Connection) -> Result<SyncReport, rusqlite::Error> {
        let mut report = SyncReport::default();
        let root = &self.config.workspace_root;
        if !root.exists() {
            return Ok(report);
        }

        let now_ms = current_time_ms();

        // 1. Root files
        let user_md = root.join("USER.md");
        if user_md.exists() {
            self.ingest_single_file(conn, &user_md, "user", "personal", &mut report)?;
        }

        let memory_md = root.join("MEMORY.md");
        if memory_md.exists() {
            self.ingest_single_file(conn, &memory_md, "system", "global", &mut report)?;
        }

        // 2. Subdirectory: people/
        let people_dir = root.join("people");
        if people_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&people_dir) {
                let mut files: Vec<PathBuf> = entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.is_file() && p.extension().map_or(false, |ext| ext == "md"))
                    .collect();
                files.sort();
                for file_path in files {
                    let slug = file_path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("person");
                    let default_subject = slug;
                    let default_scope = format!("person:{}", slug);
                    self.ingest_single_file(
                        conn,
                        &file_path,
                        default_subject,
                        &default_scope,
                        &mut report,
                    )?;
                }
            }
        }

        // 3. Subdirectory: projects/
        let projects_dir = root.join("projects");
        if projects_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&projects_dir) {
                let mut files: Vec<PathBuf> = entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.is_file() && p.extension().map_or(false, |ext| ext == "md"))
                    .collect();
                files.sort();
                for file_path in files {
                    let slug = file_path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("project");
                    let default_subject = slug;
                    let default_scope = format!("project:{}", slug);
                    self.ingest_single_file(
                        conn,
                        &file_path,
                        default_subject,
                        &default_scope,
                        &mut report,
                    )?;
                }
            }
        }

        // 4. Subdirectory: memory/
        let memory_dir = root.join("memory");
        if memory_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&memory_dir) {
                let mut files: Vec<PathBuf> = entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.is_file() && p.extension().map_or(false, |ext| ext == "md"))
                    .collect();
                files.sort();
                for file_path in files {
                    let date_str = file_path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("daily");
                    let default_subject = "user";
                    let default_scope = format!("memory:{}", date_str);
                    self.ingest_single_file(
                        conn,
                        &file_path,
                        default_subject,
                        &default_scope,
                        &mut report,
                    )?;
                }
            }
        }

        // 5. Invalidate facts from tracked files that have been deleted from disk
        self.check_and_expire_deleted_files(conn, now_ms, &mut report)?;

        Ok(report)
    }

    /// Ingest a specific markdown file into DB.
    pub fn ingest_markdown_file(
        &self,
        conn: &Connection,
        file_path: &Path,
        report: &mut SyncReport,
    ) -> Result<(), rusqlite::Error> {
        let file_name = file_path.file_name().and_then(|f| f.to_str()).unwrap_or("");
        let parent_name = file_path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("");
        let stem = file_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

        let (default_subject, default_scope) = if file_name == "USER.md" {
            ("user".to_string(), "personal".to_string())
        } else if file_name == "MEMORY.md" {
            ("system".to_string(), "global".to_string())
        } else if parent_name == "people" {
            (stem.to_string(), format!("person:{}", stem))
        } else if parent_name == "projects" {
            (stem.to_string(), format!("project:{}", stem))
        } else if parent_name == "memory" {
            ("user".to_string(), format!("memory:{}", stem))
        } else {
            ("user".to_string(), "personal".to_string())
        };

        self.ingest_single_file(conn, file_path, &default_subject, &default_scope, report)
    }

    fn export_facts_to_file(
        &self,
        conn: &Connection,
        dest_path: &Path,
        header: &str,
        facts: &[BitemporalFact],
    ) -> Result<(), rusqlite::Error> {
        let mut content = String::from(header);
        for f in facts {
            let val = if !f.object_value.is_empty() {
                &f.object_value
            } else {
                &f.value
            };
            let line = format!(
                "- **{}**: {} <!-- liva:id={} sub={} pred={} scope={} -->\n",
                f.predicate, val, f.fact_id, f.subject, f.predicate, f.scope
            );
            content.push_str(&line);
        }

        let nfc_content: String = content.nfc().collect();
        let blake3_hash = blake3::hash(nfc_content.as_bytes()).to_hex().to_string();

        if let Some(parent) = dest_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        // Atomic swap write: write to .tmp and rename
        let tmp_path = dest_path.with_extension("tmp");
        if let Err(e) = fs::write(&tmp_path, &nfc_content) {
            return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(e)));
        }
        if let Err(e) = fs::rename(&tmp_path, dest_path) {
            return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(e)));
        }

        // Record sync state
        let now_ms = current_time_ms();
        let path_str = dest_path.to_string_lossy().to_string();
        conn.execute(
            "INSERT INTO markdown_sync_state (file_path, last_hash, last_synced_at, blake3_hash, synced_at)
             VALUES (?1, ?2, ?3, ?2, ?3)
             ON CONFLICT(file_path) DO UPDATE SET 
                last_hash = excluded.last_hash,
                last_synced_at = excluded.last_synced_at,
                blake3_hash = excluded.blake3_hash, 
                synced_at = excluded.synced_at",
            params![&path_str, &blake3_hash, now_ms],
        )?;

        Ok(())
    }

    fn ingest_single_file(
        &self,
        conn: &Connection,
        file_path: &Path,
        default_subject: &str,
        default_scope: &str,
        report: &mut SyncReport,
    ) -> Result<(), rusqlite::Error> {
        report.files_scanned += 1;
        let content = match fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(e) => {
                report
                    .errors
                    .push(format!("Cannot read {file_path:?}: {e}"));
                return Ok(());
            }
        };

        let nfc_content: String = content.nfc().collect();
        let current_hash = blake3::hash(nfc_content.as_bytes()).to_hex().to_string();
        let path_str = file_path.to_string_lossy().to_string();

        // Check if file has changed since last sync
        let last_hash: Option<String> = conn
            .query_row(
                "SELECT blake3_hash FROM markdown_sync_state WHERE file_path = ?1",
                [&path_str],
                |r| r.get(0),
            )
            .rusqlite_optional()?;

        if let Some(lh) = last_hash {
            if lh == current_hash {
                // No changes -> skip
                return Ok(());
            }
        }

        let now_ms = current_time_ms();

        // Ingest inside an immediate transaction
        conn.execute_batch("BEGIN IMMEDIATE;")?;

        let res = (|| -> Result<(), rusqlite::Error> {
            // 1. Collect all fact_ids present in comments of this file
            let mut present_fact_ids = std::collections::HashSet::new();
            for line in nfc_content.lines() {
                let trimmed = line.trim();
                if let Some(caps) = RE_METADATA_COMMENT.captures(trimmed) {
                    if let Some(m) = caps.get(1) {
                        let fid = m.as_str().trim();
                        if !fid.is_empty() {
                            present_fact_ids.insert(fid.to_string());
                        }
                    }
                }
            }

            // 2. Identify and soft-invalidate Zombie Facts (active facts belonging to this file that user deleted)
            let active_facts = list_active_facts(conn, None)?;
            for fact in active_facts {
                if self.destination_file(&fact) == file_path {
                    if !present_fact_ids.contains(&fact.fact_id) {
                        conn.execute(
                            "UPDATE facts 
                             SET valid_to = ?1, expired_at = ?1, reason = 'user_deleted_from_markdown', updated_at = ?1 
                             WHERE fact_id = ?2 AND expired_at IS NULL AND valid_to IS NULL",
                            params![now_ms, &fact.fact_id],
                        )?;
                        report.facts_expired += 1;
                    }
                }
            }

            // 3. Parse existing lines and fresh user-added lines
            for line in nfc_content.lines() {
                let trimmed = line.trim();
                if !trimmed.starts_with('-') && !trimmed.starts_with('*') {
                    continue;
                }

                if let Some(caps) = RE_METADATA_COMMENT.captures(trimmed) {
                    let fact_id = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                    let sub = caps.get(2).map(|m| m.as_str()).unwrap_or(default_subject);
                    let pred = caps.get(3).map(|m| m.as_str()).unwrap_or("note");
                    let sc = caps.get(4).map(|m| m.as_str()).unwrap_or(default_scope);

                    let line_no_comment = RE_METADATA_COMMENT.replace(trimmed, "").to_string();
                    let clean_line = line_no_comment
                        .trim()
                        .trim_start_matches('-')
                        .trim_start_matches('*')
                        .trim();

                    let new_value = if let Some((_k, val)) = clean_line.split_once(':') {
                        val.trim().to_string()
                    } else {
                        clean_line.to_string()
                    };

                    let existing_val: Option<String> = conn
                        .query_row(
                            "SELECT CASE WHEN object_value != '' THEN object_value ELSE value END FROM facts WHERE fact_id = ?1",
                            [fact_id],
                            |r| r.get(0),
                        )
                        .rusqlite_optional()?;

                    match existing_val {
                        Some(old_val) => {
                            if old_val != new_value {
                                let _ = atomic_supersede_fact(
                                    conn,
                                    fact_id,
                                    &new_value,
                                    FactOrigin::UserEdited,
                                    1.0,
                                    now_ms,
                                    Some("user_markdown_edit"),
                                    None,
                                )?;
                                report.facts_updated += 1;
                            }
                        }
                        None => {
                            let new_fact = BitemporalFact {
                                fact_id: String::new(),
                                subject: sub.to_string(),
                                predicate: pred.to_string(),
                                value: new_value.clone(),
                                object_value: new_value,
                                scope: sc.to_string(),
                                origin: FactOrigin::UserEdited,
                                confidence: 1.0,
                                valid_from: now_ms,
                                valid_to: None,
                                recorded_at: now_ms,
                                expired_at: None,
                                superseded_by: None,
                                reason: None,
                                supersede_reason: None,
                                category: None,
                                key: None,
                                importance: 5,
                                sensitivity: FactSensitivity::Normal,
                                created_at: now_ms,
                                updated_at: now_ms,
                            };
                            let _ = insert_active_fact(conn, &new_fact, None)?;
                            report.facts_ingested += 1;
                        }
                    }
                } else {
                    let clean_line = trimmed
                        .trim_start_matches('-')
                        .trim_start_matches('*')
                        .trim();
                    let (pred, val) = if let Some((k, v)) = clean_line.split_once(':') {
                        let cleaned_k = k.trim().trim_matches('*').to_string();
                        (cleaned_k, v.trim().to_string())
                    } else {
                        ("note".to_string(), clean_line.to_string())
                    };

                    if !val.is_empty() {
                        let new_fact = BitemporalFact {
                            fact_id: String::new(),
                            subject: default_subject.to_string(),
                            predicate: pred,
                            value: val.clone(),
                            object_value: val,
                            scope: default_scope.to_string(),
                            origin: FactOrigin::UserEdited,
                            confidence: 1.0,
                            valid_from: now_ms,
                            valid_to: None,
                            recorded_at: now_ms,
                            expired_at: None,
                            superseded_by: None,
                            reason: None,
                            supersede_reason: None,
                            category: None,
                            key: None,
                            importance: 5,
                            sensitivity: FactSensitivity::Normal,
                            created_at: now_ms,
                            updated_at: now_ms,
                        };

                        let existing_active: Option<String> = conn
                            .query_row(
                                "SELECT fact_id FROM facts WHERE subject = ?1 AND predicate = ?2 AND scope = ?3 AND expired_at IS NULL AND valid_to IS NULL",
                                params![&new_fact.subject, &new_fact.predicate, &new_fact.scope],
                                |r| r.get(0),
                            )
                            .rusqlite_optional()?;

                        if let Some(old_id) = existing_active {
                            let _ = atomic_supersede_fact(
                                conn,
                                &old_id,
                                &new_fact.object_value,
                                FactOrigin::UserEdited,
                                1.0,
                                now_ms,
                                Some("user_markdown_edit"),
                                None,
                            )?;
                            report.facts_updated += 1;
                        } else {
                            let _ = insert_active_fact(conn, &new_fact, None)?;
                            report.facts_ingested += 1;
                        }
                    }
                }
            }

            // 4. Update sync state
            conn.execute(
                "INSERT INTO markdown_sync_state (file_path, last_hash, last_synced_at, blake3_hash, synced_at)
                 VALUES (?1, ?2, ?3, ?2, ?3)
                 ON CONFLICT(file_path) DO UPDATE SET 
                    last_hash = excluded.last_hash,
                    last_synced_at = excluded.last_synced_at,
                    blake3_hash = excluded.blake3_hash, 
                    synced_at = excluded.synced_at",
                params![&path_str, &current_hash, now_ms],
            )?;

            Ok(())
        })();

        match res {
            Ok(()) => {
                conn.execute_batch("COMMIT;")?;
                Ok(())
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK;");
                Err(e)
            }
        }
    }

    fn check_and_expire_deleted_files(
        &self,
        conn: &Connection,
        now_ms: i64,
        report: &mut SyncReport,
    ) -> Result<(), rusqlite::Error> {
        let mut stmt = conn.prepare("SELECT file_path FROM markdown_sync_state")?;
        let tracked_files: Vec<String> = stmt
            .query_map([], |r| r.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        for tf in tracked_files {
            let p = PathBuf::from(&tf);
            if !p.exists() {
                // File was deleted on disk -> soft-invalidate its active facts
                let active_facts = list_active_facts(conn, None)?;
                for fact in active_facts {
                    if self.destination_file(&fact) == p {
                        conn.execute(
                            "UPDATE facts 
                             SET valid_to = ?1, expired_at = ?1, reason = 'user_deleted_markdown_file', updated_at = ?1 
                             WHERE fact_id = ?2 AND expired_at IS NULL AND valid_to IS NULL",
                            params![now_ms, &fact.fact_id],
                        )?;
                        report.facts_expired += 1;
                    }
                }
                let _ = conn.execute(
                    "DELETE FROM markdown_sync_state WHERE file_path = ?1",
                    [&tf],
                );
            }
        }
        Ok(())
    }
}

trait RusqliteOptionalExt<T> {
    fn rusqlite_optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> RusqliteOptionalExt<T> for Result<T, rusqlite::Error> {
    fn rusqlite_optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::bitemporal::init_bitemporal_schema;

    fn setup_test_env() -> (Connection, PathBuf) {
        let conn = Connection::open_in_memory().expect("open memory db");
        init_bitemporal_schema(&conn).expect("init bitemporal schema");
        let temp_dir =
            std::env::temp_dir().join(format!("liva_md_sync_test_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        (conn, temp_dir)
    }

    #[test]
    fn test_markdown_export_active_facts_all_partitions() {
        let (conn, dir) = setup_test_env();
        let now = 1700000000000i64; // 2023-11-14

        // 1. User profile fact -> USER.md
        let fact_user = BitemporalFact {
            fact_id: "fact_user_lang".to_string(),
            subject: "user".to_string(),
            predicate: "language".to_string(),
            value: "Vietnamese".to_string(),
            object_value: "Vietnamese".to_string(),
            scope: "personal".to_string(),
            origin: FactOrigin::Explicit,
            confidence: 1.0,
            importance: 5,
            sensitivity: FactSensitivity::Normal,
            valid_from: now,
            valid_to: None,
            recorded_at: now,
            expired_at: None,
            superseded_by: None,
            reason: None,
            supersede_reason: None,
            category: Some("profile".to_string()),
            key: Some("user:language".to_string()),
            created_at: now,
            updated_at: now,
        };

        // 2. Project fact -> projects/liva.md
        let fact_proj = BitemporalFact {
            fact_id: "fact_proj_tech".to_string(),
            subject: "liva".to_string(),
            predicate: "backend".to_string(),
            value: "Rust".to_string(),
            object_value: "Rust".to_string(),
            scope: "project".to_string(),
            origin: FactOrigin::Explicit,
            confidence: 1.0,
            importance: 5,
            sensitivity: FactSensitivity::Normal,
            valid_from: now,
            valid_to: None,
            recorded_at: now,
            expired_at: None,
            superseded_by: None,
            reason: None,
            supersede_reason: None,
            category: Some("architecture".to_string()),
            key: Some("liva:backend".to_string()),
            created_at: now,
            updated_at: now,
        };

        // 3. Global system fact -> MEMORY.md
        let fact_global = BitemporalFact {
            fact_id: "fact_sys_mode".to_string(),
            subject: "system".to_string(),
            predicate: "mode".to_string(),
            value: "autonomous".to_string(),
            object_value: "autonomous".to_string(),
            scope: "global".to_string(),
            origin: FactOrigin::Explicit,
            confidence: 1.0,
            importance: 5,
            sensitivity: FactSensitivity::Normal,
            valid_from: now,
            valid_to: None,
            recorded_at: now,
            expired_at: None,
            superseded_by: None,
            reason: None,
            supersede_reason: None,
            category: Some("system".to_string()),
            key: Some("system:mode".to_string()),
            created_at: now,
            updated_at: now,
        };

        // 4. Person fact -> people/alice.md
        let fact_person = BitemporalFact {
            fact_id: "fact_person_alice".to_string(),
            subject: "alice".to_string(),
            predicate: "role".to_string(),
            value: "architect".to_string(),
            object_value: "architect".to_string(),
            scope: "person".to_string(),
            origin: FactOrigin::Explicit,
            confidence: 1.0,
            importance: 5,
            sensitivity: FactSensitivity::Normal,
            valid_from: now,
            valid_to: None,
            recorded_at: now,
            expired_at: None,
            superseded_by: None,
            reason: None,
            supersede_reason: None,
            category: Some("person".to_string()),
            key: Some("alice:role".to_string()),
            created_at: now,
            updated_at: now,
        };

        // 5. Daily episodic fact -> memory/2023-11-14.md
        let fact_episodic = BitemporalFact {
            fact_id: "fact_daily_event".to_string(),
            subject: "user".to_string(),
            predicate: "meeting".to_string(),
            value: "reviewed milestone 3".to_string(),
            object_value: "reviewed milestone 3".to_string(),
            scope: "episodic".to_string(),
            origin: FactOrigin::Explicit,
            confidence: 1.0,
            importance: 5,
            sensitivity: FactSensitivity::Normal,
            valid_from: now,
            valid_to: None,
            recorded_at: now,
            expired_at: None,
            superseded_by: None,
            reason: None,
            supersede_reason: None,
            category: Some("episodic".to_string()),
            key: Some("turn:meeting".to_string()),
            created_at: now,
            updated_at: now,
        };

        insert_active_fact(&conn, &fact_user, None).expect("insert fact_user");
        insert_active_fact(&conn, &fact_proj, None).expect("insert fact_proj");
        insert_active_fact(&conn, &fact_global, None).expect("insert fact_global");
        insert_active_fact(&conn, &fact_person, None).expect("insert fact_person");
        insert_active_fact(&conn, &fact_episodic, None).expect("insert fact_episodic");

        let manager = MarkdownSyncManager::new(MarkdownSyncConfig::new(&dir));
        let report = manager
            .export_all_active_facts(&conn)
            .expect("export facts");

        assert_eq!(report.facts_exported, 5);

        // Check USER.md
        let user_md_path = dir.join("USER.md");
        assert!(user_md_path.exists());
        let user_md_content = fs::read_to_string(&user_md_path).expect("read USER.md");
        assert!(user_md_content.contains("- **language**: Vietnamese"));
        assert!(user_md_content.contains("sub=user pred=language scope=personal"));

        // Check MEMORY.md
        let memory_md_path = dir.join("MEMORY.md");
        assert!(memory_md_path.exists());
        let memory_md_content = fs::read_to_string(&memory_md_path).expect("read MEMORY.md");
        assert!(memory_md_content.contains("- **mode**: autonomous"));
        assert!(memory_md_content.contains("sub=system pred=mode scope=global"));

        // Check projects/liva.md
        let proj_md_path = dir.join("projects").join("liva.md");
        assert!(proj_md_path.exists());
        let proj_content = fs::read_to_string(&proj_md_path).expect("read projects/liva.md");
        assert!(proj_content.contains("- **backend**: Rust"));
        assert!(proj_content.contains("sub=liva pred=backend scope=project"));

        // Check people/alice.md
        let person_md_path = dir.join("people").join("alice.md");
        assert!(person_md_path.exists());
        let person_content = fs::read_to_string(&person_md_path).expect("read people/alice.md");
        assert!(person_content.contains("- **role**: architect"));
        assert!(person_content.contains("sub=alice pred=role scope=person"));

        // Check memory/2023-11-14.md
        let daily_md_path = dir.join("memory").join("2023-11-14.md");
        assert!(daily_md_path.exists());
        let daily_content = fs::read_to_string(&daily_md_path).expect("read memory/2023-11-14.md");
        assert!(daily_content.contains("- **meeting**: reviewed milestone 3"));

        // Clean up
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_markdown_ingest_user_edit_and_fresh_lines() {
        let (mut conn, dir) = setup_test_env();
        let now = 1700000000000i64;

        let fact1 = BitemporalFact {
            fact_id: "fact_init_theme".to_string(),
            subject: "user".to_string(),
            predicate: "theme".to_string(),
            value: "dark".to_string(),
            object_value: "dark".to_string(),
            scope: "personal".to_string(),
            origin: FactOrigin::Explicit,
            confidence: 1.0,
            importance: 5,
            sensitivity: FactSensitivity::Normal,
            valid_from: now,
            valid_to: None,
            recorded_at: now,
            expired_at: None,
            superseded_by: None,
            reason: None,
            supersede_reason: None,
            category: Some("profile".to_string()),
            key: Some("user:theme".to_string()),
            created_at: now,
            updated_at: now,
        };

        insert_active_fact(&conn, &fact1, None).expect("insert fact1");

        let manager = MarkdownSyncManager::new(MarkdownSyncConfig::new(&dir));
        let _ = manager
            .export_all_active_facts(&conn)
            .expect("export facts");

        // User edit: change "dark" to "light", and add fresh line "- **editor**: neovim"
        let user_md_path = dir.join("USER.md");
        let edited_content = "# USER PROFILE\n\n\
- **theme**: light <!-- liva:id=fact_init_theme sub=user pred=theme scope=personal -->\n\
- **editor**: neovim\n";
        fs::write(&user_md_path, edited_content).expect("write edited USER.md");

        let mut report = SyncReport::default();
        manager
            .ingest_markdown_file(&mut conn, &user_md_path, &mut report)
            .expect("ingest");

        assert_eq!(
            report.facts_updated, 1,
            "Theme should be superseded/updated"
        );
        assert_eq!(report.facts_ingested, 1, "Editor should be newly ingested");

        // Verify old fact is superseded
        let old_active: Option<String> = conn
            .query_row(
                "SELECT superseded_by FROM facts WHERE fact_id = 'fact_init_theme'",
                [],
                |r| r.get(0),
            )
            .expect("query old fact");
        assert!(old_active.is_some(), "Old fact must have superseded_by set");

        // Verify active facts
        let active_facts = list_active_facts(&conn, Some("personal")).expect("list active");
        assert_eq!(active_facts.len(), 2);

        let theme_fact = active_facts
            .iter()
            .find(|f| f.predicate == "theme")
            .expect("find theme");
        assert_eq!(theme_fact.object_value, "light");
        assert_eq!(theme_fact.origin, FactOrigin::UserEdited);
        assert_eq!(theme_fact.confidence, 1.0);

        let editor_fact = active_facts
            .iter()
            .find(|f| f.predicate == "editor")
            .expect("find editor");
        assert_eq!(editor_fact.object_value, "neovim");
        assert_eq!(editor_fact.origin, FactOrigin::UserEdited);
        assert_eq!(editor_fact.confidence, 1.0);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_markdown_ingest_zombie_fact_invalidation() {
        let (mut conn, dir) = setup_test_env();
        let now = 1700000000000i64;

        // Insert two facts into USER.md
        let fact1 = BitemporalFact {
            fact_id: "fact_to_delete".to_string(),
            subject: "user".to_string(),
            predicate: "theme".to_string(),
            value: "dark".to_string(),
            object_value: "dark".to_string(),
            scope: "personal".to_string(),
            origin: FactOrigin::Explicit,
            confidence: 1.0,
            importance: 5,
            sensitivity: FactSensitivity::Normal,
            valid_from: now,
            valid_to: None,
            recorded_at: now,
            expired_at: None,
            superseded_by: None,
            reason: None,
            supersede_reason: None,
            category: Some("profile".to_string()),
            key: Some("user:theme".to_string()),
            created_at: now,
            updated_at: now,
        };
        let fact2 = BitemporalFact {
            fact_id: "fact_to_keep".to_string(),
            subject: "user".to_string(),
            predicate: "editor".to_string(),
            value: "neovim".to_string(),
            object_value: "neovim".to_string(),
            scope: "personal".to_string(),
            origin: FactOrigin::Explicit,
            confidence: 1.0,
            importance: 5,
            sensitivity: FactSensitivity::Normal,
            valid_from: now,
            valid_to: None,
            recorded_at: now,
            expired_at: None,
            superseded_by: None,
            reason: None,
            supersede_reason: None,
            category: Some("profile".to_string()),
            key: Some("user:editor".to_string()),
            created_at: now,
            updated_at: now,
        };

        insert_active_fact(&conn, &fact1, None).expect("insert fact1");
        insert_active_fact(&conn, &fact2, None).expect("insert fact2");

        let manager = MarkdownSyncManager::new(MarkdownSyncConfig::new(&dir));
        manager
            .export_all_active_facts(&conn)
            .expect("export facts");

        // User deletes `theme` line in Obsidian: only `editor` remains
        let user_md_path = dir.join("USER.md");
        let remaining_content = "# User Profile\n\n\
- **editor**: neovim <!-- liva:id=fact_to_keep sub=user pred=editor scope=personal -->\n";
        fs::write(&user_md_path, remaining_content).expect("write modified USER.md");

        // Ingest user edits
        let report = manager.ingest_user_edits(&mut conn).expect("ingest");
        assert_eq!(
            report.facts_expired, 1,
            "Deleted line must trigger soft-invalidation"
        );

        // Verify SQLite state: fact_to_delete is expired with reason 'user_deleted_from_markdown'
        let (valid_to, expired_at, reason): (Option<i64>, Option<i64>, Option<String>) = conn
            .query_row(
                "SELECT valid_to, expired_at, reason FROM facts WHERE fact_id = 'fact_to_delete'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .expect("query deleted fact");

        assert!(
            valid_to.is_some(),
            "valid_to must be set upon markdown deletion"
        );
        assert!(
            expired_at.is_some(),
            "expired_at must be set upon markdown deletion"
        );
        assert_eq!(reason.as_deref(), Some("user_deleted_from_markdown"));

        // Verify fact_to_keep is still active
        let active_facts = list_active_facts(&conn, Some("personal")).expect("list active");
        assert_eq!(active_facts.len(), 1);
        assert_eq!(active_facts[0].fact_id, "fact_to_keep");

        // Re-export and verify Zombie Fact does NOT resurrect in USER.md
        manager.export_all_active_facts(&conn).expect("re-export");
        let exported_content = fs::read_to_string(&user_md_path).expect("read re-exported");
        assert!(
            !exported_content.contains("theme"),
            "Zombie fact must NOT resurrect!"
        );
        assert!(exported_content.contains("editor"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_markdown_entity_partition_and_ingestion() {
        let (mut conn, dir) = setup_test_env();
        let now = 1700000000000i64;

        let fact_person = BitemporalFact {
            fact_id: "fact_person_bob".to_string(),
            subject: "bob".to_string(),
            predicate: "phone".to_string(),
            value: "0912345678".to_string(),
            object_value: "0912345678".to_string(),
            scope: "person:bob".to_string(),
            origin: FactOrigin::Explicit,
            confidence: 1.0,
            importance: 5,
            sensitivity: FactSensitivity::Normal,
            valid_from: now,
            valid_to: None,
            recorded_at: now,
            expired_at: None,
            superseded_by: None,
            reason: None,
            supersede_reason: None,
            category: Some("person".to_string()),
            key: Some("bob:phone".to_string()),
            created_at: now,
            updated_at: now,
        };

        insert_active_fact(&conn, &fact_person, None).expect("insert person fact");

        let manager = MarkdownSyncManager::new(MarkdownSyncConfig::new(&dir));
        manager.export_all_active_facts(&conn).expect("export");

        let bob_md_path = dir.join("people").join("bob.md");
        assert!(bob_md_path.exists());

        // User edits bob.md: updates phone and adds city
        let edited_bob = "# Person: bob\n\n\
- **phone**: 0987654321 <!-- liva:id=fact_person_bob sub=bob pred=phone scope=person:bob -->\n\
- **city**: Da Nang\n";
        fs::write(&bob_md_path, edited_bob).expect("write edited bob.md");

        let report = manager
            .ingest_user_edits(&mut conn)
            .expect("ingest user edits");
        assert_eq!(report.facts_updated, 1);
        assert_eq!(report.facts_ingested, 1);

        let active_facts = list_active_facts(&conn, None).expect("list active");
        let phone_fact = active_facts
            .iter()
            .find(|f| f.predicate == "phone")
            .expect("find phone");
        assert_eq!(phone_fact.object_value, "0987654321");
        assert_eq!(phone_fact.origin, FactOrigin::UserEdited);

        let city_fact = active_facts
            .iter()
            .find(|f| f.predicate == "city")
            .expect("find city");
        assert_eq!(city_fact.object_value, "Da Nang");
        assert_eq!(city_fact.subject, "bob");
        assert_eq!(city_fact.scope, "person:bob");

        let _ = fs::remove_dir_all(&dir);
    }
}
