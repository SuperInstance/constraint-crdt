//! # Novel Experiment 3: Time-Decay CRDT
//!
//! Constraints that lose weight over time — old violations decay,
//! recent violations weigh more. This models real systems where
//! "3 violations in the last hour" matters more than "100 violations last month".
//!
//! Uses exponential decay: weight = e^(-λ * age)
//! The decay parameter λ controls how fast old data becomes irrelevant.
//!
//! Novel: the decay IS a semilattice operation (monotone decreasing),
//! so time-decay CRDTs still satisfy C/A/I laws.

use crate::merge::Merge;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// A time-decaying counter. Each event decays exponentially.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecayCounter {
    /// Per-node decay accumulators: node → (last_value, last_time_ns)
    accumulators: HashMap<String, (f64, u64)>,
    /// Decay rate λ (higher = faster decay)
    lambda: f64,
    /// Current total (lazily computed)
    total: f64,
}

impl DecayCounter {
    pub fn new(half_life_secs: f64) -> Self {
        // λ = ln(2) / half_life
        let lambda = 2.0_f64.ln() / half_life_secs;
        Self {
            accumulators: HashMap::new(),
            lambda,
            total: 0.0,
        }
    }

    /// Record an event from a node at a given time.
    pub fn record(&mut self, node: &str, value: f64, time_ns: u64) {
        let entry = self.accumulators.entry(node.to_string()).or_insert((0.0, 0));
        
        // Decay existing value to current time
        let elapsed_ns = time_ns.saturating_sub(entry.1);
        let elapsed_secs = elapsed_ns as f64 / 1e9;
        entry.0 *= (-self.lambda * elapsed_secs).exp();
        
        // Add new value
        entry.0 += value;
        entry.1 = time_ns;
        
        self.recompute_total();
    }

    /// Get the current decayed value for a node.
    pub fn node_value(&mut self, node: &str, now_ns: u64) -> f64 {
        if let Some((val, last_time)) = self.accumulators.get(node) {
            let elapsed_ns = now_ns.saturating_sub(*last_time);
            let elapsed_secs = elapsed_ns as f64 / 1e9;
            val * (-self.lambda * elapsed_secs).exp()
        } else {
            0.0
        }
    }

    /// Get the total decayed value across all nodes.
    pub fn total(&mut self, now_ns: u64) -> f64 {
        let mut sum = 0.0;
        for (val, last_time) in self.accumulators.values() {
            let elapsed_ns = now_ns.saturating_sub(*last_time);
            let elapsed_secs = elapsed_ns as f64 / 1e9;
            sum += val * (-self.lambda * elapsed_secs).exp();
        }
        self.total = sum;
        sum
    }

    /// Half-life in seconds.
    pub fn half_life(&self) -> f64 {
        2.0_f64.ln() / self.lambda
    }

    /// Lambda (decay rate).
    pub fn lambda(&self) -> f64 {
        self.lambda
    }

    fn recompute_total(&mut self) {
        // Approximate — exact total requires knowing current time
        self.total = self.accumulators.values().map(|(v, _)| v).sum();
    }
}

impl Merge for DecayCounter {
    fn merge(&mut self, other: &Self) {
        // Merge accumulators: take the one with later timestamp per node
        for (node, (val, time)) in &other.accumulators {
            let entry = self.accumulators.entry(node.clone()).or_insert((0.0, 0));
            if *time > entry.1 {
                *entry = (*val, *time);
            } else if *time == entry.1 {
                // Same time: take max value
                entry.0 = entry.0.max(*val);
            }
            // If our time is later, keep ours
        }
        self.recompute_total();
    }
}

impl PartialEq for DecayCounter {
    fn eq(&self, other: &Self) -> bool {
        (self.total - other.total).abs() < 0.001
    }
}

/// A time-decaying constraint state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecayConstraintState {
    pub node_id: String,
    /// Satisfied constraints (decay counter)
    pub satisfied: DecayCounter,
    /// Violations (decay counter)
    pub violations: DecayCounter,
    /// Half-life in seconds
    pub half_life: f64,
}

impl DecayConstraintState {
    pub fn new(node_id: &str, half_life_secs: f64) -> Self {
        Self {
            node_id: node_id.to_string(),
            satisfied: DecayCounter::new(half_life_secs),
            violations: DecayCounter::new(half_life_secs),
            half_life: half_life_secs,
        }
    }

    /// Record satisfied constraints.
    pub fn record_satisfied(&mut self, count: f64, time_ns: u64) {
        self.satisfied.record(&self.node_id, count, time_ns);
    }

    /// Record violations.
    pub fn record_violations(&mut self, count: f64, time_ns: u64) {
        self.violations.record(&self.node_id, count, time_ns);
    }

    /// Get satisfaction rate at a given time.
    pub fn satisfaction_rate(&mut self, time_ns: u64) -> f64 {
        let sat = self.satisfied.total(time_ns);
        let vio = self.violations.total(time_ns);
        let total = sat + vio;
        if total == 0.0 { return 1.0; }
        sat / total
    }

    /// Current violation "weight" (how much recent violations matter).
    pub fn violation_weight(&mut self, time_ns: u64) -> f64 {
        self.violations.total(time_ns)
    }
}

impl Merge for DecayConstraintState {
    fn merge(&mut self, other: &Self) {
        self.satisfied.merge(&other.satisfied);
        self.violations.merge(&other.violations);
    }
}

impl PartialEq for DecayConstraintState {
    fn eq(&self, other: &Self) -> bool {
        self.satisfied == other.satisfied && self.violations == other.violations
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merge::laws;

    const NS_PER_SEC: u64 = 1_000_000_000;

    #[test]
    fn test_decay() {
        let mut dc = DecayCounter::new(1.0); // 1 second half-life
        dc.record("a", 100.0, 0);
        
        // After 1 second, should be ~50
        let val = dc.node_value("a", NS_PER_SEC);
        assert!((val - 50.0).abs() < 1.0, "Expected ~50, got {:.1}", val);
        
        // After 2 seconds, should be ~25
        let val = dc.node_value("a", 2 * NS_PER_SEC);
        assert!((val - 25.0).abs() < 1.0, "Expected ~25, got {:.1}", val);
    }

    #[test]
    fn test_recent_weighs_more() {
        let mut dc = DecayCounter::new(1.0);
        dc.record("a", 100.0, 0); // 100 at t=0
        dc.record("b", 100.0, 5 * NS_PER_SEC); // 100 at t=5s
        
        // At t=6s: a decayed to ~3.1, b decayed to ~50
        let total = dc.total(6 * NS_PER_SEC);
        assert!(total < 60.0, "Recent violations should dominate, total={:.1}", total);
    }

    #[test]
    fn test_merge_takes_latest() {
        let mut a = DecayCounter::new(1.0);
        a.record("x", 100.0, 10 * NS_PER_SEC);
        
        let mut b = DecayCounter::new(1.0);
        b.record("x", 200.0, 20 * NS_PER_SEC);
        
        let merged = a.merged(&b);
        // Should take b's value (later timestamp)
        assert!(merged.accumulators.get("x").unwrap().1 == 20 * NS_PER_SEC);
    }

    #[test]
    fn test_decay_constraint_state() {
        let mut state = DecayConstraintState::new("test", 60.0); // 1 min half-life
        state.record_satisfied(100.0, 0);
        state.record_violations(5.0, 0);
        
        // At t=0: rate = 100/105 ≈ 95.2%
        let rate = state.satisfaction_rate(0);
        assert!((rate - 0.952).abs() < 0.01);
    }

    #[test]
    fn test_old_violations_decay() {
        let mut state = DecayConstraintState::new("test", 1.0);
        state.record_violations(100.0, 0);
        
        // After 10 half-lives, violations should be negligible
        let weight = state.violation_weight(10 * NS_PER_SEC);
        assert!(weight < 1.0, "Old violations should decay: weight={:.2}", weight);
    }
}
