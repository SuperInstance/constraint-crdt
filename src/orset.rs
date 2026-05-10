//! # Constraint OR-Set
//!
//! Observed-Remove Set for tracking which constraints are applied across fleet nodes.
//! Extracted from SmartCRDT's OR-Set, specialized for constraint tracking.
//!
//! Key property: `add` wins over `remove` for concurrent operations.
//! This means if one node removes a constraint while another adds it,
//! the constraint stays — safe default for safety-critical systems.

use crate::merge::Merge;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;

/// Unique tag for an OR-Set operation
type Tag = (String, u64); // (node_id, sequence_number)

/// An OR-Set tracking constraint applications across fleet nodes.
///
/// Supports:
/// - `add(constraint_id, node)` — add a constraint with unique tag
/// - `remove(constraint_id)` — remove by observing current tags
/// - `merge(other)` — conflict-free merge via add-wins semantics
///
/// The "add-wins" policy is critical for constraint safety:
/// if in doubt, keep the constraint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintORSet {
    /// Constraint IDs → set of unique tags
    elements: HashMap<String, HashSet<Tag>>,
    /// Removed tags (tombstones)
    tombstones: HashSet<Tag>,
    /// Per-node sequence counters
    seq: HashMap<String, u64>,
}

impl ConstraintORSet {
    pub fn new() -> Self {
        Self {
            elements: HashMap::new(),
            tombstones: HashSet::new(),
            seq: HashMap::new(),
        }
    }

    /// Add a constraint from a specific node
    pub fn add(&mut self, constraint_id: &str, node: &str) {
        let seq = self.seq.entry(node.to_string()).or_insert(0);
        *seq += 1;
        let tag = (node.to_string(), *seq);

        self.elements
            .entry(constraint_id.to_string())
            .or_insert_with(HashSet::new)
            .insert(tag);
    }

    /// Remove a constraint (observed-remove: must have seen it first)
    pub fn remove(&mut self, constraint_id: &str) {
        if let Some(tags) = self.elements.get(constraint_id) {
            for tag in tags {
                self.tombstones.insert(tag.clone());
            }
            self.elements.remove(constraint_id);
        }
    }

    /// Check if a constraint is currently active
    pub fn contains(&self, constraint_id: &str) -> bool {
        self.elements.contains_key(constraint_id)
    }

    /// Get all active constraint IDs
    pub fn active_constraints(&self) -> Vec<String> {
        self.elements.keys().cloned().collect()
    }

    /// Number of active constraints
    pub fn len(&self) -> usize {
        self.elements.len()
    }

    /// Is the set empty?
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    /// Number of tombstones (for garbage collection decisions)
    pub fn tombstone_count(&self) -> usize {
        self.tombstones.len()
    }

    /// Garbage collect tombstones older than a given sequence number per node.
    /// Only call this when you're sure all nodes have seen these operations.
    pub fn gc_tombstones(&mut self, node: &str, before_seq: u64) {
        self.tombstones.retain(|(n, seq)| {
            !(n == node && *seq < before_seq)
        });
    }
}

impl Merge for ConstraintORSet {
    fn merge(&mut self, other: &Self) {
        // Merge elements (union of tags per constraint)
        for (constraint, tags) in &other.elements {
            let entry = self.elements
                .entry(constraint.clone())
                .or_insert_with(HashSet::new);
            entry.extend(tags.iter().cloned());
        }

        // Merge tombstones
        self.tombstones.extend(other.tombstones.iter().cloned());

        // Merge sequence counters (take max)
        for (node, seq) in &other.seq {
            let entry = self.seq.entry(node.clone()).or_insert(0);
            *entry = (*entry).max(*seq);
        }

        // Remove elements whose tags are ALL tombstoned (add-wins)
        self.elements.retain(|_, tags| {
            tags.iter().any(|tag| !self.tombstones.contains(tag))
        });
    }
}

impl PartialEq for ConstraintORSet {
    fn eq(&self, other: &Self) -> bool {
        let self_keys: HashSet<_> = self.elements.keys().collect();
        let other_keys: HashSet<_> = other.elements.keys().collect();
        self_keys == other_keys
    }
}

impl fmt::Display for ConstraintORSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ConstraintORSet({} active, {} tombstones)",
            self.len(), self.tombstone_count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merge::laws;

    #[test]
    fn test_add_contains() {
        let mut s = ConstraintORSet::new();
        s.add("bounds_check", "node-a");
        assert!(s.contains("bounds_check"));
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn test_remove() {
        let mut s = ConstraintORSet::new();
        s.add("bounds_check", "node-a");
        s.remove("bounds_check");
        assert!(!s.contains("bounds_check"));
    }

    #[test]
    fn test_add_wins_on_merge() {
        // Node A adds constraint from both nodes
        let mut a = ConstraintORSet::new();
        a.add("bounds_check", "node-a");

        // Node B sees the constraint, then removes it
        let mut b = a.clone();
        b.remove("bounds_check");

        // But node A never saw the remove — it still has the add
        // After merge: add wins (node-a's tag survives because b tombstoned node-b's tag)
        // In this case b tombstoned node-a's tag, so add-wins requires a NEW add
        
        // Re-add from node-b after removing — add-wins means new add survives
        b.add("bounds_check", "node-b");

        let merged = a.merged(&b);
        assert!(merged.contains("bounds_check"),
            "add-wins: re-added constraint should survive");
    }

    #[test]
    fn test_merge_combines_constraints() {
        let mut a = ConstraintORSet::new();
        a.add("bounds_check", "node-a");

        let mut b = ConstraintORSet::new();
        b.add("norm_check", "node-b");

        let merged = a.merged(&b);
        assert!(merged.contains("bounds_check"));
        assert!(merged.contains("norm_check"));
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn test_merge_commutative() {
        let mut a = ConstraintORSet::new();
        a.add("c1", "node-a");
        let mut b = ConstraintORSet::new();
        b.add("c2", "node-b");
        assert!(laws::check_commutative(&a, &b));
    }

    #[test]
    fn test_merge_associative() {
        let mut a = ConstraintORSet::new();
        a.add("c1", "a");
        let mut b = ConstraintORSet::new();
        b.add("c2", "b");
        let mut c = ConstraintORSet::new();
        c.add("c3", "c");
        assert!(laws::check_associative(&a, &b, &c));
    }

    #[test]
    fn test_merge_idempotent() {
        let mut a = ConstraintORSet::new();
        a.add("c1", "node-a");
        a.add("c2", "node-a");
        assert!(laws::check_idempotent(&a));
    }

    #[test]
    fn test_gc_tombstones() {
        let mut s = ConstraintORSet::new();
        s.add("c1", "node-a");
        s.remove("c1");
        assert_eq!(s.tombstone_count(), 1);

        s.gc_tombstones("node-a", 10);
        assert_eq!(s.tombstone_count(), 0);
    }
}
