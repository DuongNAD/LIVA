//! Vector Clock Causal Tracking Engine for Multi-Agent Swarm Orchestration
//!
//! Implements strict partial ordering, causality detection (Equal, Before, After, Concurrent),
//! and actor logical time lattice operations based on the Lamport / Mattern model.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Causal relationship between two vector clocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CausalRelation {
    /// Clocks are identical in all actor dimensions.
    Equal,
    /// Self causally precedes Other (Self is an ancestor of Other).
    Before,
    /// Self causally succeeds Other (Self is a descendant of Other).
    After,
    /// Clocks are concurrent (divergent uncoordinated branches requiring 3-way merge).
    Concurrent,
}

impl fmt::Display for CausalRelation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Equal => write!(f, "Equal"),
            Self::Before => write!(f, "Before (Dominated)"),
            Self::After => write!(f, "After (Dominating)"),
            Self::Concurrent => write!(f, "Concurrent (Conflict)"),
        }
    }
}

/// Deterministic Vector Clock implementation using BTreeMap for ordered serialization.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VectorClock {
    /// Mapping of Actor ID -> Logical Clock Counter
    #[serde(default)]
    pub entries: BTreeMap<String, u64>,
}

impl VectorClock {
    /// Create an empty vector clock.
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    /// Initialize a vector clock for a specific actor with an initial tick.
    pub fn from_actor(actor_id: impl Into<String>, initial_tick: u64) -> Self {
        let mut clock = Self::new();
        clock.entries.insert(actor_id.into(), initial_tick);
        clock
    }

    /// Retrieve logical clock for a specific actor (0 if never observed).
    #[inline]
    pub fn get(&self, actor_id: &str) -> u64 {
        self.entries.get(actor_id).copied().unwrap_or(0)
    }

    /// Increment the local actor's logical clock and return the new value.
    pub fn tick(&mut self, actor_id: impl Into<String>) -> u64 {
        let entry = self.entries.entry(actor_id.into()).or_insert(0);
        *entry += 1;
        *entry
    }

    /// Set an explicit counter value for an actor.
    pub fn set(&mut self, actor_id: impl Into<String>, value: u64) {
        self.entries.insert(actor_id.into(), value);
    }

    /// Compute element-wise maximum merge in-place: for all k: self[k] = max(self[k], other[k]).
    pub fn merge(&mut self, other: &VectorClock) {
        for (actor, &other_tick) in &other.entries {
            let entry = self.entries.entry(actor.clone()).or_insert(0);
            if other_tick > *entry {
                *entry = other_tick;
            }
        }
    }

    /// Alias for merge() to update clock causality from received message clock.
    pub fn update(&mut self, other: &VectorClock) {
        self.merge(other);
    }

    /// Return a new VectorClock representing the element-wise maximum merge.
    pub fn merged(&self, other: &VectorClock) -> VectorClock {
        let mut result = self.clone();
        result.merge(other);
        result
    }

    /// Compute element-wise minimum (used for finding Least Common Ancestor bound).
    pub fn meet(&self, other: &VectorClock) -> VectorClock {
        let mut result = VectorClock::new();
        for (actor, &tick) in &self.entries {
            if let Some(&other_tick) = other.entries.get(actor) {
                let min_tick = tick.min(other_tick);
                if min_tick > 0 {
                    result.entries.insert(actor.clone(), min_tick);
                }
            }
        }
        result
    }

    /// Determine strict causal relationship with another vector clock.
    pub fn relation(&self, other: &VectorClock) -> CausalRelation {
        let mut self_has_greater = false;
        let mut other_has_greater = false;

        let mut all_actors: BTreeSet<&str> = self.entries.keys().map(|s| s.as_str()).collect();
        for k in other.entries.keys() {
            all_actors.insert(k.as_str());
        }

        for actor in all_actors {
            let v_self = self.get(actor);
            let v_other = other.get(actor);

            if v_self > v_other {
                self_has_greater = true;
            } else if v_other > v_self {
                other_has_greater = true;
            }

            if self_has_greater && other_has_greater {
                return CausalRelation::Concurrent;
            }
        }

        match (self_has_greater, other_has_greater) {
            (false, false) => CausalRelation::Equal,
            (true, false) => CausalRelation::After,
            (false, true) => CausalRelation::Before,
            (true, true) => CausalRelation::Concurrent,
        }
    }

    /// Returns true if self strictly causally precedes other (self < other).
    pub fn causally_precedes(&self, other: &VectorClock) -> bool {
        self.relation(other) == CausalRelation::Before
    }

    /// Returns true if two vector clocks are concurrent (neither dominates the other).
    pub fn is_concurrent_with(&self, other: &VectorClock) -> bool {
        self.relation(other) == CausalRelation::Concurrent
    }

    /// Returns true if self is an ancestor of or equal to other (self <= other).
    pub fn is_ancestor_of(&self, other: &VectorClock) -> bool {
        matches!(self.relation(other), CausalRelation::Before | CausalRelation::Equal)
    }

    /// Returns true if self is a descendant of or equal to other (self >= other).
    pub fn is_descendant_of(&self, other: &VectorClock) -> bool {
        matches!(self.relation(other), CausalRelation::After | CausalRelation::Equal)
    }

    /// Fork a new child clock for a spawned subagent.
    pub fn fork(&self, child_actor_id: impl Into<String>) -> VectorClock {
        let mut child = self.clone();
        child.tick(child_actor_id);
        child
    }

    /// Calculate tick difference per actor (self - other).
    pub fn diff(&self, other: &VectorClock) -> BTreeMap<String, i64> {
        let mut delta = BTreeMap::new();
        let mut all_keys: BTreeSet<&str> = self.entries.keys().map(|s| s.as_str()).collect();
        for k in other.entries.keys() {
            all_keys.insert(k.as_str());
        }

        for k in all_keys {
            let diff = self.get(k) as i64 - other.get(k) as i64;
            if diff != 0 {
                delta.insert(k.to_string(), diff);
            }
        }
        delta
    }

    /// Check if clock is completely empty (zero ticks across all dimensions).
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() || self.entries.values().all(|&v| v == 0)
    }

    /// Reset all clocks to zero.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector_clock_tick_and_get() {
        let mut vc = VectorClock::new();
        assert_eq!(vc.get("planner"), 0);
        assert_eq!(vc.tick("planner"), 1);
        assert_eq!(vc.tick("planner"), 2);
        assert_eq!(vc.get("planner"), 2);
        assert_eq!(vc.tick("coder"), 1);
        assert_eq!(vc.get("coder"), 1);
    }

    #[test]
    fn test_vector_clock_causality_relations() {
        let mut v1 = VectorClock::new();
        v1.set("planner", 1);
        v1.set("coder", 1);

        let mut v2 = VectorClock::new();
        v2.set("planner", 1);
        v2.set("coder", 1);

        // Equal
        assert_eq!(v1.relation(&v2), CausalRelation::Equal);
        assert!(v1.is_ancestor_of(&v2));
        assert!(v1.is_descendant_of(&v2));

        // Before / After
        v2.tick("coder"); // v2: {planner: 1, coder: 2}
        assert_eq!(v1.relation(&v2), CausalRelation::Before);
        assert_eq!(v2.relation(&v1), CausalRelation::After);
        assert!(v1.causally_precedes(&v2));
        assert!(v1.is_ancestor_of(&v2));
        assert!(!v2.causally_precedes(&v1));

        // Concurrent
        let mut v3 = VectorClock::new();
        v3.set("planner", 2);
        v3.set("coder", 1); // v3: {planner: 2, coder: 1} vs v2: {planner: 1, coder: 2}
        assert_eq!(v2.relation(&v3), CausalRelation::Concurrent);
        assert!(v2.is_concurrent_with(&v3));
        assert!(!v2.is_ancestor_of(&v3));
        assert!(!v3.is_ancestor_of(&v2));
    }

    #[test]
    fn test_vector_clock_merge_and_meet() {
        let mut v1 = VectorClock::new();
        v1.set("planner", 2);
        v1.set("coder", 1);

        let mut v2 = VectorClock::new();
        v2.set("planner", 1);
        v2.set("coder", 3);
        v2.set("reviewer", 1);

        let merged = v1.merged(&v2);
        assert_eq!(merged.get("planner"), 2);
        assert_eq!(merged.get("coder"), 3);
        assert_eq!(merged.get("reviewer"), 1);

        let meet = v1.meet(&v2);
        assert_eq!(meet.get("planner"), 1);
        assert_eq!(meet.get("coder"), 1);
        assert_eq!(meet.get("reviewer"), 0);
    }

    #[test]
    fn test_vector_clock_fork_and_diff() {
        let mut parent = VectorClock::new();
        parent.set("root", 5);

        let child = parent.fork("subagent_1");
        assert_eq!(child.get("root"), 5);
        assert_eq!(child.get("subagent_1"), 1);

        let delta = child.diff(&parent);
        assert_eq!(delta.get("subagent_1"), Some(&1));
        assert_eq!(delta.get("root"), None);
    }
}
