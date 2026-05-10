//! # Delta-State CRDTs
//!
//! Instead of sending entire CRDT state across the network, send only what changed.
//! A delta is the minimal state needed to bring another replica up to date.

use crate::eisenstein::eisenstein_norm;
use serde::{Deserialize, Serialize};
use std::fmt;

/// A delta (incremental change) for a G-Counter.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CounterDelta {
    /// Node that generated this delta
    pub node: String,
    /// Increment since last delta
    pub satisfied_delta: u64,
    /// Violation increment since last delta
    pub violations_delta: u64,
    /// Sequence number for ordering
    pub seq: u64,
}

/// A delta for the OR-Set (added or removed constraints).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OrsetDelta {
    Add {
        constraint_id: String,
        node: String,
        seq: u64,
    },
    Remove {
        constraint_id: String,
        tombstoned_tags: Vec<(String, u64)>,
        seq: u64,
    },
}

/// A delta for an Eisenstein position.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PositionDelta {
    pub old_norm: i64,
    pub new_norm: i64,
    pub position: (i32, i32),
    pub node: String,
    pub seq: u64,
}

/// A composite delta for the full constraint state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintDelta {
    /// Source node
    pub node: String,
    /// Monotonic sequence number
    pub seq: u64,
    /// Counter deltas
    pub counter: CounterDelta,
    /// Constraint set deltas
    pub constraints: Vec<OrsetDelta>,
    /// Position delta (if changed)
    pub position: Option<PositionDelta>,
}

impl ConstraintDelta {
    /// Create an empty delta
    pub fn empty(node: &str, seq: u64) -> Self {
        Self {
            node: node.to_string(),
            seq,
            counter: CounterDelta {
                node: node.to_string(),
                satisfied_delta: 0,
                violations_delta: 0,
                seq,
            },
            constraints: Vec::new(),
            position: None,
        }
    }

    /// Is this delta empty (no changes)?
    pub fn is_empty(&self) -> bool {
        self.counter.satisfied_delta == 0
            && self.counter.violations_delta == 0
            && self.constraints.is_empty()
            && self.position.is_none()
    }

    /// Serialize to JSON bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// Deserialize from JSON bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        serde_json::from_slice(data).ok()
    }

    /// Approximate wire size in bytes
    pub fn wire_size(&self) -> usize {
        self.to_bytes().len()
    }
}

/// Tracks the last known state per node for delta generation.
#[derive(Debug, Clone, Default)]
pub struct DeltaTracker {
    /// Per-node: (last_satisfied, last_violations, last_seq, last_position)
    node_state: std::collections::HashMap<String, (u64, u64, u64, Option<(i32, i32)>)>,
}

impl DeltaTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Generate a delta from a full state snapshot.
    pub fn generate(
        &mut self,
        node: &str,
        satisfied: u64,
        violations: u64,
        position: (i32, i32),
        added: &[String],
        removed: &[String],
    ) -> ConstraintDelta {
        let (prev_sat, prev_vio, prev_seq, prev_pos) = self
            .node_state
            .get(node)
            .copied()
            .unwrap_or((0, 0, 0, None));

        let new_seq = prev_seq + 1;

        let counter_delta = CounterDelta {
            node: node.to_string(),
            satisfied_delta: satisfied.saturating_sub(prev_sat),
            violations_delta: violations.saturating_sub(prev_vio),
            seq: new_seq,
        };

        let mut constraint_deltas = Vec::new();
        for id in added {
            constraint_deltas.push(OrsetDelta::Add {
                constraint_id: id.clone(),
                node: node.to_string(),
                seq: new_seq,
            });
        }
        for id in removed {
            constraint_deltas.push(OrsetDelta::Remove {
                constraint_id: id.clone(),
                tombstoned_tags: Vec::new(),
                seq: new_seq,
            });
        }

        let pos_delta = if prev_pos != Some(position) {
            Some(PositionDelta {
                old_norm: prev_pos.map(eisenstein_norm).unwrap_or(0),
                new_norm: eisenstein_norm(position),
                position,
                node: node.to_string(),
                seq: new_seq,
            })
        } else {
            None
        };

        self.node_state.insert(
            node.to_string(),
            (satisfied, violations, new_seq, Some(position)),
        );

        ConstraintDelta {
            node: node.to_string(),
            seq: new_seq,
            counter: counter_delta,
            constraints: constraint_deltas,
            position: pos_delta,
        }
    }

    /// Get last known seq for a node
    pub fn last_seq(&self, node: &str) -> u64 {
        self.node_state.get(node).map(|(_, _, s, _)| *s).unwrap_or(0)
    }
}

impl fmt::Display for ConstraintDelta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Delta(node={}, seq={}, +{}s/+{}v, {} ops, {} bytes)",
            self.node, self.seq,
            self.counter.satisfied_delta, self.counter.violations_delta,
            self.constraints.len(),
            self.wire_size())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_delta() {
        let d = ConstraintDelta::empty("node-a", 1);
        assert!(d.is_empty());
    }

    #[test]
    fn test_delta_generation() {
        let mut tracker = DeltaTracker::new();
        let d1 = tracker.generate("node-a", 100, 5, (1, 0), &["c1".into()], &[]);
        assert_eq!(d1.counter.satisfied_delta, 100);
        assert_eq!(d1.counter.violations_delta, 5);
        assert_eq!(d1.constraints.len(), 1);
        assert!(d1.position.is_some());

        let d2 = tracker.generate("node-a", 150, 8, (1, 0), &["c2".into()], &[]);
        assert_eq!(d2.counter.satisfied_delta, 50);
        assert_eq!(d2.counter.violations_delta, 3);
        assert_eq!(d2.constraints.len(), 1);
        assert!(d2.position.is_none());
    }

    #[test]
    fn test_delta_no_change() {
        let mut tracker = DeltaTracker::new();
        tracker.generate("a", 100, 5, (1, 0), &[], &[]);
        let d = tracker.generate("a", 100, 5, (1, 0), &[], &[]);
        assert!(d.is_empty());
    }

    #[test]
    fn test_delta_serialization() {
        let mut tracker = DeltaTracker::new();
        let d = tracker.generate("node-a", 100, 5, (2, 1), &["c1".into()], &[]);
        let bytes = d.to_bytes();
        assert!(!bytes.is_empty());
        let restored = ConstraintDelta::from_bytes(&bytes).unwrap();
        assert_eq!(restored.node, "node-a");
        assert_eq!(restored.counter.satisfied_delta, 100);
    }

    #[test]
    fn test_delta_tracker_per_node() {
        let mut tracker = DeltaTracker::new();
        tracker.generate("a", 100, 5, (0, 0), &[], &[]);
        tracker.generate("b", 200, 10, (0, 0), &[], &[]);
        let da = tracker.generate("a", 120, 5, (0, 0), &[], &[]);
        let db = tracker.generate("b", 200, 15, (0, 0), &[], &[]);
        assert_eq!(da.counter.satisfied_delta, 20);
        assert_eq!(db.counter.satisfied_delta, 0);
        assert_eq!(db.counter.violations_delta, 5);
    }

    #[test]
    fn test_wire_size() {
        let d = ConstraintDelta::empty("node-a", 1);
        assert!(d.wire_size() > 0);
        assert!(d.wire_size() < 200);
    }

    #[test]
    fn test_display() {
        let mut tracker = DeltaTracker::new();
        let d = tracker.generate("a", 100, 5, (1, 0), &["c1".into()], &[]);
        let s = format!("{}", d);
        assert!(s.contains("node=a"));
        assert!(s.contains("+100s"));
    }
}
