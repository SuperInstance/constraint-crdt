//! # Distributed constraint counter (G-Counter)
//!
//! A grow-only counter that tracks constraint satisfaction counts per fleet node.
//! Extracted from SmartCRDT's GCounter and specialized for constraint counting.
//!
//! Each node increments independently; merge takes the max per-node.

use crate::merge::Merge;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// A distributed counter for constraint satisfaction metrics.
///
/// Each fleet node maintains its own count. Merge takes element-wise max.
/// This is a standard state-based G-Counter CRDT.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConstraintGCounter {
    /// Per-node counts: node_id → satisfied count
    counts: HashMap<String, u64>,
    /// Per-node violation counts
    violations: HashMap<String, u64>,
}

impl ConstraintGCounter {
    pub fn new() -> Self {
        Self {
            counts: HashMap::new(),
            violations: HashMap::new(),
        }
    }

    /// Record satisfied constraints from a node
    pub fn record_satisfied(&mut self, node: &str, count: u64) {
        *self.counts.entry(node.to_string()).or_insert(0) += count;
    }

    /// Record violations from a node
    pub fn record_violations(&mut self, node: &str, count: u64) {
        *self.violations.entry(node.to_string()).or_insert(0) += count;
    }

    /// Total satisfied across all nodes
    pub fn total_satisfied(&self) -> u64 {
        self.counts.values().sum()
    }

    /// Total violations across all nodes
    pub fn total_violations(&self) -> u64 {
        self.violations.values().sum()
    }

    /// Satisfaction rate (0.0 - 1.0)
    pub fn satisfaction_rate(&self) -> f64 {
        let total = self.total_satisfied() + self.total_violations();
        if total == 0 { return 1.0; }
        self.total_satisfied() as f64 / total as f64
    }

    /// Nodes reporting
    pub fn node_count(&self) -> usize {
        let mut nodes: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for k in self.counts.keys() { nodes.insert(k); }
        for k in self.violations.keys() { nodes.insert(k); }
        nodes.len()
    }

    /// Get a specific node's satisfied count
    pub fn node_satisfied(&self, node: &str) -> u64 {
        self.counts.get(node).copied().unwrap_or(0)
    }

    /// Get a specific node's violation count
    pub fn node_violations(&self, node: &str) -> u64 {
        self.violations.get(node).copied().unwrap_or(0)
    }
}

impl Merge for ConstraintGCounter {
    fn merge(&mut self, other: &Self) {
        // G-Counter merge: take max per node for counts
        for (node, count) in &other.counts {
            let entry = self.counts.entry(node.clone()).or_insert(0);
            *entry = (*entry).max(*count);
        }
        // Same for violations (separate G-Counter)
        for (node, count) in &other.violations {
            let entry = self.violations.entry(node.clone()).or_insert(0);
            *entry = (*entry).max(*count);
        }
    }
}

impl fmt::Display for ConstraintGCounter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ConstraintGCounter({}/{} satisfied, {} nodes, {:.1}% rate)",
            self.total_satisfied(),
            self.total_satisfied() + self.total_violations(),
            self.node_count(),
            self.satisfaction_rate() * 100.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merge::laws;

    #[test]
    fn test_basic_counting() {
        let mut c = ConstraintGCounter::new();
        c.record_satisfied("node-a", 100);
        c.record_violations("node-a", 5);
        assert_eq!(c.total_satisfied(), 100);
        assert_eq!(c.total_violations(), 5);
        assert!((c.satisfaction_rate() - 0.9524).abs() < 0.01);
    }

    #[test]
    fn test_merge_combines_nodes() {
        let mut a = ConstraintGCounter::new();
        a.record_satisfied("node-a", 100);
        a.record_violations("node-a", 5);

        let mut b = ConstraintGCounter::new();
        b.record_satisfied("node-b", 200);
        b.record_violations("node-b", 10);

        let merged = a.merged(&b);
        assert_eq!(merged.total_satisfied(), 300);
        assert_eq!(merged.total_violations(), 15);
        assert_eq!(merged.node_count(), 2);
    }

    #[test]
    fn test_merge_takes_max_per_node() {
        let mut a = ConstraintGCounter::new();
        a.record_satisfied("node-a", 100);

        let mut b = ConstraintGCounter::new();
        b.record_satisfied("node-a", 150); // Higher count

        let merged = a.merged(&b);
        assert_eq!(merged.node_satisfied("node-a"), 150);
    }

    #[test]
    fn test_merge_commutative() {
        let mut a = ConstraintGCounter::new();
        a.record_satisfied("node-a", 100);
        let mut b = ConstraintGCounter::new();
        b.record_satisfied("node-b", 200);
        assert!(laws::check_commutative(&a, &b));
    }

    #[test]
    fn test_merge_associative() {
        let mut a = ConstraintGCounter::new();
        a.record_satisfied("a", 1);
        let mut b = ConstraintGCounter::new();
        b.record_satisfied("b", 2);
        let mut c = ConstraintGCounter::new();
        c.record_satisfied("c", 3);
        assert!(laws::check_associative(&a, &b, &c));
    }

    #[test]
    fn test_merge_idempotent() {
        let mut a = ConstraintGCounter::new();
        a.record_satisfied("a", 100);
        a.record_violations("a", 5);
        assert!(laws::check_idempotent(&a));
    }

    #[test]
    fn test_subsumes() {
        let mut a = ConstraintGCounter::new();
        a.record_satisfied("a", 100);
        let mut b = ConstraintGCounter::new();
        b.record_satisfied("a", 50);
        // a (100) subsumes b (50) after merge stays at 100
        assert!(a.subsumes(&b));
    }

    #[test]
    fn test_display() {
        let mut c = ConstraintGCounter::new();
        c.record_satisfied("a", 100);
        c.record_violations("a", 5);
        let s = format!("{}", c);
        assert!(s.contains("100"));
        assert!(s.contains("95.2%"));
    }
}
