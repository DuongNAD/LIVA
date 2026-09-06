//! 3-Way RFC 6902 JSON Patch State Merge Engine
//!
//! Reconciles concurrent agent state mutations:
//! S_merged = ThreeWayMerger::merge(S_base, S_ours, S_theirs)
//! Features deep object recursion, intelligent array merge, and policy-driven conflict resolution.

use super::conflict::{ConflictItem, ConflictResolutionStrategy, ConflictType, MergeResult};
use crate::agent::graph::checkpoint::JsonPatchOp;
use serde_json::{Map, Value};
use std::collections::HashSet;

/// 3-Way State Merger implementation.
pub struct ThreeWayMerger {
    pub strategy: ConflictResolutionStrategy,
}

impl ThreeWayMerger {
    pub fn new(strategy: ConflictResolutionStrategy) -> Self {
        Self { strategy }
    }

    /// Perform a full 3-way merge on generic JSON values: Base, Ours, Theirs.
    pub fn merge(
        &self,
        base: &Value,
        ours: &Value,
        theirs: &Value,
    ) -> Result<MergeResult, String> {
        let mut conflicts = Vec::new();
        let mut auto_resolved = 0;
        let mut unresolvable = 0;

        let merged = self.merge_recursive(
            "",
            base,
            ours,
            theirs,
            &mut conflicts,
            &mut auto_resolved,
            &mut unresolvable,
        )?;

        let is_clean = unresolvable == 0;
        Ok(MergeResult {
            merged_state: merged,
            conflicts,
            is_clean,
            auto_resolved_count: auto_resolved,
            unresolvable_count: unresolvable,
        })
    }

    fn merge_recursive(
        &self,
        path: &str,
        base: &Value,
        ours: &Value,
        theirs: &Value,
        conflicts: &mut Vec<ConflictItem>,
        auto_resolved: &mut usize,
        unresolvable: &mut usize,
    ) -> Result<Value, String> {
        // Fast paths: exact identity checks
        if ours == theirs {
            return Ok(ours.clone());
        }
        if ours == base {
            // Only theirs modified
            return Ok(theirs.clone());
        }
        if theirs == base {
            // Only ours modified
            return Ok(ours.clone());
        }

        // Both modified from base!
        match (base, ours, theirs) {
            (Value::Object(b_map), Value::Object(o_map), Value::Object(t_map)) => {
                self.merge_objects(path, b_map, o_map, t_map, conflicts, auto_resolved, unresolvable)
            }
            (Value::Array(b_arr), Value::Array(o_arr), Value::Array(t_arr)) => {
                self.merge_arrays(path, b_arr, o_arr, t_arr, conflicts, auto_resolved, unresolvable)
            }
            _ => {
                // Scalar / Type Mismatch Conflict
                let conflict_item = ConflictItem {
                    path: if path.is_empty() {
                        "/".to_string()
                    } else {
                        path.to_string()
                    },
                    conflict_type: ConflictType::ValueMismatch,
                    base_value: Some(base.clone()),
                    our_value: Some(ours.clone()),
                    their_value: Some(theirs.clone()),
                    our_op: JsonPatchOp::Replace {
                        path: path.to_string(),
                        value: ours.clone(),
                    },
                    their_op: JsonPatchOp::Replace {
                        path: path.to_string(),
                        value: theirs.clone(),
                    },
                    resolved_value: None,
                    resolution_applied: None,
                };

                let resolved = self.resolve_conflict(&conflict_item, auto_resolved, unresolvable)?;
                let mut final_item = conflict_item;
                final_item.resolved_value = Some(resolved.clone());
                final_item.resolution_applied = Some(format!("{:?}", self.strategy));
                conflicts.push(final_item);

                Ok(resolved)
            }
        }
    }

    fn merge_objects(
        &self,
        path: &str,
        base: &Map<String, Value>,
        ours: &Map<String, Value>,
        theirs: &Map<String, Value>,
        conflicts: &mut Vec<ConflictItem>,
        auto_resolved: &mut usize,
        unresolvable: &mut usize,
    ) -> Result<Value, String> {
        let mut merged_map = Map::new();
        let mut all_keys: HashSet<String> = base.keys().cloned().collect();
        all_keys.extend(ours.keys().cloned());
        all_keys.extend(theirs.keys().cloned());

        for key in all_keys {
            let child_path = format!("{}/{}", path, key.replace('~', "~0").replace('/', "~1"));
            let in_b = base.get(&key);
            let in_o = ours.get(&key);
            let in_t = theirs.get(&key);

            match (in_b, in_o, in_t) {
                // Key unmodified in both
                (Some(b_val), Some(o_val), Some(t_val)) if o_val == b_val && t_val == b_val => {
                    merged_map.insert(key, b_val.clone());
                }
                // Only ours modified key
                (Some(b_val), Some(o_val), Some(t_val)) if t_val == b_val => {
                    merged_map.insert(key, o_val.clone());
                }
                // Only theirs modified key
                (Some(b_val), Some(o_val), Some(t_val)) if o_val == b_val => {
                    merged_map.insert(key, t_val.clone());
                }
                // Both modified key concurrently
                (Some(b_val), Some(o_val), Some(t_val)) => {
                    let merged_val = self.merge_recursive(
                        &child_path,
                        b_val,
                        o_val,
                        t_val,
                        conflicts,
                        auto_resolved,
                        unresolvable,
                    )?;
                    merged_map.insert(key, merged_val);
                }
                // Added only in ours
                (None, Some(o_val), None) => {
                    merged_map.insert(key, o_val.clone());
                }
                // Added only in theirs
                (None, None, Some(t_val)) => {
                    merged_map.insert(key, t_val.clone());
                }
                // Added concurrently in both
                (None, Some(o_val), Some(t_val)) => {
                    let merged_val = self.merge_recursive(
                        &child_path,
                        &Value::Null,
                        o_val,
                        t_val,
                        conflicts,
                        auto_resolved,
                        unresolvable,
                    )?;
                    merged_map.insert(key, merged_val);
                }
                // Deleted only in ours
                (Some(b_val), None, Some(t_val)) if t_val == b_val => {
                    // Clean deletion from ours
                }
                // Deleted only in theirs
                (Some(b_val), Some(o_val), None) if o_val == b_val => {
                    // Clean deletion from theirs
                }
                // Delete-Modify conflict: ours deleted, theirs modified
                (Some(b_val), None, Some(t_val)) => {
                    let conflict_item = ConflictItem {
                        path: child_path.clone(),
                        conflict_type: ConflictType::DeleteModify,
                        base_value: Some(b_val.clone()),
                        our_value: None,
                        their_value: Some(t_val.clone()),
                        our_op: JsonPatchOp::Remove {
                            path: child_path.clone(),
                        },
                        their_op: JsonPatchOp::Replace {
                            path: child_path.clone(),
                            value: t_val.clone(),
                        },
                        resolved_value: None,
                        resolution_applied: None,
                    };
                    let resolved = self.resolve_conflict(&conflict_item, auto_resolved, unresolvable)?;
                    if !resolved.is_null() {
                        merged_map.insert(key, resolved);
                    }
                    conflicts.push(conflict_item);
                }
                // Modify-Delete conflict: ours modified, theirs deleted
                (Some(b_val), Some(o_val), None) => {
                    let conflict_item = ConflictItem {
                        path: child_path.clone(),
                        conflict_type: ConflictType::ModifyDelete,
                        base_value: Some(b_val.clone()),
                        our_value: Some(o_val.clone()),
                        their_value: None,
                        our_op: JsonPatchOp::Replace {
                            path: child_path.clone(),
                            value: o_val.clone(),
                        },
                        their_op: JsonPatchOp::Remove {
                            path: child_path.clone(),
                        },
                        resolved_value: None,
                        resolution_applied: None,
                    };
                    let resolved = self.resolve_conflict(&conflict_item, auto_resolved, unresolvable)?;
                    if !resolved.is_null() {
                        merged_map.insert(key, resolved);
                    }
                    conflicts.push(conflict_item);
                }
                // Deleted in both (idempotent removal)
                (Some(_), None, None) => {}
                (None, None, None) => {}
            }
        }

        Ok(Value::Object(merged_map))
    }

    fn merge_arrays(
        &self,
        path: &str,
        base: &[Value],
        ours: &[Value],
        theirs: &[Value],
        conflicts: &mut Vec<ConflictItem>,
        auto_resolved: &mut usize,
        unresolvable: &mut usize,
    ) -> Result<Value, String> {
        // Special case: append streams such as messages and visited_nodes
        let is_append_stream = path.ends_with("/messages")
            || path.ends_with("/visited_nodes")
            || path.ends_with("messages")
            || path.ends_with("visited_nodes");

        if is_append_stream {
            let mut merged_arr = base.to_vec();

            // Collect items added by ours not present in base
            let base_set: HashSet<String> = base
                .iter()
                .map(|v| serde_json::to_string(v).unwrap_or_default())
                .collect();

            for item in ours {
                let s = serde_json::to_string(item).unwrap_or_default();
                if !base_set.contains(&s) && !merged_arr.contains(item) {
                    merged_arr.push(item.clone());
                }
            }

            // Collect items added by theirs not present in base or already added
            for item in theirs {
                let s = serde_json::to_string(item).unwrap_or_default();
                if !base_set.contains(&s) && !merged_arr.contains(item) {
                    merged_arr.push(item.clone());
                }
            }

            *auto_resolved += 1;
            return Ok(Value::Array(merged_arr));
        }

        // Positional 3-way merge on elements
        let max_len = base.len().max(ours.len()).max(theirs.len());
        let mut merged_arr = Vec::new();

        for i in 0..max_len {
            let item_path = format!("{}/{}", path, i);
            let b_elem = base.get(i).unwrap_or(&Value::Null);
            let o_elem = ours.get(i).unwrap_or(&Value::Null);
            let t_elem = theirs.get(i).unwrap_or(&Value::Null);

            let res = self.merge_recursive(
                &item_path,
                b_elem,
                o_elem,
                t_elem,
                conflicts,
                auto_resolved,
                unresolvable,
            )?;

            if !res.is_null() {
                merged_arr.push(res);
            }
        }

        Ok(Value::Array(merged_arr))
    }

    fn resolve_conflict(
        &self,
        conflict: &ConflictItem,
        auto_resolved: &mut usize,
        unresolvable: &mut usize,
    ) -> Result<Value, String> {
        match &self.strategy {
            ConflictResolutionStrategy::LastWriterWins
            | ConflictResolutionStrategy::DeepMergeLww => {
                *auto_resolved += 1;
                Ok(conflict.their_value.clone().unwrap_or(Value::Null))
            }
            ConflictResolutionStrategy::PreferOurs => {
                *auto_resolved += 1;
                Ok(conflict.our_value.clone().unwrap_or(Value::Null))
            }
            ConflictResolutionStrategy::PreferTheirs => {
                *auto_resolved += 1;
                Ok(conflict.their_value.clone().unwrap_or(Value::Null))
            }
            ConflictResolutionStrategy::SentinelAuthority { .. } => {
                *auto_resolved += 1;
                // If remote modified it, adopt remote, otherwise keep ours
                Ok(conflict
                    .their_value
                    .clone()
                    .unwrap_or_else(|| conflict.our_value.clone().unwrap_or(Value::Null)))
            }
            ConflictResolutionStrategy::FailFast => {
                *unresolvable += 1;
                Err(format!(
                    "Unresolvable conflict at '{}' under FailFast strategy",
                    conflict.path
                ))
            }
            ConflictResolutionStrategy::ManualHitl => {
                *unresolvable += 1;
                Ok(conflict.base_value.clone().unwrap_or(Value::Null))
            }
            ConflictResolutionStrategy::PathScoped(rules) => {
                for (prefix, sub_strategy) in rules {
                    if conflict.path.starts_with(prefix) {
                        let sub_merger = ThreeWayMerger::new((**sub_strategy).clone());
                        return sub_merger.resolve_conflict(conflict, auto_resolved, unresolvable);
                    }
                }
                *auto_resolved += 1;
                Ok(conflict.their_value.clone().unwrap_or(Value::Null))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_disjoint_object_merge() {
        let merger = ThreeWayMerger::new(ConflictResolutionStrategy::DeepMergeLww);
        let base = json!({
            "common": "initial",
            "branch_a": "base_a",
            "branch_b": "base_b"
        });

        let ours = json!({
            "common": "initial",
            "branch_a": "updated_by_ours",
            "branch_b": "base_b"
        });

        let theirs = json!({
            "common": "initial",
            "branch_a": "base_a",
            "branch_b": "updated_by_theirs"
        });

        let res = merger.merge(&base, &ours, &theirs).unwrap();
        assert!(res.is_clean);
        assert_eq!(
            res.merged_state,
            json!({
                "common": "initial",
                "branch_a": "updated_by_ours",
                "branch_b": "updated_by_theirs"
            })
        );
    }

    #[test]
    fn test_conflicting_scalar_lww_and_prefer_ours() {
        let base = json!({"field": "base"});
        let ours = json!({"field": "ours"});
        let theirs = json!({"field": "theirs"});

        // 1. LWW -> chooses theirs
        let merger_lww = ThreeWayMerger::new(ConflictResolutionStrategy::LastWriterWins);
        let res_lww = merger_lww.merge(&base, &ours, &theirs).unwrap();
        assert_eq!(res_lww.merged_state, json!({"field": "theirs"}));
        assert_eq!(res_lww.conflicts.len(), 1);

        // 2. PreferOurs -> chooses ours
        let merger_ours = ThreeWayMerger::new(ConflictResolutionStrategy::PreferOurs);
        let res_ours = merger_ours.merge(&base, &ours, &theirs).unwrap();
        assert_eq!(res_ours.merged_state, json!({"field": "ours"}));
    }

    #[test]
    fn test_messages_append_stream_deduplicated_merge() {
        let merger = ThreeWayMerger::new(ConflictResolutionStrategy::DeepMergeLww);
        let base = json!({
            "messages": [
                {"role": "system", "content": "persona"},
                {"role": "user", "content": "goal"}
            ]
        });

        let ours = json!({
            "messages": [
                {"role": "system", "content": "persona"},
                {"role": "user", "content": "goal"},
                {"role": "planner", "content": "plan_step_1"}
            ]
        });

        let theirs = json!({
            "messages": [
                {"role": "system", "content": "persona"},
                {"role": "user", "content": "goal"},
                {"role": "coder", "content": "diff_patch_1"}
            ]
        });

        let res = merger.merge(&base, &ours, &theirs).unwrap();
        assert!(res.is_clean);
        let msgs = res.merged_state["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 4);
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[1]["role"], "user");
        assert_eq!(msgs[2]["role"], "planner");
        assert_eq!(msgs[3]["role"], "coder");
    }
}
