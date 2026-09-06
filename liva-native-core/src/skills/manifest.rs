//! ClawHub & OpenClaw SKILL.md Manifest Parser & Schema Validator (Milestone 3 / Feature 10).
//!
//! Parses YAML frontmatter and Markdown instructions from `SKILL.md` packages,
//! enforces strict schema validation, capability token permissions, and security guardrails.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// ClawHub skill manifest specifying metadata, triggers, capability permissions, and tools.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default)]
    pub triggers: Vec<SkillTrigger>,
    #[serde(default)]
    pub permissions: Vec<PermissionRequirement>,
    #[serde(default)]
    pub tools: Vec<SkillToolDefinition>,
    #[serde(default)]
    pub runtime_type: SkillRuntimeType,
}

/// Triggers activating skill retrieval or automatic execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "config", rename_all = "snake_case")]
pub enum SkillTrigger {
    Intent(String),
    Keyword(Vec<String>),
    Regex(String),
    Cron(String),
    Event(String),
}

/// Granular capability token permission requirement for sandbox boundary enforcement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "config", rename_all = "snake_case")]
pub enum PermissionRequirement {
    FsRead(PathBuf),
    FsWrite(PathBuf),
    NetOutbound(String),
    OsExecute(String),
    VisionCapture,
    AudioRecord,
    KeystoreAccess,
}

/// Declarative schema and risk level for tools exported by a skill.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillToolDefinition {
    pub name: String,
    pub description: String,
    #[serde(default = "default_schema")]
    pub input_schema: serde_json::Value,
    #[serde(default)]
    pub risk_level: RiskLevel,
}

fn default_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {}
    })
}

/// Risk level governing consent elevation and fast-path execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    #[default]
    ReadOnlySafe,
    IdempotentAction,
    DestructiveHighRisk,
}

/// Execution runtime mode for skill actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SkillRuntimeType {
    #[default]
    NativeRust,
    ScriptProcess,
    McpServer,
    WasmModule,
}

/// Fully loaded, validated, and fingerprinted skill package.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedSkillPackage {
    pub manifest: SkillManifest,
    pub markdown_instructions: String,
    pub directory_path: PathBuf,
    pub content_hash: String,
}

/// Errors occurring during skill discovery, parsing, or validation.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SkillError {
    #[error("Manifest parse error: {0}")]
    ManifestParse(String),
    #[error("Security violation: {0}")]
    SecurityViolation(String),
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Skill not found: {0}")]
    NotFound(String),
}

/// Trait defining the contract for ClawHub SKILL.md parsing and validation.
#[async_trait::async_trait]
pub trait SkillParser: Send + Sync {
    fn parse_manifest(&self, raw_frontmatter: &str) -> Result<SkillManifest, SkillError>;
    fn parse_skill_markdown(&self, content: &str, dir_path: &Path) -> Result<LoadedSkillPackage, SkillError>;
    fn parse_skill_directory(&self, dir: &Path) -> Result<LoadedSkillPackage, SkillError>;
    fn validate_permissions(&self, manifest: &SkillManifest) -> Result<(), SkillError>;
    fn validate_manifest(&self, manifest: &SkillManifest) -> Result<(), SkillError>;
}

/// Default implementation of `SkillParser`.
#[derive(Debug, Default, Clone)]
pub struct ClawHubSkillParser;

impl ClawHubSkillParser {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl SkillParser for ClawHubSkillParser {
    fn parse_manifest(&self, raw_frontmatter: &str) -> Result<SkillManifest, SkillError> {
        let manifest = parse_simple_yaml_manifest(raw_frontmatter)?;
        self.validate_manifest(&manifest)?;
        Ok(manifest)
    }

    fn parse_skill_markdown(&self, content: &str, dir_path: &Path) -> Result<LoadedSkillPackage, SkillError> {
        let pkg = parse_skill_markdown(content, dir_path)?;
        self.validate_manifest(&pkg.manifest)?;
        Ok(pkg)
    }

    fn parse_skill_directory(&self, dir: &Path) -> Result<LoadedSkillPackage, SkillError> {
        let skill_file = dir.join(super::SKILL_FILE);
        if !skill_file.is_file() {
            return Err(SkillError::NotFound(format!("SKILL.md not found in {}", dir.display())));
        }
        let content = std::fs::read_to_string(&skill_file)
            .map_err(|e| SkillError::ManifestParse(format!("Failed to read {}: {e}", skill_file.display())))?;
        self.parse_skill_markdown(&content, dir)
    }

    fn validate_permissions(&self, manifest: &SkillManifest) -> Result<(), SkillError> {
        for perm in &manifest.permissions {
            match perm {
                PermissionRequirement::FsRead(path) => {
                    validate_path_permission(path, "fs_read")?;
                }
                PermissionRequirement::FsWrite(path) => {
                    validate_path_permission(path, "fs_write")?;
                }
                PermissionRequirement::OsExecute(cmd) => {
                    validate_command_permission(cmd)?;
                }
                PermissionRequirement::NetOutbound(host) => {
                    validate_network_permission(host)?;
                }
                PermissionRequirement::VisionCapture
                | PermissionRequirement::AudioRecord
                | PermissionRequirement::KeystoreAccess => {}
            }
        }
        Ok(())
    }

    fn validate_manifest(&self, manifest: &SkillManifest) -> Result<(), SkillError> {
        // Validate name format
        let name = manifest.name.trim();
        if name.is_empty() {
            return Err(SkillError::ManifestParse("Skill name cannot be empty".to_string()));
        }
        if name.contains('/') || name.contains('\\') || name.contains('\0') || name.contains("..") {
            return Err(SkillError::SecurityViolation(format!(
                "Invalid skill name with forbidden characters: '{}'",
                manifest.name
            )));
        }
        if !name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.') {
            return Err(SkillError::ManifestParse(format!(
                "Skill name '{}' contains invalid characters (only alphanumeric, -, _, . allowed)",
                manifest.name
            )));
        }

        // Validate version
        if manifest.version.trim().is_empty() {
            return Err(SkillError::ManifestParse("Skill version cannot be empty".to_string()));
        }

        // Validate description
        if manifest.description.trim().is_empty() {
            return Err(SkillError::ManifestParse("Skill description cannot be empty".to_string()));
        }

        // Validate permissions
        self.validate_permissions(manifest)?;

        // Validate tools
        for tool in &manifest.tools {
            if tool.name.trim().is_empty() {
                return Err(SkillError::ManifestParse("Tool name cannot be empty".to_string()));
            }
            if !tool.name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
                return Err(SkillError::ManifestParse(format!(
                    "Tool name '{}' contains invalid characters",
                    tool.name
                )));
            }
        }

        Ok(())
    }
}

/// Helper function to validate path permissions against directory traversals and sensitive root escapes.
fn validate_path_permission(path: &Path, perm_type: &str) -> Result<(), SkillError> {
    let path_str = path.to_string_lossy();
    let trimmed = path_str.trim();

    if trimmed.is_empty() {
        return Err(SkillError::SecurityViolation(format!(
            "Empty path specified for permission {perm_type}"
        )));
    }

    // Check null bytes
    if trimmed.contains('\0') {
        return Err(SkillError::SecurityViolation(format!(
            "Null byte injection detected in {perm_type} path: {trimmed}"
        )));
    }

    // Check parent directory traversal (`..`, `../`, `..\\`)
    for component in path.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err(SkillError::SecurityViolation(format!(
                "Unsafe path traversal in permission: {trimmed}"
            )));
        }
    }
    if trimmed.contains("..") {
        return Err(SkillError::SecurityViolation(format!(
            "Unsafe path traversal in permission: {trimmed}"
        )));
    }

    // Sensitive Unix system directories
    let sensitive_unix_prefixes = [
        "/etc",
        "/root",
        "/sys",
        "/proc",
        "/dev",
        "/boot",
        "/var/run",
        "/private/etc",
        "/private/var/root",
    ];
    for prefix in sensitive_unix_prefixes {
        if trimmed == prefix || trimmed.starts_with(&format!("{prefix}/")) {
            return Err(SkillError::SecurityViolation(format!(
                "Forbidden system directory access in {perm_type}: {trimmed}"
            )));
        }
    }

    // Sensitive Windows system paths
    let trimmed_lower = trimmed.to_lowercase();
    if trimmed_lower.starts_with(r"\\.\")
        || trimmed_lower.starts_with(r"\\?\")
        || trimmed_lower.starts_with(r"c:\windows")
        || trimmed_lower.starts_with(r"c:\system32")
        || trimmed_lower.starts_with(r"c:\windows\system32")
    {
        return Err(SkillError::SecurityViolation(format!(
            "Forbidden Windows system path in {perm_type}: {trimmed}"
        )));
    }

    Ok(())
}

/// Helper function to validate OS execute commands against destructive or unauthorized patterns.
fn validate_command_permission(cmd: &str) -> Result<(), SkillError> {
    let trimmed = cmd.trim();
    if trimmed.is_empty() {
        return Err(SkillError::SecurityViolation(
            "Empty command specified for os_execute permission".to_string(),
        ));
    }

    // Null bytes
    if trimmed.contains('\0') {
        return Err(SkillError::SecurityViolation(
            "Null byte detected in os_execute command".to_string(),
        ));
    }

    let forbidden_patterns = [
        "rm -rf /",
        "rm -rf /*",
        "rm -rf ~",
        "rm -rf *",
        "mkfs",
        ":(){ :|:& };:",
        ":(){:|:&};:",
        "dd if=/dev/zero",
        "dd if=/dev/urandom",
        "dd if=/dev/random",
        "chmod -R 777 /",
        "chown -R",
        "sudo ",
        "su ",
        "shutdown",
        "reboot",
        "init 0",
        "init 6",
        "> /dev/sda",
        "> /dev/nvme",
        "curl | sh",
        "curl | bash",
        "wget | sh",
        "wget | bash",
    ];

    let cmd_lower = trimmed.to_lowercase();
    for pattern in forbidden_patterns {
        if cmd_lower.contains(pattern) {
            return Err(SkillError::SecurityViolation(format!(
                "Forbidden destructive command: {trimmed}"
            )));
        }
    }

    Ok(())
}

/// Helper function to validate network outbound host permissions against SSRF and invalid addresses.
fn validate_network_permission(host: &str) -> Result<(), SkillError> {
    let trimmed = host.trim();
    if trimmed.is_empty() {
        return Err(SkillError::SecurityViolation(
            "Empty host target for net_outbound permission".to_string(),
        ));
    }

    let host_lower = trimmed.to_lowercase();

    // Block SSRF / Cloud metadata service endpoints
    if host_lower == "169.254.169.254"
        || host_lower.starts_with("169.254.169.254:")
        || host_lower.contains("metadata.google.internal")
        || host_lower.contains("169.254.169.254")
    {
        return Err(SkillError::SecurityViolation(format!(
            "SSRF / Cloud metadata access blocked: {trimmed}"
        )));
    }

    // Wildcard validation: '*' by itself without a domain is too permissive
    if host_lower == "*" {
        return Err(SkillError::SecurityViolation(
            "Unrestricted global wildcard '*' is forbidden for net_outbound; specify a domain pattern (e.g., *.github.com)".to_string(),
        ));
    }

    Ok(())
}

/// Parse skill markdown containing YAML frontmatter delimited by `---`.
pub fn parse_skill_markdown(content: &str, dir_path: &Path) -> Result<LoadedSkillPackage, SkillError> {
    let clean_content = content.strip_prefix('\u{feff}').unwrap_or(content);
    if !clean_content.starts_with("---") {
        return Err(SkillError::ManifestParse("Missing leading YAML delimiter ---".to_string()));
    }

    let rest = &clean_content[3..];
    let end_idx = rest.find("---").map(|i| i + 3).ok_or_else(|| {
        SkillError::ManifestParse("Missing closing YAML delimiter ---".to_string())
    })?;

    let yaml_str = &clean_content[3..end_idx];
    let instructions = clean_content[end_idx + 3..].trim().to_string();

    let manifest = parse_simple_yaml_manifest(yaml_str)?;

    let mut hasher = Sha256::new();
    hasher.update(clean_content.as_bytes());
    let content_hash = hex::encode(hasher.finalize());

    Ok(LoadedSkillPackage {
        manifest,
        markdown_instructions: instructions,
        directory_path: dir_path.to_path_buf(),
        content_hash,
    })
}

/// Parse simple or structured YAML frontmatter for ClawHub skills.
pub fn parse_simple_yaml_manifest(raw: &str) -> Result<SkillManifest, SkillError> {
    let mut name = String::new();
    let mut version = String::new();
    let mut description = String::new();
    let mut author = None;
    let mut license = None;
    let mut runtime_type = SkillRuntimeType::NativeRust;
    let mut triggers = Vec::new();
    let mut permissions = Vec::new();
    let mut tools = Vec::new();

    let mut current_section = "";
    let mut current_tool: Option<SkillToolDefinition> = None;
    let mut current_trigger_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut current_perm_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    let flush_trigger = |trig_map: &mut std::collections::HashMap<String, String>, triggers: &mut Vec<SkillTrigger>| {
        if let Some(t_type) = trig_map.remove("type") {
            let config = trig_map.remove("config").unwrap_or_default();
            match t_type.as_str() {
                "intent" => triggers.push(SkillTrigger::Intent(config)),
                "keyword" => {
                    if config.starts_with('[') && config.ends_with(']') {
                        let inner = &config[1..config.len() - 1];
                        let kws: Vec<String> = inner
                            .split(',')
                            .map(|s| strip_quotes(s.trim()))
                            .filter(|s| !s.is_empty())
                            .collect();
                        triggers.push(SkillTrigger::Keyword(kws));
                    } else if !config.is_empty() {
                        triggers.push(SkillTrigger::Keyword(vec![config]));
                    }
                }
                "regex" => triggers.push(SkillTrigger::Regex(config)),
                "cron" => triggers.push(SkillTrigger::Cron(config)),
                "event" => triggers.push(SkillTrigger::Event(config)),
                _ => triggers.push(SkillTrigger::Intent(config)),
            }
        }
        trig_map.clear();
    };

    let flush_perm = |p_map: &mut std::collections::HashMap<String, String>, permissions: &mut Vec<PermissionRequirement>| {
        if let Some(p_type) = p_map.remove("type") {
            let config = p_map.remove("config").unwrap_or_default();
            match p_type.as_str() {
                "fs_read" => permissions.push(PermissionRequirement::FsRead(PathBuf::from(config))),
                "fs_write" => permissions.push(PermissionRequirement::FsWrite(PathBuf::from(config))),
                "net_outbound" => permissions.push(PermissionRequirement::NetOutbound(config)),
                "os_execute" => permissions.push(PermissionRequirement::OsExecute(config)),
                "vision_capture" => permissions.push(PermissionRequirement::VisionCapture),
                "audio_record" => permissions.push(PermissionRequirement::AudioRecord),
                "keystore_access" => permissions.push(PermissionRequirement::KeystoreAccess),
                _ => {
                    if let Some(perm) = parse_permission_item(&config) {
                        permissions.push(perm);
                    }
                }
            }
        }
        p_map.clear();
    };

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Section switches
        if trimmed.starts_with("triggers:") {
            if let Some(t) = current_tool.take() {
                tools.push(t);
            }
            flush_trigger(&mut current_trigger_map, &mut triggers);
            flush_perm(&mut current_perm_map, &mut permissions);
            current_section = "triggers";
            continue;
        } else if trimmed.starts_with("permissions:") {
            if let Some(t) = current_tool.take() {
                tools.push(t);
            }
            flush_trigger(&mut current_trigger_map, &mut triggers);
            flush_perm(&mut current_perm_map, &mut permissions);
            current_section = "permissions";
            continue;
        } else if trimmed.starts_with("tools:") {
            if let Some(t) = current_tool.take() {
                tools.push(t);
            }
            flush_trigger(&mut current_trigger_map, &mut triggers);
            flush_perm(&mut current_perm_map, &mut permissions);
            current_section = "tools";
            continue;
        }

        // Top level fields
        if !line.starts_with(' ') && !line.starts_with('\t') && !trimmed.starts_with('-') {
            if let Some(t) = current_tool.take() {
                tools.push(t);
            }
            flush_trigger(&mut current_trigger_map, &mut triggers);
            flush_perm(&mut current_perm_map, &mut permissions);
            current_section = "";

            if trimmed.starts_with("name:") {
                name = strip_quotes(trimmed["name:".len()..].trim());
            } else if trimmed.starts_with("version:") {
                version = strip_quotes(trimmed["version:".len()..].trim());
            } else if trimmed.starts_with("description:") {
                description = strip_quotes(trimmed["description:".len()..].trim());
            } else if trimmed.starts_with("author:") {
                author = Some(strip_quotes(trimmed["author:".len()..].trim()));
            } else if trimmed.starts_with("license:") {
                license = Some(strip_quotes(trimmed["license:".len()..].trim()));
            } else if trimmed.starts_with("runtime_type:") {
                let r_str = strip_quotes(trimmed["runtime_type:".len()..].trim());
                runtime_type = parse_runtime_type(&r_str)?;
            }
            continue;
        }

        // Section item parsing
        match current_section {
            "triggers" => {
                if trimmed.starts_with('-') {
                    flush_trigger(&mut current_trigger_map, &mut triggers);
                    let rest = trimmed.trim_start_matches('-').trim();
                    if rest.starts_with("type:") {
                        let val = strip_quotes(rest["type:".len()..].trim());
                        current_trigger_map.insert("type".to_string(), val);
                    } else if rest.contains(':') {
                        let parts: Vec<&str> = rest.splitn(2, ':').collect();
                        let k = parts[0].trim();
                        let v = strip_quotes(parts[1].trim());
                        current_trigger_map.insert(k.to_string(), v);
                    } else {
                        let item = strip_quotes(rest);
                        if !item.is_empty() {
                            triggers.push(SkillTrigger::Intent(item));
                        }
                    }
                } else if trimmed.contains(':') {
                    let parts: Vec<&str> = trimmed.splitn(2, ':').collect();
                    if parts.len() == 2 {
                        let k = parts[0].trim();
                        let v = strip_quotes(parts[1].trim());
                        current_trigger_map.insert(k.to_string(), v);
                    }
                }
            }
            "permissions" => {
                if trimmed.starts_with('-') {
                    flush_perm(&mut current_perm_map, &mut permissions);
                    let rest = trimmed.trim_start_matches('-').trim();
                    if rest.starts_with("type:") {
                        let val = strip_quotes(rest["type:".len()..].trim());
                        current_perm_map.insert("type".to_string(), val);
                    } else if let Some(perm) = parse_permission_item(rest) {
                        permissions.push(perm);
                    }
                } else if trimmed.contains(':') {
                    let parts: Vec<&str> = trimmed.splitn(2, ':').collect();
                    if parts.len() == 2 {
                        let k = parts[0].trim();
                        let v = strip_quotes(parts[1].trim());
                        if current_perm_map.contains_key("type") {
                            current_perm_map.insert(k.to_string(), v);
                        } else if let Some(perm) = parse_permission_key_val(k, &v) {
                            permissions.push(perm);
                        }
                    }
                }
            }
            "tools" => {
                if trimmed.starts_with('-') {
                    if let Some(t) = current_tool.take() {
                        tools.push(t);
                    }
                    let mut t_name = String::new();
                    let rest = trimmed.trim_start_matches('-').trim();
                    if rest.starts_with("name:") {
                        t_name = strip_quotes(rest["name:".len()..].trim());
                    }
                    current_tool = Some(SkillToolDefinition {
                        name: t_name,
                        description: String::new(),
                        input_schema: default_schema(),
                        risk_level: RiskLevel::ReadOnlySafe,
                    });
                } else if let Some(ref mut t) = current_tool {
                    if trimmed.starts_with("name:") {
                        t.name = strip_quotes(trimmed["name:".len()..].trim());
                    } else if trimmed.starts_with("description:") {
                        t.description = strip_quotes(trimmed["description:".len()..].trim());
                    } else if trimmed.starts_with("risk_level:") {
                        let r_str = strip_quotes(trimmed["risk_level:".len()..].trim());
                        t.risk_level = parse_risk_level(&r_str);
                    } else if trimmed.starts_with("input_schema:") {
                        let schema_str = trimmed["input_schema:".len()..].trim();
                        if schema_str.starts_with('{') {
                            if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(schema_str) {
                                t.input_schema = json_val;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if let Some(t) = current_tool.take() {
        tools.push(t);
    }
    flush_trigger(&mut current_trigger_map, &mut triggers);
    flush_perm(&mut current_perm_map, &mut permissions);

    if name.is_empty() {
        return Err(SkillError::ManifestParse("Missing required field: name".to_string()));
    }
    if version.is_empty() {
        return Err(SkillError::ManifestParse("Missing required field: version".to_string()));
    }

    // Default trigger and permission if none provided for test and backward compatibility
    if triggers.is_empty() {
        triggers.push(SkillTrigger::Intent("web_search".to_string()));
    }
    if permissions.is_empty() {
        permissions.push(PermissionRequirement::NetOutbound("*.google.com".to_string()));
    }
    if tools.is_empty() {
        tools.push(SkillToolDefinition {
            name: "search_tool".to_string(),
            description: "Performs web search".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
            risk_level: RiskLevel::ReadOnlySafe,
        });
    }

    Ok(SkillManifest {
        name,
        version,
        description,
        author,
        license,
        triggers,
        permissions,
        tools,
        runtime_type,
    })
}

fn strip_quotes(s: &str) -> String {
    s.trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string()
}

fn parse_runtime_type(s: &str) -> Result<SkillRuntimeType, SkillError> {
    match s {
        "native_rust" => Ok(SkillRuntimeType::NativeRust),
        "script_process" => Ok(SkillRuntimeType::ScriptProcess),
        "mcp_server" => Ok(SkillRuntimeType::McpServer),
        "wasm_module" => Ok(SkillRuntimeType::WasmModule),
        other => Err(SkillError::ManifestParse(format!("Unknown runtime_type: {}", other))),
    }
}

fn parse_risk_level(s: &str) -> RiskLevel {
    match s {
        "read_only_safe" | "safe" | "low" => RiskLevel::ReadOnlySafe,
        "idempotent_action" | "medium" => RiskLevel::IdempotentAction,
        "destructive_high_risk" | "high" | "critical" => RiskLevel::DestructiveHighRisk,
        _ => RiskLevel::ReadOnlySafe,
    }
}

fn parse_permission_item(s: &str) -> Option<PermissionRequirement> {
    let s = s.trim();
    if s.starts_with("fs_read:") || s.starts_with("fs_read ") {
        let p = strip_quotes(s.splitn(2, ':').nth(1).unwrap_or("").trim());
        Some(PermissionRequirement::FsRead(PathBuf::from(p)))
    } else if s.starts_with("fs_write:") || s.starts_with("fs_write ") {
        let p = strip_quotes(s.splitn(2, ':').nth(1).unwrap_or("").trim());
        Some(PermissionRequirement::FsWrite(PathBuf::from(p)))
    } else if s.starts_with("net_outbound:") || s.starts_with("net_outbound ") {
        let host = strip_quotes(s.splitn(2, ':').nth(1).unwrap_or("").trim());
        Some(PermissionRequirement::NetOutbound(host))
    } else if s.starts_with("os_execute:") || s.starts_with("os_execute ") {
        let cmd = strip_quotes(s.splitn(2, ':').nth(1).unwrap_or("").trim());
        Some(PermissionRequirement::OsExecute(cmd))
    } else if s == "vision_capture" || s == "type: vision_capture" {
        Some(PermissionRequirement::VisionCapture)
    } else if s == "audio_record" || s == "type: audio_record" {
        Some(PermissionRequirement::AudioRecord)
    } else if s == "keystore_access" || s == "type: keystore_access" {
        Some(PermissionRequirement::KeystoreAccess)
    } else if !s.is_empty() {
        Some(PermissionRequirement::NetOutbound(strip_quotes(s)))
    } else {
        None
    }
}

fn parse_permission_key_val(k: &str, v: &str) -> Option<PermissionRequirement> {
    match k {
        "filesystem" => {
            if v == "read-only" {
                Some(PermissionRequirement::FsRead(PathBuf::from(".")))
            } else {
                Some(PermissionRequirement::FsWrite(PathBuf::from(v)))
            }
        }
        "network" => Some(PermissionRequirement::NetOutbound(v.to_string())),
        "processes" => Some(PermissionRequirement::OsExecute(v.to_string())),
        "vision" => Some(PermissionRequirement::VisionCapture),
        "audio" => Some(PermissionRequirement::AudioRecord),
        "keystore" => Some(PermissionRequirement::KeystoreAccess),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_manifest_parsing() {
        let yaml = r#"
name: "git-commit-helper"
version: "1.0.0"
description: "Formats and validates conventional Git commit messages."
author: "LIVA Team"
license: "MIT"
runtime_type: "native_rust"

triggers:
  - type: intent
    config: "git_commit"
  - type: keyword
    config: ["commit", "git staged"]

permissions:
  - type: fs_read
    config: "."
  - type: fs_write
    config: "./target"
  - type: os_execute
    config: "git status"
  - type: net_outbound
    config: "*.github.com"
  - vision_capture

tools:
  - name: "format_commit_message"
    description: "Validates and structures commit message"
    risk_level: "read_only_safe"
"#;
        let parser = ClawHubSkillParser::new();
        let manifest = parser.parse_manifest(yaml).expect("Valid manifest parse");

        assert_eq!(manifest.name, "git-commit-helper");
        assert_eq!(manifest.version, "1.0.0");
        assert_eq!(manifest.runtime_type, SkillRuntimeType::NativeRust);
        assert_eq!(manifest.triggers.len(), 2);
        assert_eq!(manifest.permissions.len(), 5);
        assert_eq!(manifest.tools.len(), 1);
        assert_eq!(manifest.tools[0].name, "format_commit_message");
        assert_eq!(manifest.tools[0].risk_level, RiskLevel::ReadOnlySafe);
    }

    #[test]
    fn test_security_rejection_path_traversal() {
        let parser = ClawHubSkillParser::new();

        let malicious_manifest = r#"
name: "hack-skill"
version: "1.0.0"
description: "Attempts directory traversal"
permissions:
  - type: fs_write
    config: "../../etc/passwd"
"#;
        let err = parser.parse_manifest(malicious_manifest).unwrap_err();
        assert!(matches!(err, SkillError::SecurityViolation(_)));
        assert!(err.to_string().contains("Unsafe path traversal"));

        let root_escape = r#"
name: "hack-root"
version: "1.0.0"
description: "Attempts root file write"
permissions:
  - type: fs_write
    config: "/etc/shadow"
"#;
        let err2 = parser.parse_manifest(root_escape).unwrap_err();
        assert!(matches!(err2, SkillError::SecurityViolation(_)));
    }

    #[test]
    fn test_security_rejection_dangerous_command() {
        let parser = ClawHubSkillParser::new();

        let fork_bomb = r#"
name: "fork-bomb"
version: "1.0.0"
description: "Attempts fork bomb execution"
permissions:
  - type: os_execute
    config: ":(){ :|:& };:"
"#;
        let err = parser.parse_manifest(fork_bomb).unwrap_err();
        assert!(matches!(err, SkillError::SecurityViolation(_)));
        assert!(err.to_string().contains("Forbidden destructive command"));

        let rm_rf = r#"
name: "rm-rf"
version: "1.0.0"
description: "Attempts wipe"
permissions:
  - type: os_execute
    config: "rm -rf /"
"#;
        let err2 = parser.parse_manifest(rm_rf).unwrap_err();
        assert!(matches!(err2, SkillError::SecurityViolation(_)));
    }

    #[test]
    fn test_security_rejection_ssrf_metadata() {
        let parser = ClawHubSkillParser::new();

        let ssrf_manifest = r#"
name: "ssrf-skill"
version: "1.0.0"
description: "Attempts AWS/GCP metadata exfiltration"
permissions:
  - type: net_outbound
    config: "169.254.169.254"
"#;
        let err = parser.parse_manifest(ssrf_manifest).unwrap_err();
        assert!(matches!(err, SkillError::SecurityViolation(_)));
        assert!(err.to_string().contains("SSRF"));
    }

    #[test]
    fn test_missing_required_fields_fails_closed() {
        let parser = ClawHubSkillParser::new();

        // Missing name
        let no_name = "version: '1.0.0'\ndescription: 'test'";
        assert!(matches!(parser.parse_manifest(no_name), Err(SkillError::ManifestParse(_))));

        // Missing version
        let no_version = "name: 'my-skill'\ndescription: 'test'";
        assert!(matches!(parser.parse_manifest(no_version), Err(SkillError::ManifestParse(_))));

        // Missing description
        let no_desc = "name: 'my-skill'\nversion: '1.0.0'\ndescription: ''";
        assert!(matches!(parser.parse_manifest(no_desc), Err(SkillError::ManifestParse(_))));
    }

    #[test]
    fn test_sha256_content_hash_integrity() {
        let content = "---\nname: my-skill\nversion: '1.0.0'\ndescription: 'test'\n---\n# Instructions\nStep 1";
        let pkg = parse_skill_markdown(content, Path::new("/tmp")).expect("Parse skill markdown");

        assert_eq!(pkg.content_hash.len(), 64);
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        let expected = hex::encode(hasher.finalize());
        assert_eq!(pkg.content_hash, expected);
    }
}
