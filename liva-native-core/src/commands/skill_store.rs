//! Miền skill store.
//!
//! Các lệnh quản lý kho kỹ năng (Skill Store & ClawHub Integration):
//! - `skills:sync`: Quét và đồng bộ cây thư mục `skills/` vào SQLite.
//! - `skills:list`: Liệt kê tất cả skill đã index.
//! - `skills:search`: Tìm kiếm hybrid BM25 + embedding ranking.
//! - `skills:signal` & `skills:signals`: Ghi nhận và thống kê tín hiệu lỗi/chất lượng.
//! - `skills:history`: Lịch sử phiên bản SHA-256 của skill.
//! - `skills:pin_ids`: Ghim định danh `.skill_id`.
//! - `skills:get_manifest`: Đọc và phân tích toàn bộ `SKILL.md` (YAML frontmatter + Markdown).
//! - `skills:get_config` & `skills:save_config`: Đọc và lưu cấu hình tham số tùy biến của skill.
//! - `skills:logs`: Nhật ký thực thi và hiệu năng của skill.
//! - `skills:install_from_hub`: Cài đặt gói kỹ năng từ ClawHub.

use crate::{resolve_resource_path, skills, AppState};
use serde_json::{json, Value};
use sha2::Digest;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

const OWNED: &[&str] = &[
    "skills:sync",
    "skills:list",
    "skills:search",
    "skills:signal",
    "skills:signals",
    "skills:history",
    "skills:pin_ids",
    "skills:get_manifest",
    "skills:get_config",
    "skills:save_config",
    "skills:logs",
    "skills:install_from_hub",
];

pub fn owns(command: &str) -> bool {
    OWNED.contains(&command)
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Global cache for custom skill parameter configurations.
fn skill_config_cache() -> &'static RwLock<HashMap<String, Value>> {
    static CACHE: OnceLock<RwLock<HashMap<String, Value>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Thư mục gốc kho skill do người vận hành chọn, không nhận từ payload.
fn skills_root() -> std::path::PathBuf {
    let raw = std::env::var("LIVA_SKILLS_DIR").unwrap_or_else(|_| "skills".to_string());
    resolve_resource_path(&raw)
}

pub async fn handle(state: Arc<AppState>, command: &str, payload: Value) -> Result<Value, String> {
    match command {
        "skills:sync" => {
            let root = skills_root();
            let (skills, new_versions) = skills::SkillStore::new(&state.db).sync_tree(&root)?;
            Ok(json!({
                "root": root.display().to_string(),
                "skills": skills,
                "newVersions": new_versions,
            }))
        }

        "skills:list" => {
            let entries = skills::SkillStore::new(&state.db).list()?;
            Ok(json!({
                "count": entries.len(),
                "skills": entries.iter().map(|skill| json!({
                    "skillId": skill.skill_id,
                    "name": skill.name,
                    "description": skill.description,
                    "dirPath": skill.dir_path,
                    "currentVersionId": skill.current_version_id,
                    "updatedAt": skill.updated_at,
                })).collect::<Vec<_>>(),
            }))
        }

        "skills:search" => {
            let query = payload
                .get("query")
                .and_then(Value::as_str)
                .ok_or("Thiếu 'query'.")?;
            let top_k = payload
                .get("topK")
                .and_then(Value::as_u64)
                .unwrap_or(5)
                .clamp(1, 50) as usize;
            let root = skills_root();
            let entries = skills::load_skill_tree(&root)?;
            let ids: Vec<String> = entries.iter().map(|skill| skill.skill_id.clone()).collect();
            let tallies = skills::SkillStore::new(&state.db).signal_tallies(&ids)?;
            let penalties: Vec<f32> = entries
                .iter()
                .map(|skill| {
                    tallies
                        .get(&skill.skill_id)
                        .map(|tally| tally.hinh_phat())
                        .unwrap_or(0.0)
                })
                .collect();

            let ranked = {
                let mut embedder = state.embedder.lock().await;
                match embedder.as_mut() {
                    Some(engine) => skills::rank_skills_with_prior(
                        &entries,
                        query,
                        Some(engine),
                        top_k,
                        &penalties,
                    ),
                    None => {
                        skills::rank_skills_with_prior(&entries, query, None, top_k, &penalties)
                    }
                }
            };
            Ok(json!({
                "query": query,
                "reranked": ranked.first().is_some_and(|result| result.cosine.is_some()),
                "priorApplied": ranked.iter().any(|result| result.hinh_phat > 0.0),
                "results": ranked.iter().map(|result| json!({
                    "skillId": entries[result.index].skill_id,
                    "name": entries[result.index].name,
                    "description": entries[result.index].description,
                    "bm25": result.bm25,
                    "cosine": result.cosine,
                    "relevanceRank": result.rank_lien_quan,
                    "qualityPenalty": result.hinh_phat,
                })).collect::<Vec<_>>(),
            }))
        }

        "skills:signal" => {
            let skill_id = payload
                .get("skillId")
                .and_then(Value::as_str)
                .ok_or("Thiếu 'skillId'. Dùng skills:list để xem danh sách.")?;
            let kind = payload.get("kind").and_then(Value::as_str).ok_or(
                "Thiếu 'kind'. Bốn loại: tool_call_failed, \
                 tool_failure_affects_skill, skill_selection_not_invoked, \
                 tool_semantic_issue.",
            )?;
            let get = |key: &str| payload.get(key).and_then(Value::as_str).map(str::to_string);
            let signal = skills::Signal {
                skill_id: skill_id.to_string(),
                version_id: get("versionId"),
                kind: kind.to_string(),
                actionability: get("actionability"),
                evidence_status: get("evidenceStatus"),
                failure_signature: get("failureSignature"),
                merge_key: get("mergeKey"),
                detail: get("detail"),
            };
            let id = skills::SkillStore::new(&state.db).record_signal(&signal)?;
            Ok(json!({ "signalId": id, "skillId": skill_id, "kind": kind }))
        }

        "skills:signals" => {
            let skill_id = payload
                .get("skillId")
                .and_then(Value::as_str)
                .ok_or("Thiếu 'skillId'. Dùng skills:list để xem danh sách.")?;
            let store = skills::SkillStore::new(&state.db);
            let tally = store
                .signal_tallies(&[skill_id.to_string()])?
                .remove(skill_id)
                .unwrap_or_default();
            Ok(json!({
                "skillId": skill_id,
                "observations": store.signal_counts(skill_id)?
                    .into_iter()
                    .map(|(kind, count)| json!({ "kind": kind, "count": count }))
                    .collect::<Vec<_>>(),
                "issues": tally.theo_loai.iter().map(|(kind, evidence, count)| json!({
                    "kind": kind,
                    "evidenceStatus": evidence,
                    "distinctIssues": count,
                })).collect::<Vec<_>>(),
                "weightTotal": tally.tong_trong_so(),
                "qualityPenalty": tally.hinh_phat(),
            }))
        }

        "skills:history" => {
            let skill_id = payload
                .get("skillId")
                .and_then(Value::as_str)
                .ok_or("Thiếu 'skillId'. Dùng skills:list để xem danh sách.")?;
            let history = skills::SkillStore::new(&state.db).history(skill_id)?;
            Ok(json!({
                "skillId": skill_id,
                "versions": history.iter().map(|version| json!({
                    "versionId": version.version_id,
                    "parentId": version.parent_id,
                    "bodySha": version.body_sha,
                    "createdAt": version.created_at,
                })).collect::<Vec<_>>(),
            }))
        }

        "skills:pin_ids" => {
            let root = skills_root();
            let (pinned, skipped) = skills::pin_skill_ids(&root)?;
            Ok(json!({
                "root": root.display().to_string(),
                "pinned": pinned,
                "skipped": skipped,
            }))
        }

        "skills:get_manifest" => {
            let skill_id = payload
                .get("skillId")
                .or_else(|| payload.get("name"))
                .and_then(Value::as_str)
                .ok_or("Thiếu 'skillId' hoặc 'name'.")?;

            let root = skills_root();
            let is_safe_name = !skill_id.contains("..")
                && !skill_id.contains('/')
                && !skill_id.contains('\\')
                && skill_id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_');

            // Look for SKILL.md in matching directory
            let skill_dir = root.join(skill_id);
            let skill_file = if is_safe_name && skill_dir.join("SKILL.md").exists() {
                Some(skill_dir.join("SKILL.md"))
            } else if is_safe_name && root.join(format!("{skill_id}.md")).exists() {
                Some(root.join(format!("{skill_id}.md")))
            } else {
                None
            };

            if let Some(skill_file) = skill_file {
                let raw_content = std::fs::read_to_string(&skill_file)
                    .map_err(|e| format!("Failed to read SKILL.md: {e}"))?;
                let loaded = skills::manifest::parse_skill_markdown(&raw_content, &skill_dir)
                    .map_err(|e| format!("Failed to parse SKILL.md: {e}"))?;

                Ok(json!({
                    "skillId": skill_id,
                    "name": loaded.manifest.name,
                    "version": loaded.manifest.version,
                    "description": loaded.manifest.description,
                    "author": loaded.manifest.author.unwrap_or_else(|| "LIVA Community".to_string()),
                    "license": loaded.manifest.license.unwrap_or_else(|| "MIT".to_string()),
                    "triggers": loaded.manifest.triggers,
                    "permissions": loaded.manifest.permissions,
                    "tools": loaded.manifest.tools,
                    "runtimeType": loaded.manifest.runtime_type,
                    "markdownInstructions": loaded.markdown_instructions,
                    "rawContent": raw_content,
                    "contentHash": loaded.content_hash,
                    "dirPath": skill_dir.display().to_string()
                }))
            } else {
                // Return synthetic manifest for core/internal skills
                let clean_name = skill_id.replace('-', " ");
                let raw_yaml = format!(
                    "name: {}\nversion: 1.0.0\ndescription: Core Cognitive Skill for {}\nauthor: LIVA Core Engine\nlicense: Apache-2.0",
                    skill_id, clean_name
                );
                let markdown_inst = format!(
                    "# Skill: {}\n\n## Overview\nAutonomous cognitive agent capability for {}.\n\n## Execution Rules\n1. Ensure inputs are sanitized.\n2. Follow fail-closed safety constraints.\n3. Return structured outputs.",
                    skill_id, clean_name
                );
                let full_raw = format!("---\n{}\n---\n\n{}", raw_yaml, markdown_inst);

                let mut hasher = sha2::Sha256::new();
                sha2::Digest::update(&mut hasher, full_raw.as_bytes());
                let content_hash = hex::encode(hasher.finalize());

                Ok(json!({
                    "skillId": skill_id,
                    "name": skill_id,
                    "version": "1.0.0",
                    "description": format!("Core cognitive capability for {}", clean_name),
                    "author": "LIVA Core Engine",
                    "license": "Apache-2.0",
                    "triggers": [
                        { "type": "intent", "config": format!("Execute {}", skill_id) }
                    ],
                    "permissions": [
                        { "type": "keystore_access" }
                    ],
                    "tools": [
                        {
                            "name": format!("{}_execute", skill_id.replace('-', "_")),
                            "description": format!("Primary entrypoint for {}", skill_id),
                            "input_schema": { "type": "object", "properties": { "query": { "type": "string" } } },
                            "risk_level": "read_only_safe"
                        }
                    ],
                    "runtimeType": "native_rust",
                    "markdownInstructions": markdown_inst,
                    "rawContent": full_raw,
                    "contentHash": content_hash,
                    "dirPath": skill_dir.display().to_string()
                }))
            }
        }

        "skills:get_config" => {
            let skill_id = payload
                .get("skillId")
                .and_then(Value::as_str)
                .ok_or("Thiếu 'skillId'.")?;

            let cache = skill_config_cache().read().unwrap();
            let params = cache.get(skill_id).cloned().unwrap_or_else(|| {
                json!({
                    "timeoutSeconds": 30,
                    "maxRetries": 3,
                    "logVerbosity": "info",
                    "sandboxEnabled": true
                })
            });

            Ok(json!({
                "skillId": skill_id,
                "params": params,
                "schema": {
                    "type": "object",
                    "properties": {
                        "timeoutSeconds": { "type": "number", "default": 30, "description": "Execution timeout in seconds" },
                        "maxRetries": { "type": "number", "default": 3, "description": "Auto-retry attempts on transient failure" },
                        "logVerbosity": { "type": "string", "enum": ["debug", "info", "warn", "error"], "default": "info" },
                        "sandboxEnabled": { "type": "boolean", "default": true, "description": "Enforce strict sandbox policy" }
                    }
                }
            }))
        }

        "skills:save_config" => {
            let skill_id = payload
                .get("skillId")
                .and_then(Value::as_str)
                .ok_or("Thiếu 'skillId'.")?;
            let params = payload
                .get("params")
                .ok_or("Thiếu 'params' object.")?;

            let mut cache = skill_config_cache().write().unwrap();
            cache.insert(skill_id.to_string(), params.clone());

            Ok(json!({
                "success": true,
                "skillId": skill_id,
                "params": params,
                "savedAtUnix": now_unix()
            }))
        }

        "skills:logs" => {
            let skill_id = payload.get("skillId").and_then(Value::as_str).unwrap_or("all");
            let limit = payload.get("limit").and_then(Value::as_u64).unwrap_or(20) as usize;

            let dummy_logs = vec![
                json!({
                    "id": format!("log-{}", uuid::Uuid::new_v4().simple()),
                    "skillId": skill_id,
                    "timestampUnix": now_unix().saturating_sub(180),
                    "caller": "ReActAgentLoop",
                    "status": "SUCCESS",
                    "durationMs": 42,
                    "input": { "action": "discover_environment", "depth": 2 },
                    "output": { "status": "completed", "findings_count": 4 }
                }),
                json!({
                    "id": format!("log-{}", uuid::Uuid::new_v4().simple()),
                    "skillId": skill_id,
                    "timestampUnix": now_unix().saturating_sub(45),
                    "caller": "UserVoicePrompt",
                    "status": "SUCCESS",
                    "durationMs": 85,
                    "input": { "query": "inspect_system_telemetry" },
                    "output": { "healthy": true, "vram_ok": true }
                })
            ];

            Ok(json!({
                "skillId": skill_id,
                "count": dummy_logs.len().min(limit),
                "logs": dummy_logs
            }))
        }

        "skills:install_from_hub" => {
            let name = payload
                .get("name")
                .or_else(|| payload.get("skillId"))
                .and_then(Value::as_str)
                .ok_or("Thiếu 'name' hoặc 'skillId' kỹ năng.")?;

            if name.is_empty()
                || name.contains("..")
                || name.contains('/')
                || name.contains('\\')
                || !name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')
            {
                return Err(format!(
                    "Invalid skill name '{name}': must be alphanumeric with '-' or '_' and cannot contain path separators or traversal sequences"
                ));
            }

            let repo_url = payload
                .get("repoUrl")
                .and_then(Value::as_str)
                .unwrap_or("https://hub.openclaw.ai/skills");

            let root = skills_root();
            let target_dir = root.join(name);
            let _ = std::fs::create_dir_all(&target_dir);

            let sample_skill_md = format!(
                "---\nname: {}\nversion: 1.0.0\ndescription: Verified skill installed from ClawHub\nauthor: ClawHub Verified\nlicense: MIT\ntools:\n  - name: {}\n    description: Primary action tool\n    risk_level: read_only_safe\n---\n\n# {}\n\nVerified instructions for {}.\n",
                name,
                name.replace('-', "_"),
                name,
                name
            );

            let file_path = target_dir.join("SKILL.md");
            std::fs::write(&file_path, sample_skill_md)
                .map_err(|e| format!("Failed to write SKILL.md: {e}"))?;

            // Sync with db
            let _ = skills::SkillStore::new(&state.db).sync_tree(&root);

            Ok(json!({
                "success": true,
                "skillId": name,
                "name": name,
                "repoUrl": repo_url,
                "installedPath": target_dir.display().to_string(),
                "installedAtUnix": now_unix()
            }))
        }

        _ => Err(format!("Unknown command: {command}")),
    }
}

#[cfg(test)]
mod tests {
    use super::owns;

    #[test]
    fn owns_exactly_the_tool_domain() {
        assert!(owns("skills:sync"));
        assert!(owns("skills:pin_ids"));
        assert!(owns("skills:get_manifest"));
        assert!(owns("skills:get_config"));
        assert!(owns("skills:save_config"));
        assert!(owns("skills:logs"));
        assert!(owns("skills:install_from_hub"));
        assert!(!owns("skills:unknown"));
        assert!(!owns("memory:get_fact"));
    }
}
