//! # Deterministic Network Simulation
//!
//! Test fleet consensus under controlled conditions:
//! - Variable latency
//! - Message loss
//! - Partition/rejoin
//! - Node crash/recovery
//!
//! All random seeds are deterministic — same seed, same simulation.

use crate::gossip::{GossipNode, GossipMessage, GossipResult};
use crate::state::ConstraintState;
use std::collections::HashMap;

/// Simulation seed for deterministic randomness.
pub struct SimRng {
    state: u64,
}

impl SimRng {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// xorshift64 — fast, deterministic, good enough for simulation.
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    /// Uniform f64 in [0, 1)
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Should we drop this message? (based on loss rate 0.0-1.0)
    pub fn should_drop(&mut self, loss_rate: f64) -> bool {
        self.next_f64() < loss_rate
    }

    /// Pick a random peer index (not self).
    pub fn random_peer(&mut self, self_idx: usize, total: usize) -> usize {
        if total <= 1 { return 0; }
        let mut peer = (self.next_u64() as usize) % (total - 1);
        if peer >= self_idx { peer += 1; }
        peer
    }
}

/// A delayed message in the network.
#[derive(Debug, Clone)]
struct InFlightMessage {
    from: usize,
    message: GossipMessage,
    deliver_at_round: u64,
}

/// The simulation environment.
pub struct Simulation {
    /// Nodes in the simulation.
    pub nodes: Vec<GossipNode>,
    /// Random number generator.
    rng: SimRng,
    /// Message loss rate (0.0 = no loss, 1.0 = all lost).
    pub loss_rate: f64,
    /// Messages in flight (delayed delivery).
    in_flight: Vec<InFlightMessage>,
    /// Current simulation round.
    round: u64,
    /// Statistics.
    stats: SimStats,
}

/// Simulation statistics.
#[derive(Debug, Clone, Default)]
pub struct SimStats {
    pub total_messages_sent: u64,
    pub messages_dropped: u64,
    pub messages_delivered: u64,
    pub state_changes: u64,
    pub convergence_rounds: Vec<u64>,
}

impl Simulation {
    /// Create a new simulation with N nodes.
    pub fn new(node_count: usize, seed: u64) -> Self {
        let nodes = (0..node_count)
            .map(|i| GossipNode::new(&format!("node-{}", i)))
            .collect();

        Self {
            nodes,
            rng: SimRng::new(seed),
            loss_rate: 0.0,
            in_flight: Vec::new(),
            round: 0,
            stats: SimStats::default(),
        }
    }

    /// Set message loss rate.
    pub fn with_loss_rate(mut self, rate: f64) -> Self {
        self.loss_rate = rate;
        self
    }

    /// Get stats.
    pub fn stats(&self) -> &SimStats {
        &self.stats
    }

    /// Current round.
    pub fn round(&self) -> u64 {
        self.round
    }

    /// Run one round of gossip: each node pings a random peer.
    pub fn step(&mut self) {
        self.round += 1;
        let n = self.nodes.len();
        let mut new_messages = Vec::new();

        for i in 0..n {
            let peer = self.rng.random_peer(i, n);
            let ping = self.nodes[i].ping();
            self.stats.total_messages_sent += 1;

            if self.rng.should_drop(self.loss_rate) {
                self.stats.messages_dropped += 1;
                continue;
            }

            // Deliver immediately (could add latency delay)
            let result = self.nodes[peer].receive(&ping);
            self.stats.messages_delivered += 1;

            // Process responses
            for resp in result.responses {
                self.stats.total_messages_sent += 1;
                if self.rng.should_drop(self.loss_rate) {
                    self.stats.messages_dropped += 1;
                    continue;
                }
                let back_result = self.nodes[i].receive(&resp);
                self.stats.messages_delivered += 1;
                if back_result.state_changed {
                    self.stats.state_changes += 1;
                }
                new_messages.extend(back_result.responses);
            }
        }
    }

    /// Run simulation until convergence or max rounds.
    /// Returns the round at which convergence was reached, or None.
    pub fn run_until_converged(&mut self, max_rounds: u64) -> Option<u64> {
        for _ in 0..max_rounds {
            self.step();

            // Check convergence: all nodes have the same state hash
            if self.nodes.len() > 1 {
                let hashes: Vec<_> = self.nodes.iter()
                    .map(|n| crate::merkle::StateHash::from_state(&n.state))
                    .collect();
                let first = hashes[0];
                if hashes.iter().all(|h| *h == first) {
                    self.stats.convergence_rounds.push(self.round);
                    return Some(self.round);
                }
            }
        }
        None
    }

    /// Add constraints to a specific node (simulate local operation).
    pub fn add_constraint(&mut self, node_idx: usize, constraint: &str) {
        self.nodes[node_idx].add_constraint(constraint);
    }

    /// Record satisfied constraints on a specific node.
    pub fn record_satisfied(&mut self, node_idx: usize, count: u64) {
        self.nodes[node_idx].record_satisfied(count);
    }

    /// Check if all nodes have converged to the same state.
    pub fn is_converged(&self) -> bool {
        if self.nodes.len() <= 1 { return true; }
        let hashes: Vec<_> = self.nodes.iter()
            .map(|n| crate::merkle::StateHash::from_state(&n.state))
            .collect();
        let first = hashes[0];
        hashes.iter().all(|h| *h == first)
    }

    /// Print a summary of node states.
    pub fn summary(&self) -> String {
        let mut lines = vec![format!("=== Simulation Round {} ===", self.round)];
        for node in &self.nodes {
            lines.push(format!("  {}", node));
        }
        lines.push(format!("Stats: {} sent, {} dropped, {} delivered, {} state changes",
            self.stats.total_messages_sent,
            self.stats.messages_dropped,
            self.stats.messages_delivered,
            self.stats.state_changes));
        lines.push(format!("Converged: {}", self.is_converged()));
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rng_deterministic() {
        let mut a = SimRng::new(42);
        let mut b = SimRng::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn test_rng_different_seeds() {
        let mut a = SimRng::new(42);
        let mut b = SimRng::new(43);
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn test_loss_rate() {
        let mut rng = SimRng::new(42);
        let mut dropped = 0;
        let trials = 10000;
        for _ in 0..trials {
            if rng.should_drop(0.5) { dropped += 1; }
        }
        // Should be roughly 50% ± 2%
        let rate = dropped as f64 / trials as f64;
        assert!(rate > 0.47 && rate < 0.53, "Loss rate was {:.3}, expected ~0.5", rate);
    }

    #[test]
    fn test_two_nodes_converge() {
        let mut sim = Simulation::new(2, 42);
        sim.add_constraint(0, "c1");
        sim.add_constraint(1, "c2");

        let converged = sim.run_until_converged(20);
        assert!(converged.is_some(), "Should converge within 20 rounds");
        assert_eq!(sim.nodes[0].state.active_constraint_count(), 2);
        assert_eq!(sim.nodes[1].state.active_constraint_count(), 2);
    }

    #[test]
    fn test_five_nodes_converge() {
        let mut sim = Simulation::new(5, 42);
        for i in 0..5 {
            sim.add_constraint(i, &format!("c{}", i));
        }

        let converged = sim.run_until_converged(50);
        assert!(converged.is_some(), "5 nodes should converge within 50 rounds");
        for node in &sim.nodes {
            assert_eq!(node.state.active_constraint_count(), 5);
        }
    }

    #[test]
    fn test_converge_with_loss() {
        let mut sim = Simulation::new(4, 42).with_loss_rate(0.3);
        for i in 0..4 {
            sim.add_constraint(i, &format!("c{}", i));
        }

        let converged = sim.run_until_converged(100);
        assert!(converged.is_some(), "Should converge even with 30% loss");
    }

    #[test]
    fn test_summary() {
        let sim = Simulation::new(3, 42);
        let s = sim.summary();
        assert!(s.contains("node-0"));
        assert!(s.contains("Converged:"));
    }

    #[test]
    fn test_random_peer() {
        let mut rng = SimRng::new(42);
        for _ in 0..100 {
            let peer = rng.random_peer(0, 5);
            assert_ne!(peer, 0);
            assert!(peer < 5);
        }
    }
}
