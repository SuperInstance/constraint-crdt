//! # Constraint State — Composite CRDT
//!
//! The top-level state of a constraint satisfaction system as a single CRDT.
//! Combines all sub-CRDTs into one mergeable unit.

use crate::merge::Merge;
use crate::counter::ConstraintGCounter;
use crate::orset::ConstraintORSet;
use crate::eisenstein::EisensteinRegister;
use serde::{Deserialize, Serialize};
use std::fmt;

/// The complete constraint state of a fleet node, mergeable without coordination.
///
/// This is the key data structure: each node maintains one `ConstraintState`,
/// and periodically merges with other nodes. The result is always consistent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintState {
    /// Node identifier
    pub node_id: String,
    /// Active constraints (OR-Set)
    pub constraints: ConstraintORSet,
    /// Aggregate metrics (G-Counter)
    pub metrics: ConstraintGCounter,
    /// Current position in constraint space
    pub position: EisensteinRegister,
    /// State version (increments on each local mutation)
    pub version: u64,
}

impl ConstraintState {
    pub fn new(node_id: &str) -> Self {
        Self {
            node_id: node_id.to_string(),
            constraints: ConstraintORSet::new(),
            metrics: ConstraintGCounter::new(),
            position: EisensteinRegister::new((0, 0), node_id),
            version: 0,
        }
    }

    /// Add a constraint
    pub fn add_constraint(&mut self, id: &str) {
        self.constraints.add(id, &self.node_id);
        self.version += 1;
    }

    /// Remove a constraint
    pub fn remove_constraint(&mut self, id: &str) {
        self.constraints.remove(id);
        self.version += 1;
    }

    /// Record satisfied constraints
    pub fn record_satisfied(&mut self, count: u64) {
        self.metrics.record_satisfied(&self.node_id, count);
        self.version += 1;
    }

    /// Record violations
    pub fn record_violations(&mut self, count: u64) {
        self.metrics.record_violations(&self.node_id, count);
        self.version += 1;
    }

    /// Update lattice position
    pub fn update_position(&mut self, pos: (i32, i32)) {
        self.position.update(pos, &self.node_id);
        self.version += 1;
    }

    /// Satisfaction rate (0.0 - 1.0)
    pub fn satisfaction_rate(&self) -> f64 {
        self.metrics.satisfaction_rate()
    }

    /// Number of active constraints
    pub fn active_constraint_count(&self) -> usize {
        self.constraints.len()
    }

    /// Serialize full state
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

impl Merge for ConstraintState {
    fn merge(&mut self, other: &Self) {
        self.constraints.merge(&other.constraints);
        self.metrics.merge(&other.metrics);
        self.position.merge(&other.position);
        // Version: take max, but don't increment (idempotence)
        self.version = self.version.max(other.version);
    }
}

impl fmt::Display for ConstraintState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ConstraintState(node={}, v={}, {} active, {:.1}% satisfied, pos={})",
            self.node_id, self.version,
            self.active_constraint_count(),
            self.satisfaction_rate() * 100.0,
            self.position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merge::laws;

    #[test]
    fn test_state_creation() {
        let s = ConstraintState::new("forgemaster");
        assert_eq!(s.node_id, "forgemaster");
        assert_eq!(s.version, 0);
        assert_eq!(s.active_constraint_count(), 0);
    }

    #[test]
    fn test_add_remove_constraints() {
        let mut s = ConstraintState::new("a");
        s.add_constraint("bounds");
        s.add_constraint("norm");
        assert_eq!(s.active_constraint_count(), 2);

        s.remove_constraint("bounds");
        assert_eq!(s.active_constraint_count(), 1);
        assert!(s.constraints.contains("norm"));
    }

    #[test]
    fn test_merge_two_nodes() {
        let mut a = ConstraintState::new("forgemaster");
        a.add_constraint("bounds");
        a.add_constraint("norm");
        a.record_satisfied(1000);
        a.record_violations(5);

        let mut b = ConstraintState::new("oracle1");
        b.add_constraint("holonomy");
        b.record_satisfied(2000);
        b.record_violations(10);

        let merged = a.merged(&b);

        // All constraints present
        assert!(merged.constraints.contains("bounds"));
        assert!(merged.constraints.contains("norm"));
        assert!(merged.constraints.contains("holonomy"));

        // Metrics aggregated
        assert_eq!(merged.metrics.total_satisfied(), 3000);
        assert_eq!(merged.metrics.total_violations(), 15);

        // Version takes max
        assert!(merged.version >= a.version);
    }

    #[test]
    fn test_merge_commutative() {
        let mut a = ConstraintState::new("a");
        a.add_constraint("c1");
        a.record_satisfied(100);

        let mut b = ConstraintState::new("b");
        b.add_constraint("c2");
        b.record_satisfied(200);

        assert!(laws::check_commutative(&a, &b));
    }

    #[test]
    fn test_merge_associative() {
        let mut a = ConstraintState::new("a");
        a.add_constraint("c1");
        let mut b = ConstraintState::new("b");
        b.add_constraint("c2");
        let mut c = ConstraintState::new("c");
        c.add_constraint("c3");
        assert!(laws::check_associative(&a, &b, &c));
    }

    #[test]
    fn test_merge_idempotent() {
        let mut a = ConstraintState::new("a");
        a.add_constraint("c1");
        a.record_satisfied(100);
        assert!(laws::check_idempotent(&a));
    }

    #[test]
    fn test_satisfaction_rate() {
        let mut s = ConstraintState::new("a");
        s.record_satisfied(950);
        s.record_violations(50);
        assert!((s.satisfaction_rate() - 0.95).abs() < 0.01);
    }

    #[test]
    fn test_json_roundtrip() {
        let mut s = ConstraintState::new("test");
        s.add_constraint("c1");
        s.record_satisfied(100);
        let json = s.to_json();
        assert!(json.contains("test"));
        assert!(json.contains("c1"));
    }
}

impl PartialEq for ConstraintState {
    fn eq(&self, other: &Self) -> bool {
        // CRDT semantics: equality is about the data, not the node_id
        self.constraints == other.constraints
            && self.metrics == other.metrics
            && self.position == other.position
            && self.version == other.version
    }
}
