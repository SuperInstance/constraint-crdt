//! # PN-Counter (Positive-Negative Counter)
//!
//! A counter that can both increment AND decrement — extracted from SmartCRDT.
//! Internally two G-Counters: one for additions, one for subtractions.
//!
//! Use cases in the fleet:
//! - Track constraint violations that can be resolved (decrement)
//! - Track node join/leave counts
//! - Track resource allocation/deallocation

use crate::merge::Merge;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// A distributed counter supporting both increment and decrement.
///
/// Internally maintains two G-Counters (positive and negative).
/// Value = positive - negative per node, summed across nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PNCounter {
    /// Per-node positive counts
    p: HashMap<String, u64>,
    /// Per-node negative counts
    n: HashMap<String, u64>,
}

impl PNCounter {
    pub fn new() -> Self {
        Self {
            p: HashMap::new(),
            n: HashMap::new(),
        }
    }

    /// Increment by amount for a node
    pub fn increment(&mut self, node: &str, amount: u64) {
        *self.p.entry(node.to_string()).or_insert(0) += amount;
    }

    /// Decrement by amount for a node
    pub fn decrement(&mut self, node: &str, amount: u64) {
        *self.n.entry(node.to_string()).or_insert(0) += amount;
    }

    /// Current net value (positive - negative, summed across nodes)
    pub fn value(&self) -> i64 {
        let pos: u64 = self.p.values().sum();
        let neg: u64 = self.n.values().sum();
        pos as i64 - neg as i64
    }

    /// Total positive count
    pub fn total_positive(&self) -> u64 {
        self.p.values().sum()
    }

    /// Total negative count
    pub fn total_negative(&self) -> u64 {
        self.n.values().sum()
    }

    /// Get a node's net contribution
    pub fn node_value(&self, node: &str) -> i64 {
        let pos = self.p.get(node).copied().unwrap_or(0);
        let neg = self.n.get(node).copied().unwrap_or(0);
        pos as i64 - neg as i64
    }

    /// Number of nodes contributing
    pub fn node_count(&self) -> usize {
        let mut nodes: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for k in self.p.keys() { nodes.insert(k); }
        for k in self.n.keys() { nodes.insert(k); }
        nodes.len()
    }
}

impl Merge for PNCounter {
    fn merge(&mut self, other: &Self) {
        // Merge positive (take max per node)
        for (node, count) in &other.p {
            let entry = self.p.entry(node.clone()).or_insert(0);
            *entry = (*entry).max(*count);
        }
        // Merge negative (take max per node)
        for (node, count) in &other.n {
            let entry = self.n.entry(node.clone()).or_insert(0);
            *entry = (*entry).max(*count);
        }
    }
}

impl PartialEq for PNCounter {
    fn eq(&self, other: &Self) -> bool {
        self.value() == other.value()
    }
}

impl fmt::Display for PNCounter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PNCounter(+{} -{} = {}, {} nodes)",
            self.total_positive(), self.total_negative(),
            self.value(), self.node_count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merge::laws;

    #[test]
    fn test_increment_decrement() {
        let mut c = PNCounter::new();
        c.increment("a", 100);
        c.decrement("a", 30);
        assert_eq!(c.value(), 70);
    }

    #[test]
    fn test_multi_node() {
        let mut c = PNCounter::new();
        c.increment("a", 100);
        c.increment("b", 200);
        c.decrement("a", 50);
        assert_eq!(c.value(), 250); // (100+200) - 50
    }

    #[test]
    fn test_merge_combines_nodes() {
        let mut a = PNCounter::new();
        a.increment("a", 100);

        let mut b = PNCounter::new();
        b.increment("b", 200);
        b.decrement("b", 50);

        let merged = a.merged(&b);
        assert_eq!(merged.value(), 250);
        assert_eq!(merged.node_count(), 2);
    }

    #[test]
    fn test_merge_commutative() {
        let mut a = PNCounter::new();
        a.increment("a", 100);
        let mut b = PNCounter::new();
        b.increment("b", 200);
        assert!(laws::check_commutative(&a, &b));
    }

    #[test]
    fn test_merge_associative() {
        let mut a = PNCounter::new();
        a.increment("a", 1);
        let mut b = PNCounter::new();
        b.increment("b", 2);
        let mut c = PNCounter::new();
        c.increment("c", 3);
        assert!(laws::check_associative(&a, &b, &c));
    }

    #[test]
    fn test_merge_idempotent() {
        let mut a = PNCounter::new();
        a.increment("a", 100);
        a.decrement("a", 30);
        assert!(laws::check_idempotent(&a));
    }

    #[test]
    fn test_negative_value() {
        let mut c = PNCounter::new();
        c.decrement("a", 100);
        c.increment("a", 30);
        assert_eq!(c.value(), -70);
    }

    #[test]
    fn test_display() {
        let mut c = PNCounter::new();
        c.increment("a", 100);
        c.decrement("a", 30);
        let s = format!("{}", c);
        assert!(s.contains("+100"));
        assert!(s.contains("-30"));
        assert!(s.contains("70"));
    }
}
