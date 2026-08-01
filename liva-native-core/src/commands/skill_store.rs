//! Miền skill store.
//!
//! Bảy lệnh knowledge còn sót trong `lib.rs` sau khi các miền khác đã được
//! tách. Chúng dùng chung `AppState::{db,embedder}` và một trust root, nên đây
//! là một miền kết dính thay vì bảy arm ở dispatcher.

use crate::{AppState, resolve_resource_path, skills};
use serde_json::{Value, json};
use std::sync::Arc;

const OWNED: &[&str] = &[
    "skills:sync",
    "skills:list",
    "skills:search",
    "skills:signal",
    "skills:signals",
    "skills:history",
    "skills:pin_ids",
];

pub fn owns(command: &str) -> bool {
    OWNED.contains(&command)
}

/// Thư mục gốc kho skill do người vận hành chọn, không nhận từ payload.
///
/// `skills:pin_ids` có quyền ghi `.skill_id`; cho client truyền path sẽ biến
/// lệnh này thành path traversal. Mặc định dùng `skills/`, không tự sửa cây
/// `.claude/skills` của công cụ khác.
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
        assert!(!owns("skills:unknown"));
        assert!(!owns("memory:get_fact"));
    }
}
