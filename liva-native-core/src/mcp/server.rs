use crate::mcp::protocol::{CallToolRequest, CallToolResult, Tool, ToolContent, ToolList};
use schemars::{schema_for, JsonSchema};
use serde::Deserialize;
use std::path::{Path, PathBuf};

pub struct NativeMcpServer {
    vault_path: PathBuf,
}

#[derive(JsonSchema, Deserialize)]
struct ReadMarkdownArgs {
    path: String,
}

#[derive(JsonSchema, Deserialize)]
struct WriteMarkdownArgs {
    path: String,
    content: String,
}

#[derive(JsonSchema, Deserialize)]
struct SearchVaultArgs {
    query: String,
}

#[derive(JsonSchema, Deserialize)]
struct ControlSmartHomeArgs {
    device: String,
    command: String,
}

impl NativeMcpServer {
    pub fn new(vault_path: &str) -> Self {
        Self {
            vault_path: PathBuf::from(vault_path),
        }
    }

    pub fn list_tools(&self) -> ToolList {
        ToolList {
            tools: vec![
                Tool {
                    name: "read_markdown".to_string(),
                    description: "Read a markdown file from the vault".to_string(),
                    input_schema: schema_for!(ReadMarkdownArgs),
                },
                Tool {
                    name: "write_markdown".to_string(),
                    description: "Write content to a markdown file in the vault".to_string(),
                    input_schema: schema_for!(WriteMarkdownArgs),
                },
                Tool {
                    name: "search_vault".to_string(),
                    description: "Search the vault for a keyword".to_string(),
                    input_schema: schema_for!(SearchVaultArgs),
                },
                Tool {
                    name: "control_smarthome".to_string(),
                    description: "Control a smart home device".to_string(),
                    input_schema: schema_for!(ControlSmartHomeArgs),
                },
            ]
        }
    }

    // A helper to prevent path traversal
    fn resolve_path(&self, rel_path: &str) -> Result<PathBuf, String> {
        let p = Path::new(rel_path);
        if p.is_absolute() || p.has_root() || p.components().any(|c| c == std::path::Component::ParentDir) {
            return Err("Invalid path (traversal detected)".to_string());
        }
        let full = self.vault_path.join(p);
        if !full.starts_with(&self.vault_path) {
            return Err("Invalid path (traversal detected)".to_string());
        }
        Ok(full)
    }

    pub async fn call_tool(&self, req: CallToolRequest) -> Result<CallToolResult, String> {
        match req.name.as_str() {
            "read_markdown" => {
                let args: ReadMarkdownArgs = serde_json::from_value(req.arguments)
                    .map_err(|e| e.to_string())?;
                let path = self.resolve_path(&args.path)?;
                match tokio::fs::read_to_string(&path).await {
                    Ok(content) => Ok(CallToolResult {
                        content: vec![ToolContent::Text { text: content }],
                        is_error: false,
                    }),
                    Err(e) => Ok(CallToolResult {
                        content: vec![ToolContent::Text { text: format!("Error: {}", e) }],
                        is_error: true,
                    }),
                }
            }
            "write_markdown" => {
                let args: WriteMarkdownArgs = serde_json::from_value(req.arguments)
                    .map_err(|e| e.to_string())?;
                let path = self.resolve_path(&args.path)?;
                if let Some(parent) = path.parent() {
                    tokio::fs::create_dir_all(parent).await.map_err(|e| e.to_string())?;
                }
                match tokio::fs::write(&path, args.content).await {
                    Ok(_) => Ok(CallToolResult {
                        content: vec![ToolContent::Text { text: "Success".to_string() }],
                        is_error: false,
                    }),
                    Err(e) => Ok(CallToolResult {
                        content: vec![ToolContent::Text { text: format!("Error: {}", e) }],
                        is_error: true,
                    }),
                }
            }
            "search_vault" => {
                let args: SearchVaultArgs = serde_json::from_value(req.arguments)
                    .map_err(|e| e.to_string())?;
                
                let mut matched_files = Vec::new();
                let mut files_to_check = Vec::new();
                
                fn walk_dir(dir: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
                    if dir.is_dir() {
                        for entry in std::fs::read_dir(dir)? {
                            let entry = entry?;
                            let path = entry.path();
                            if path.is_dir() {
                                walk_dir(&path, files)?;
                            } else {
                                files.push(path);
                            }
                        }
                    }
                    Ok(())
                }

                if let Err(e) = walk_dir(&self.vault_path, &mut files_to_check) {
                    return Ok(CallToolResult {
                        content: vec![ToolContent::Text { text: format!("Error reading vault directory: {}", e) }],
                        is_error: true,
                    });
                }

                for path in files_to_check {
                    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
                    if ext == "md" || ext == "txt" {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            if content.contains(&args.query) {
                                if let Ok(rel_path) = path.strip_prefix(&self.vault_path) {
                                    let rel_str = rel_path.to_string_lossy().replace('\\', "/");
                                    matched_files.push(rel_str);
                                }
                            }
                        }
                    }
                }

                let text_res = if matched_files.is_empty() {
                    format!("No matching files found for query '{}'", args.query)
                } else {
                    let mut res = format!("Search results for '{}':\n", args.query);
                    for file in matched_files {
                        res.push_str(&format!("- {}\n", file));
                    }
                    res
                };

                Ok(CallToolResult {
                    content: vec![ToolContent::Text { text: text_res }],
                    is_error: false,
                })
            }
            "control_smarthome" => {
                let args: ControlSmartHomeArgs = serde_json::from_value(req.arguments)
                    .map_err(|e| e.to_string())?;
                Ok(CallToolResult {
                    content: vec![ToolContent::Text { text: format!("Command '{}' sent to '{}'", args.command, args.device) }],
                    is_error: false,
                })
            }
            _ => Err(format!("Tool '{}' not found", req.name)),
        }
    }
}
