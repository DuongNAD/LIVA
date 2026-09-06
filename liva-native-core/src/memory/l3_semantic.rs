use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use regex::Regex;
use serde::{Deserialize, Serialize};
use crate::db::DatabasePool;
use crate::memory::graph::csr::CsrGraph;

/// Security and access control errors for Obsidian Vault path validation.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum VaultSecurityError {
    #[error("Access denied: Invalid URL-encoded path: {0}")]
    InvalidUrlEncoding(String),

    #[error("Access denied: Path contains null bytes")]
    ContainsNullBytes,

    #[error("Access denied: Path contains control characters")]
    ContainsControlChars,

    #[error("Invalid vault root configuration: {0}")]
    InvalidVaultRoot(String),

    #[error("Access denied: Path resolves outside the vault directory")]
    EscapesVault(String),

    #[error("Access denied: Symlink loop detected")]
    SymlinkLoopDetected(String),

    #[error("I/O error during path resolution: {0}")]
    IoError(String),
}

/// Normalizes path string by stripping Windows UNC prefix and lowercasing drive letter.
pub fn clean_path_buf(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    let mut cleaned = s.as_ref();
    if cleaned.starts_with(r"\\?\") {
        cleaned = &cleaned[4..];
    }
    #[cfg(windows)]
    {
        if cleaned.len() >= 2 && cleaned.as_bytes()[1] == b':' {
            let mut res = String::with_capacity(cleaned.len());
            res.push((cleaned.as_bytes()[0] as char).to_ascii_lowercase());
            res.push_str(&cleaned[1..]);
            return PathBuf::from(res);
        }
    }
    PathBuf::from(cleaned)
}

/// Percent-decode a URL-encoded path string (e.g., `%2e%2e%2f` -> `../`).
pub fn percent_decode_path(input: &str) -> Result<String, VaultSecurityError> {
    let mut bytes = Vec::with_capacity(input.len());
    let input_bytes = input.as_bytes();
    let mut i = 0;
    while i < input_bytes.len() {
        if input_bytes[i] == b'%' {
            if i + 2 >= input_bytes.len() {
                return Err(VaultSecurityError::InvalidUrlEncoding(input.to_string()));
            }
            let h1 = input_bytes[i + 1];
            let h2 = input_bytes[i + 2];
            let hex_bytes = [h1, h2];
            let hex_str = match std::str::from_utf8(&hex_bytes) {
                Ok(s) => s,
                Err(_) => return Err(VaultSecurityError::InvalidUrlEncoding(input.to_string())),
            };
            match u8::from_str_radix(hex_str, 16) {
                Ok(byte_val) => {
                    bytes.push(byte_val);
                    i += 3;
                }
                Err(_) => return Err(VaultSecurityError::InvalidUrlEncoding(input.to_string())),
            }
        } else {
            bytes.push(input_bytes[i]);
            i += 1;
        }
    }

    String::from_utf8(bytes).map_err(|_| VaultSecurityError::InvalidUrlEncoding(input.to_string()))
}

/// Validates that the requested target path is safely contained within the vault directory.
/// Resolves relative paths, prevents directory traversal, verifies symlinks, and detects circular symlinks.
pub fn validate_and_resolve_path(
    vault_root: &Path,
    input_path: &str,
) -> Result<PathBuf, VaultSecurityError> {
    let mut visited = HashSet::new();
    validate_and_resolve_path_internal(vault_root, input_path, &mut visited)
}

fn validate_and_resolve_path_internal(
    vault_root: &Path,
    input_path: &str,
    visited: &mut HashSet<PathBuf>,
) -> Result<PathBuf, VaultSecurityError> {
    // 1. URL decode input path to guard against encoded traversals
    let decoded = percent_decode_path(input_path)?;

    // 2. Reject null bytes and control characters
    if decoded.contains('\0') {
        return Err(VaultSecurityError::ContainsNullBytes);
    }
    if decoded.chars().any(|c| (c as u32) <= 0x1F || (c as u32) == 0x7F) {
        return Err(VaultSecurityError::ContainsControlChars);
    }

    // 3. Obtain canonical vault root path
    let canonical_vault_root = fs::canonicalize(vault_root)
        .map(|p| clean_path_buf(&p))
        .map_err(|e| VaultSecurityError::InvalidVaultRoot(e.to_string()))?;
    let clean_vault_root = clean_path_buf(vault_root);

    if visited.is_empty() {
        visited.insert(canonical_vault_root.clone());
        visited.insert(clean_vault_root.clone());
    }

    // 4. Resolve the target path relative to canonical root
    let normalized_input = decoded.replace('\\', "/");
    let trimmed_input = normalized_input.trim_start_matches('/');

    let decoded_path = Path::new(&decoded);
    let target_path = if decoded_path.is_absolute() {
        let clean_abs = clean_path_buf(decoded_path);
        if clean_abs.starts_with(&canonical_vault_root) {
            clean_abs
        } else if clean_abs.starts_with(&clean_vault_root) {
            if let Ok(rel) = clean_abs.strip_prefix(&clean_vault_root) {
                canonical_vault_root.join(rel)
            } else {
                clean_abs
            }
        } else {
            // Treat root-relative path (e.g. "/Knowledge/test.md") as relative to vault root
            canonical_vault_root.join(trimmed_input)
        }
    } else {
        canonical_vault_root.join(trimmed_input)
    };

    // 5. Fast-path check: relative containment
    let relative = match target_path.strip_prefix(&canonical_vault_root) {
        Ok(r) => r,
        Err(_) => return Err(VaultSecurityError::EscapesVault(input_path.to_string())),
    };

    if relative.components().any(|c| c == std::path::Component::ParentDir) {
        return Err(VaultSecurityError::EscapesVault(input_path.to_string()));
    }

    // 6. Deep verification: step segment by segment to resolve symlinks
    let mut current_path = canonical_vault_root.clone();

    for component in relative.components() {
        match component {
            std::path::Component::Normal(seg) => {
                let next_path = current_path.join(seg);
                match fs::symlink_metadata(&next_path) {
                    Ok(meta) => {
                        if meta.file_type().is_symlink() {
                            let clean_next = clean_path_buf(&next_path);
                            let canonical_next = if let Ok(canon) = fs::canonicalize(&next_path) {
                                clean_path_buf(&canon)
                            } else if let Ok(rel) = clean_next.strip_prefix(&clean_vault_root) {
                                canonical_vault_root.join(rel)
                            } else if let Ok(rel) = clean_next.strip_prefix(&canonical_vault_root) {
                                canonical_vault_root.join(rel)
                            } else {
                                clean_next
                            };

                            if visited.contains(&canonical_next) || visited.contains(&next_path) {
                                return Err(VaultSecurityError::SymlinkLoopDetected(
                                    canonical_next.to_string_lossy().to_string(),
                                ));
                            }
                            visited.insert(canonical_next);
                            visited.insert(next_path.clone());

                            let target = fs::read_link(&next_path)
                                .map_err(|e| VaultSecurityError::IoError(e.to_string()))?;
                            let resolved_target = if target.is_relative() {
                                current_path.join(&target)
                            } else {
                                target
                            };

                            let clean_target = clean_path_buf(&resolved_target);
                            let canonical_target = if let Ok(canon) = fs::canonicalize(&resolved_target) {
                                clean_path_buf(&canon)
                            } else if let Ok(rel) = clean_target.strip_prefix(&clean_vault_root) {
                                canonical_vault_root.join(rel)
                            } else if let Ok(rel) = clean_target.strip_prefix(&canonical_vault_root) {
                                canonical_vault_root.join(rel)
                            } else {
                                clean_target
                            };

                            if visited.contains(&canonical_target) || visited.contains(&resolved_target) {
                                return Err(VaultSecurityError::SymlinkLoopDetected(
                                    canonical_target.to_string_lossy().to_string(),
                                ));
                            }
                            visited.insert(canonical_target.clone());
                            visited.insert(resolved_target.clone());

                            // Recursively validate symlink target containment
                            current_path = validate_and_resolve_path_internal(
                                &canonical_vault_root,
                                &canonical_target.to_string_lossy(),
                                visited,
                            )?;
                        } else {
                            current_path = next_path;
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        // Nonexistent segments cannot be symlinks or escape
                        current_path = next_path;
                    }
                    Err(e) => return Err(VaultSecurityError::IoError(e.to_string())),
                }
            }
            std::path::Component::CurDir => continue,
            std::path::Component::ParentDir => {
                return Err(VaultSecurityError::EscapesVault(input_path.to_string()));
            }
            _ => {}
        }
    }

    // 7. Final containment double-check
    let final_clean = clean_path_buf(&current_path);
    if !final_clean.starts_with(&canonical_vault_root) {
        return Err(VaultSecurityError::EscapesVault(input_path.to_string()));
    }

    Ok(final_clean)
}

/// Parsed YAML Frontmatter fields.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Frontmatter {
    pub title: String,
    pub tags: Vec<String>,
    pub author: String,
    pub last_update: String,
    pub status: Option<String>,
    pub custom: HashMap<String, serde_json::Value>,
}

/// Parsed [[wikilink]] internal reference.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WikiLink {
    /// Target note name or path (e.g., "MemoryArchitecture")
    pub target: String,
    /// Optional section/heading anchor (e.g., "Overview")
    pub section: Option<String>,
    /// Optional custom display alias (e.g., "Memory Architecture Document")
    pub alias: Option<String>,
    /// 1-based line number where the link appears in the source file
    pub line: usize,
    /// Raw link text
    pub raw: String,
}

/// In-memory representation of a vault note.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultNote {
    pub relative_path: String,
    pub absolute_path: PathBuf,
    pub title: String,
    pub frontmatter: Frontmatter,
    pub body: String,
    pub links: Vec<WikiLink>,
    pub is_template: bool,
    pub last_modified_ms: u64,
}

/// Result of validating a single vault file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileValidationResult {
    pub file_path: String,
    pub relative_path: String,
    pub is_valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub is_template: bool,
}

/// Complete validation summary report for the Obsidian Vault.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VaultScanReport {
    pub total_files_checked: usize,
    pub invalid_files_count: usize,
    pub broken_links_count: usize,
    pub errors: Vec<String>,
    pub file_results: Vec<FileValidationResult>,
}

/// Helper to normalize strings for loose matching (stripping non-alphanumerics and lowercasing).
pub fn normalize_string(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

/// Native YAML frontmatter parser for Obsidian notes.
pub fn parse_frontmatter(content: &str) -> Result<(Frontmatter, &str), String> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return Err("Missing opening YAML frontmatter delimiter '---'".to_string());
    }

    let after_open = &trimmed[3..];
    let end_idx = match after_open.find("\n---") {
        Some(idx) => idx,
        None => return Err("Missing closing YAML frontmatter delimiter '---'".to_string()),
    };

    let yaml_block = &after_open[..end_idx];
    let mut body = &after_open[end_idx + 4..];
    if body.starts_with('\n') {
        body = &body[1..];
    }

    let mut fm = Frontmatter::default();
    let mut current_key: Option<String> = None;
    let mut current_list: Vec<String> = Vec::new();

    for raw_line in yaml_block.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line.starts_with('-') && current_key.is_some() {
            let item = line.trim_start_matches('-').trim();
            let unquoted = item.trim_matches(|c| c == '\'' || c == '"');
            current_list.push(unquoted.to_string());
            continue;
        }

        // Flush accumulated list
        if let Some(ref k) = current_key {
            if !current_list.is_empty() {
                if k == "tags" {
                    fm.tags.extend(current_list.drain(..));
                } else {
                    fm.custom.insert(
                        k.clone(),
                        serde_json::Value::Array(
                            current_list.drain(..).map(serde_json::Value::String).collect(),
                        ),
                    );
                }
            }
        }

        if let Some(colon_idx) = line.find(':') {
            let key = line[..colon_idx].trim().to_string();
            let value_str = line[colon_idx + 1..].trim();
            current_key = Some(key.clone());

            if value_str.is_empty() {
                // Potential multiline list following
                continue;
            }

            // Inline array [a, b, c]
            if value_str.starts_with('[') && value_str.ends_with(']') {
                let inner = &value_str[1..value_str.len() - 1];
                let items: Vec<String> = inner
                    .split(',')
                    .map(|s| s.trim().trim_matches(|c| c == '\'' || c == '"').to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                if key == "tags" {
                    fm.tags = items;
                } else {
                    fm.custom.insert(
                        key,
                        serde_json::Value::Array(
                            items.into_iter().map(serde_json::Value::String).collect(),
                        ),
                    );
                }
                current_key = None;
                continue;
            }

            let unquoted = value_str.trim_matches(|c| c == '\'' || c == '"').to_string();

            match key.as_str() {
                "title" => fm.title = unquoted,
                "author" => fm.author = unquoted,
                "last_update" => fm.last_update = unquoted,
                "status" => fm.status = Some(unquoted),
                "tags" => {
                    if !unquoted.is_empty() {
                        fm.tags = vec![unquoted];
                    }
                }
                _ => {
                    fm.custom.insert(key, serde_json::Value::String(unquoted));
                }
            }
        }
    }

    // Final list flush
    if let Some(ref k) = current_key {
        if !current_list.is_empty() {
            if k == "tags" {
                fm.tags.extend(current_list.drain(..));
            } else {
                fm.custom.insert(
                    k.clone(),
                    serde_json::Value::Array(
                        current_list.drain(..).map(serde_json::Value::String).collect(),
                    ),
                );
            }
        }
    }

    Ok((fm, body))
}

/// Validate frontmatter schema compliance.
pub fn validate_frontmatter(
    fm: &Frontmatter,
    file_stem: &str,
    is_template: bool,
) -> (Vec<String>, Vec<String>) {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    if is_template {
        // Relaxed checks for templates containing placeholders
        if fm.title.is_empty() {
            errors.push("'title' in template must be present".to_string());
        }
        return (errors, warnings);
    }

    // 1. Title validation
    if fm.title.is_empty() {
        errors.push("Missing required frontmatter field: 'title'".to_string());
    } else if fm.title != file_stem {
        if normalize_string(&fm.title) != normalize_string(file_stem) {
            errors.push(format!(
                "'title' (\"{}\") does not match filename (\"{}\")",
                fm.title, file_stem
            ));
        } else {
            warnings.push(format!(
                "'title' (\"{}\") differs in casing/spacing from filename (\"{}\")",
                fm.title, file_stem
            ));
        }
    }

    // 2. Tags validation
    if fm.tags.is_empty() {
        errors.push("Missing required frontmatter field: 'tags'".to_string());
    } else {
        let has_liva_tag = fm.tags.iter().any(|t| t.starts_with("liva/"));
        if !has_liva_tag {
            errors.push("At least one tag must start with 'liva/' (e.g., 'liva/knowledge')".to_string());
        }
    }

    // 3. Author validation
    if fm.author.trim().is_empty() {
        errors.push("Missing required frontmatter field: 'author'".to_string());
    }

    // 4. Last Update ISO 8601 validation
    let iso8601_regex = Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$").unwrap();
    if fm.last_update.trim().is_empty() {
        errors.push("Missing required frontmatter field: 'last_update'".to_string());
    } else if !iso8601_regex.is_match(&fm.last_update) {
        errors.push(format!(
            "'last_update' (\"{}\") must be a valid ISO 8601 datetime string",
            fm.last_update
        ));
    }

    (errors, warnings)
}

/// Native [[wikilinks]] parser extracting targets, sections, aliases, and line positions.
pub fn extract_wikilinks(content: &str) -> Vec<WikiLink> {
    let mut links = Vec::new();
    let link_regex = Regex::new(r"\[\[([^\]|#]+)(?:#([^\]|]*))?(?:\|([^\]]*))?\]\]").unwrap();

    for (line_idx, line) in content.lines().enumerate() {
        for cap in link_regex.captures_iter(line) {
            let target = cap.get(1).map(|m| m.as_str().trim().to_string()).unwrap_or_default();
            let section = cap.get(2).map(|m| m.as_str().trim().to_string()).filter(|s| !s.is_empty());
            let alias = cap.get(3).map(|m| m.as_str().trim().to_string()).filter(|s| !s.is_empty());
            let raw = cap.get(0).map(|m| m.as_str().to_string()).unwrap_or_default();

            if !target.is_empty() {
                links.push(WikiLink {
                    target,
                    section,
                    alias,
                    line: line_idx + 1,
                    raw,
                });
            }
        }
    }

    links
}

/// Obsidian Vault Synchronizer & L3 Semantic Memory Engine.
pub struct ObsidianVaultSync {
    pub vault_root: PathBuf,
    pub notes: HashMap<String, VaultNote>,
    pub pool: Option<DatabasePool>,
}

impl ObsidianVaultSync {
    /// Initialize a new ObsidianVaultSync engine.
    pub fn new(vault_root: PathBuf, pool: Option<DatabasePool>) -> Result<Self, String> {
        if !vault_root.exists() {
            return Err(format!("Vault root directory does not exist: {:?}", vault_root));
        }

        Ok(Self {
            vault_root,
            notes: HashMap::new(),
            pool,
        })
    }

    /// Perform a full scan and validation of the Obsidian Vault.
    pub fn scan_vault(&mut self) -> Result<VaultScanReport, String> {
        let mut report = VaultScanReport::default();

        // 1. Verify required root directories exist
        let required_dirs = ["Skills", "Knowledge", "Rules", "Templates"];
        for dir in &required_dirs {
            let p = self.vault_root.join(dir);
            if !p.exists() || !p.is_dir() {
                report.errors.push(format!("Required directory missing: '{dir}' in vault root"));
            }
        }

        // 2. Verify required template files
        let required_templates = [
            "Skill Template.md",
            "Knowledge Template.md",
            "Rule Template.md",
        ];
        for tpl in &required_templates {
            let p = self.vault_root.join("Templates").join(tpl);
            if !p.exists() || !p.is_file() {
                report.errors.push(format!("Required template file missing: 'Templates/{tpl}'"));
            }
        }

        // 3. Scan all markdown files in the vault
        let mut md_paths = Vec::new();
        self.collect_markdown_files(&self.vault_root, &mut md_paths)?;

        let mut valid_targets: HashSet<String> = HashSet::new();
        let mut parsed_notes = Vec::new();

        for path in &md_paths {
            let rel = path
                .strip_prefix(&self.vault_root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            let file_stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();

            valid_targets.insert(rel.to_lowercase());
            if rel.ends_with(".md") {
                valid_targets.insert(rel[..rel.len() - 3].to_lowercase());
            }
            valid_targets.insert(file_stem.to_lowercase());
            valid_targets.insert(normalize_string(&file_stem));
        }

        // 4. Parse notes, validate frontmatter, and extract wikilinks
        for path in md_paths {
            report.total_files_checked += 1;
            let rel = path
                .strip_prefix(&self.vault_root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let file_stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
            let is_template = rel.starts_with("Templates/");

            let content = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    report.invalid_files_count += 1;
                    report.file_results.push(FileValidationResult {
                        file_path: path.to_string_lossy().to_string(),
                        relative_path: rel,
                        is_valid: false,
                        errors: vec![format!("Failed to read file: {e}")],
                        warnings: Vec::new(),
                        is_template,
                    });
                    continue;
                }
            };

            let mtime_ms = fs::metadata(&path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);

            let (fm, body) = match parse_frontmatter(&content) {
                Ok(res) => res,
                Err(e) => {
                    report.invalid_files_count += 1;
                    report.file_results.push(FileValidationResult {
                        file_path: path.to_string_lossy().to_string(),
                        relative_path: rel,
                        is_valid: false,
                        errors: vec![format!("YAML frontmatter error: {e}")],
                        warnings: Vec::new(),
                        is_template,
                    });
                    continue;
                }
            };

            let (fm_errors, fm_warnings) = validate_frontmatter(&fm, &file_stem, is_template);
            let links = if !is_template {
                extract_wikilinks(body)
            } else {
                Vec::new()
            };

            let is_valid = fm_errors.is_empty();
            if !is_valid {
                report.invalid_files_count += 1;
            }

            let note = VaultNote {
                relative_path: rel.clone(),
                absolute_path: path.clone(),
                title: if !fm.title.is_empty() { fm.title.clone() } else { file_stem },
                frontmatter: fm,
                body: body.to_string(),
                links,
                is_template,
                last_modified_ms: mtime_ms,
            };

            report.file_results.push(FileValidationResult {
                file_path: path.to_string_lossy().to_string(),
                relative_path: rel,
                is_valid,
                errors: fm_errors,
                warnings: fm_warnings,
                is_template,
            });

            parsed_notes.push(note);
        }

        // 5. Verify Internal Wikilinks and detect broken links
        for note in &parsed_notes {
            if note.is_template {
                continue;
            }
            for link in &note.links {
                let target_norm = link.target.to_lowercase();
                let loose_target = normalize_string(&link.target);
                if !valid_targets_contains(&valid_targets, &target_norm, &loose_target) {
                    report.broken_links_count += 1;
                    if let Some(res) = report
                        .file_results
                        .iter_mut()
                        .find(|r| r.relative_path == note.relative_path)
                    {
                        res.is_valid = false;
                        res.errors.push(format!(
                            "Broken wiki link found: '[[{}]]' at line {}",
                            link.target, link.line
                        ));
                    }
                }
            }
        }

        report.invalid_files_count = report.file_results.iter().filter(|r| !r.is_valid).count();

        // Populate internal notes index
        self.notes.clear();
        for note in parsed_notes {
            self.notes.insert(note.relative_path.clone(), note);
        }

        Ok(report)
    }

    /// Retrieve a note by its title.
    pub fn get_note_by_title(&self, title: &str) -> Option<&VaultNote> {
        self.notes.values().find(|n| n.title == title || normalize_string(&n.title) == normalize_string(title))
    }

    fn collect_markdown_files(&self, dir: &Path, acc: &mut Vec<PathBuf>) -> Result<(), String> {
        if !dir.exists() {
            return Ok(());
        }
        let entries = fs::read_dir(dir).map_err(|e| e.to_string())?;
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                self.collect_markdown_files(&path, acc)?;
            } else if path.is_file() && path.extension().map_or(false, |ext| ext == "md") {
                acc.push(path);
            }
        }
        Ok(())
    }

    /// Read note content safely through path containment verification.
    pub fn read_note(&self, relative_path: &str) -> Result<String, String> {
        let safe_path = validate_and_resolve_path(&self.vault_root, relative_path)
            .map_err(|e| e.to_string())?;
        fs::read_to_string(&safe_path).map_err(|e| e.to_string())
    }

    /// Write or update a note safely and update internal index.
    pub fn write_note(&mut self, relative_path: &str, content: &str) -> Result<VaultNote, String> {
        let safe_path = validate_and_resolve_path(&self.vault_root, relative_path)
            .map_err(|e| e.to_string())?;

        if let Some(parent) = safe_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        fs::write(&safe_path, content).map_err(|e| e.to_string())?;

        let (fm, body) = parse_frontmatter(content)?;
        let links = extract_wikilinks(body);
        let file_stem = safe_path.file_stem().unwrap_or_default().to_string_lossy().to_string();
        let rel = safe_path
            .strip_prefix(&self.vault_root)
            .unwrap_or(&safe_path)
            .to_string_lossy()
            .replace('\\', "/");

        let mtime_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let note = VaultNote {
            relative_path: rel.clone(),
            absolute_path: safe_path,
            title: if !fm.title.is_empty() { fm.title.clone() } else { file_stem },
            frontmatter: fm,
            body: body.to_string(),
            links,
            is_template: rel.starts_with("Templates/"),
            last_modified_ms: mtime_ms,
        };

        self.notes.insert(rel, note.clone());

        Ok(note)
    }

    /// Delete a note safely and remove from index.
    pub fn delete_note(&mut self, relative_path: &str) -> Result<bool, String> {
        let safe_path = validate_and_resolve_path(&self.vault_root, relative_path)
            .map_err(|e| e.to_string())?;

        let rel = safe_path
            .strip_prefix(&self.vault_root)
            .unwrap_or(&safe_path)
            .to_string_lossy()
            .replace('\\', "/");

        if safe_path.exists() {
            fs::remove_file(&safe_path).map_err(|e| e.to_string())?;
            self.notes.remove(&rel);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Sync in-memory vault notes and wikilinks to SQLite l3_nodes and l3_edges tables.
    pub fn sync_to_db(&self) -> Result<(usize, usize), String> {
        let pool = match &self.pool {
            Some(p) => p,
            None => return Err("Database pool not configured".to_string()),
        };

        let mut conn = pool.writer.get().map_err(|e| e.to_string())?;

        let mut nodes_synced = 0;
        let mut edges_synced = 0;

        let tx = conn.transaction().map_err(|e| e.to_string())?;

        // 1. Insert/Update l3_nodes
        {
            let mut stmt = tx
                .prepare_cached(
                    "INSERT INTO l3_nodes (id, label, properties)
                     VALUES (?1, ?2, ?3)
                     ON CONFLICT(id) DO UPDATE SET
                        label = excluded.label,
                        properties = excluded.properties",
                )
                .map_err(|e| e.to_string())?;

            for note in self.notes.values() {
                if note.is_template {
                    continue;
                }
                let props = serde_json::json!({
                    "relative_path": note.relative_path,
                    "tags": note.frontmatter.tags,
                    "author": note.frontmatter.author,
                    "last_update": note.frontmatter.last_update,
                    "status": note.frontmatter.status,
                });

                stmt.execute(rusqlite::params![
                    note.title,
                    note.title,
                    props.to_string(),
                ])
                .map_err(|e| e.to_string())?;
                nodes_synced += 1;
            }
        }

        // 2. Insert/Update l3_edges from [[wikilinks]]
        {
            let mut stmt_stub = tx
                .prepare_cached(
                    "INSERT OR IGNORE INTO l3_nodes (id, label, properties)
                     VALUES (?1, ?2, '{}')",
                )
                .map_err(|e| e.to_string())?;

            let mut stmt = tx
                .prepare_cached(
                    "INSERT INTO l3_edges (source, target, relation, weight, obsolete)
                     VALUES (?1, ?2, ?3, ?4, 0)
                     ON CONFLICT(source, target, relation) DO UPDATE SET
                        weight = excluded.weight,
                        obsolete = 0",
                )
                .map_err(|e| e.to_string())?;

            for note in self.notes.values() {
                if note.is_template {
                    continue;
                }
                for link in &note.links {
                    let relation = link
                        .section
                        .as_deref()
                        .unwrap_or("references");

                    let target_title = self.get_note_by_title(&link.target)
                        .map(|n| n.title.as_str())
                        .unwrap_or(&link.target);

                    stmt_stub.execute(rusqlite::params![target_title, target_title])
                        .map_err(|e| e.to_string())?;

                    stmt.execute(rusqlite::params![
                        note.title,
                        target_title,
                        relation,
                        1.0f64,
                    ])
                    .map_err(|e| e.to_string())?;
                    edges_synced += 1;
                }
            }
        }

        tx.commit().map_err(|e| e.to_string())?;

        Ok((nodes_synced, edges_synced))
    }

    /// Construct a CsrGraph directly from indexed Obsidian Vault notes and wikilinks.
    pub fn build_csr_graph(&self, bidirectional: bool) -> CsrGraph {
        let mut edges = Vec::new();

        for note in self.notes.values() {
            if note.is_template {
                continue;
            }
            for link in &note.links {
                edges.push((
                    note.title.as_str(),
                    link.target.as_str(),
                    1.0f32,
                ));
            }
        }

        CsrGraph::from_named_edges(&edges, bidirectional)
    }
}

fn valid_targets_contains(set: &HashSet<String>, target_norm: &str, loose: &str) -> bool {
    set.contains(target_norm) || set.contains(loose)
}
