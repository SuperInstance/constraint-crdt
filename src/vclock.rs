//! # Vector Clock
//!
//! Causal ordering for fleet node operations. Before you can merge states,
//! you need to know which events happened-before which.
//!
//! Vector clocks give us:
//! - **Happened-before**: `a < b` means a definitely happened before b
//! - **Concurrent**: neither `a < b` nor `b < a` — both happened independently
//! - **Causality**: merge operations are only valid between causally related states

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// A vector clock for causal ordering across fleet nodes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VectorClock {
    /// Per-node logical timestamps
    clock: HashMap<String, u64>,
}

/// The causal relationship between two vector clocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CausalOrder {
    /// a happened-before b (a is strictly older)
    Before,
    /// b happened-before a (a is strictly newer)
    After,
    /// Neither happened before the other — concurrent events
    Concurrent,
    /// Identical clocks
    Equal,
}

impl VectorClock {
    /// Create a new empty vector clock.
    pub fn new() -> Self {
        Self {
            clock: HashMap::new(),
        }
    }

    /// Create with a single node's timestamp.
    pub fn from_node(node: &str, time: u64) -> Self {
        let mut clock = HashMap::new();
        clock.insert(node.to_string(), time);
        Self { clock }
    }

    /// Increment a node's counter (after a local event).
    pub fn increment(&mut self, node: &str) -> u64 {
        let entry = self.clock.entry(node.to_string()).or_insert(0);
        *entry += 1;
        *entry
    }

    /// Get a node's counter.
    pub fn get(&self, node: &str) -> u64 {
        self.clock.get(node).copied().unwrap_or(0)
    }

    /// Merge with another vector clock (element-wise max).
    /// Returns self after merging.
    pub fn merge(&mut self, other: &Self) {
        for (node, time) in &other.clock {
            let entry = self.clock.entry(node.clone()).or_insert(0);
            *entry = (*entry).max(*time);
        }
    }

    /// Compare causal ordering with another clock.
    pub fn compare(&self, other: &Self) -> CausalOrder {
        let all_nodes: std::collections::HashSet<&String> =
            self.clock.keys().chain(other.clock.keys()).collect();

        let mut self_less = false;
        let mut other_less = false;

        for node in &all_nodes {
            let s = self.clock.get(*node).copied().unwrap_or(0);
            let o = other.clock.get(*node).copied().unwrap_or(0);
            if s < o { self_less = true; }
            if s > o { other_less = true; }
        }

        match (self_less, other_less) {
            (false, false) => CausalOrder::Equal,
            (true, false) => CausalOrder::Before,
            (false, true) => CausalOrder::After,
            (true, true) => CausalOrder::Concurrent,
        }
    }

    /// Is this clock happened-before or equal to other?
    pub fn happened_before_or_equal(&self, other: &Self) -> bool {
        matches!(self.compare(other), CausalOrder::Before | CausalOrder::Equal)
    }

    /// Is this clock concurrent with other?
    pub fn is_concurrent(&self, other: &Self) -> bool {
        self.compare(other) == CausalOrder::Concurrent
    }

    /// Number of nodes in this clock.
    pub fn node_count(&self) -> usize {
        self.clock.len()
    }

    /// Total logical time (sum of all counters).
    pub fn total_time(&self) -> u64 {
        self.clock.values().sum()
    }

    /// All nodes tracked.
    pub fn nodes(&self) -> Vec<&String> {
        self.clock.keys().collect()
    }

    /// Serialize to JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Deserialize from JSON.
    pub fn from_json(s: &str) -> Option<Self> {
        serde_json::from_str(s).ok()
    }
}

impl fmt::Display for VectorClock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut entries: Vec<_> = self.clock.iter().collect();
        entries.sort_by_key(|(k, _)| k.clone());
        write!(f, "VC{{")?;
        for (i, (node, time)) in entries.iter().enumerate() {
            if i > 0 { write!(f, ", ")?; }
            write!(f, "{}:{}", node, time)?;
        }
        write!(f, "}}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_increment() {
        let mut vc = VectorClock::new();
        assert_eq!(vc.increment("a"), 1);
        assert_eq!(vc.increment("a"), 2);
        assert_eq!(vc.increment("b"), 1);
        assert_eq!(vc.get("a"), 2);
        assert_eq!(vc.get("b"), 1);
        assert_eq!(vc.get("c"), 0); // Unknown node
    }

    #[test]
    fn test_compare_equal() {
        let mut a = VectorClock::new();
        a.increment("a");
        assert_eq!(a.compare(&a), CausalOrder::Equal);
    }

    #[test]
    fn test_compare_before() {
        let mut a = VectorClock::new();
        a.increment("a"); // a: {a:1}

        let mut b = VectorClock::new();
        b.increment("a"); // b: {a:1}
        b.increment("a"); // b: {a:2}

        assert_eq!(a.compare(&b), CausalOrder::Before);
        assert_eq!(b.compare(&a), CausalOrder::After);
    }

    #[test]
    fn test_compare_concurrent() {
        let mut a = VectorClock::new();
        a.increment("a"); // a: {a:1}

        let mut b = VectorClock::new();
        b.increment("b"); // b: {b:1}

        assert_eq!(a.compare(&b), CausalOrder::Concurrent);
        assert!(a.is_concurrent(&b));
    }

    #[test]
    fn test_happened_before() {
        let mut a = VectorClock::new();
        a.increment("a");
        let mut b = a.clone();
        b.increment("a");
        assert!(a.happened_before_or_equal(&b));
        assert!(!b.happened_before_or_equal(&a));
    }

    #[test]
    fn test_merge() {
        let mut a = VectorClock::new();
        a.increment("a"); // {a:1}

        let mut b = VectorClock::new();
        b.increment("b"); // {b:1}

        a.merge(&b); // {a:1, b:1}
        assert_eq!(a.get("a"), 1);
        assert_eq!(a.get("b"), 1);
    }

    #[test]
    fn test_merge_takes_max() {
        let mut a = VectorClock::from_node("x", 5);
        let mut b = VectorClock::from_node("x", 10);
        a.merge(&b);
        assert_eq!(a.get("x"), 10);
    }

    #[test]
    fn test_display() {
        let mut vc = VectorClock::new();
        vc.increment("forgemaster");
        vc.increment("oracle1");
        let s = format!("{}", vc);
        assert!(s.contains("forgemaster:1"));
        assert!(s.contains("oracle1:1"));
    }

    #[test]
    fn test_json_roundtrip() {
        let mut vc = VectorClock::new();
        vc.increment("a");
        vc.increment("b");
        let json = vc.to_json();
        let restored = VectorClock::from_json(&json).unwrap();
        assert_eq!(vc, restored);
    }

    #[test]
    fn test_total_time() {
        let mut vc = VectorClock::new();
        vc.increment("a"); // 1
        vc.increment("a"); // 2
        vc.increment("b"); // 1
        assert_eq!(vc.total_time(), 3);
    }

    #[test]
    fn test_three_node_causality() {
        // a → b → c chain
        let mut a = VectorClock::new();
        a.increment("a"); // {a:1}

        let mut b = a.clone();
        b.increment("b"); // {a:1, b:1}

        let mut c = b.clone();
        c.increment("c"); // {a:1, b:1, c:1}

        assert_eq!(a.compare(&c), CausalOrder::Before);
        assert_eq!(c.compare(&a), CausalOrder::After);
        assert!(a.happened_before_or_equal(&c));
    }
}
