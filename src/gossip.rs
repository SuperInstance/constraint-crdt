//! # Anti-Entropy Gossip Protocol
//!
//! Eventually-consistent state sync via Merkle hash exchange.

use crate::merge::Merge;
use crate::state::ConstraintState;
use crate::merkle::StateHash;
use crate::delta::{ConstraintDelta, DeltaTracker};
use crate::vclock::VectorClock;
use serde::{Deserialize, Serialize};
use std::fmt;

/// A gossip message between fleet nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GossipMessage {
    /// "Here's my hash, do we match?"
    Ping {
        node: String,
        state_hash: StateHash,
        clock: VectorClock,
    },
    /// "Hashes differ, here's my full state"
    Sync {
        node: String,
        state: ConstraintState,
        clock: VectorClock,
    },
    /// "Here's what changed since last sync"
    Delta {
        node: String,
        delta: ConstraintDelta,
        clock: VectorClock,
    },
    /// "Acknowledged"
    Ack {
        node: String,
        clock: VectorClock,
    },
}

/// The gossip state machine for a single node.
#[derive(Debug, Clone)]
pub struct GossipNode {
    pub node_id: String,
    pub state: ConstraintState,
    pub clock: VectorClock,
    pub tracker: DeltaTracker,
    peer_hashes: std::collections::HashMap<String, StateHash>,
    rounds: u64,
    syncs: u64,
}

pub struct GossipResult {
    pub responses: Vec<GossipMessage>,
    pub state_changed: bool,
    pub converged: bool,
}

impl GossipNode {
    pub fn new(node_id: &str) -> Self {
        Self {
            node_id: node_id.to_string(),
            state: ConstraintState::new(node_id),
            clock: VectorClock::new(),
            tracker: DeltaTracker::new(),
            peer_hashes: std::collections::HashMap::new(),
            rounds: 0,
            syncs: 0,
        }
    }

    pub fn state_hash(&self) -> StateHash {
        StateHash::from_state(&self.state)
    }

    fn tick(&mut self) {
        self.clock.increment(&self.node_id);
    }

    pub fn add_constraint(&mut self, id: &str) {
        self.state.add_constraint(id);
        self.tick();
    }

    pub fn record_satisfied(&mut self, count: u64) {
        self.state.record_satisfied(count);
        self.tick();
    }

    pub fn record_violations(&mut self, count: u64) {
        self.state.record_violations(count);
        self.tick();
    }

    pub fn ping(&self) -> GossipMessage {
        GossipMessage::Ping {
            node: self.node_id.clone(),
            state_hash: self.state_hash(),
            clock: self.clock.clone(),
        }
    }

    pub fn receive(&mut self, msg: &GossipMessage) -> GossipResult {
        self.rounds += 1;

        match msg {
            GossipMessage::Ping { node, state_hash, clock } => {
                self.peer_hashes.insert(node.clone(), *state_hash);
                let my_hash = self.state_hash();

                if my_hash == *state_hash {
                    GossipResult {
                        responses: vec![GossipMessage::Ack {
                            node: self.node_id.clone(),
                            clock: self.clock.clone(),
                        }],
                        state_changed: false,
                        converged: true,
                    }
                } else {
                    self.syncs += 1;
                    GossipResult {
                        responses: vec![GossipMessage::Sync {
                            node: self.node_id.clone(),
                            state: self.state.clone(),
                            clock: self.clock.clone(),
                        }],
                        state_changed: false,
                        converged: false,
                    }
                }
            }

            GossipMessage::Sync { node, state, clock } => {
                let old_hash = self.state_hash();
                self.state.merge(state);
                self.clock.merge(clock);
                let new_hash = self.state_hash();
                self.peer_hashes.insert(node.clone(), new_hash);

                GossipResult {
                    responses: vec![GossipMessage::Ack {
                        node: self.node_id.clone(),
                        clock: self.clock.clone(),
                    }],
                    state_changed: old_hash != new_hash,
                    converged: false,
                }
            }

            GossipMessage::Delta { node, delta: _, clock } => {
                self.clock.merge(clock);
                GossipResult {
                    responses: vec![GossipMessage::Ack {
                        node: self.node_id.clone(),
                        clock: self.clock.clone(),
                    }],
                    state_changed: true,
                    converged: false,
                }
            }

            GossipMessage::Ack { node, clock } => {
                self.clock.merge(clock);
                let my_hash = self.state_hash();
                self.peer_hashes.insert(node.clone(), my_hash);
                let all_match = self.peer_hashes.values().all(|h| *h == my_hash);
                GossipResult {
                    responses: vec![],
                    state_changed: false,
                    converged: all_match,
                }
            }
        }
    }

    pub fn rounds(&self) -> u64 { self.rounds }
    pub fn syncs(&self) -> u64 { self.syncs }
    pub fn peer_count(&self) -> usize { self.peer_hashes.len() }
}

/// Run a complete gossip exchange between two nodes.
/// a pings b → b responds → a processes response.
/// Returns true if any state changed.
pub fn exchange(a: &mut GossipNode, b: &mut GossipNode) -> bool {
    let ping = a.ping();
    let r1 = b.receive(&ping);
    let mut changed = false;
    for resp in &r1.responses {
        let r2 = a.receive(resp);
        if r2.state_changed { changed = true; }
    }
    changed
}

impl fmt::Display for GossipNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GossipNode({}, {} constraints, {} peers, {} rounds, hash={})",
            self.node_id,
            self.state.active_constraint_count(),
            self.peer_count(),
            self.rounds,
            self.state_hash())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_nodes_converge_immediately() {
        let a = GossipNode::new("a");
        let mut b = GossipNode::new("b");

        let ping = a.ping();
        let result = b.receive(&ping);
        assert!(result.converged);
    }

    #[test]
    fn test_sync_via_exchange() {
        let mut a = GossipNode::new("a");
        let mut b = GossipNode::new("b");

        a.add_constraint("c1");
        a.record_satisfied(100);

        // a ↔ b exchange: a pings b, b sends sync, a merges b's state
        // b now has a's state? No — b sent ITS state to a.
        // For b to get a's state, need b → a exchange too.
        exchange(&mut a, &mut b);
        // After a→b exchange: b received ping, sent its state (empty) to a.
        // a merged b's empty state. Neither has the other's constraints yet.
        // Wait — that's wrong. Let me trace:
        // 1. a.ping() → Ping{a, hash_a}
        // 2. b.receive(Ping) → hash_a ≠ hash_b → Sync{b, state_b(empty)}
        // 3. a.receive(Sync{b, empty}) → a merges empty → no change
        // Result: a still has c1, b still empty. Need b→a exchange.
        
        exchange(&mut b, &mut a);
        // 1. b.ping() → Ping{b, hash_b}
        // 2. a.receive(Ping) → hash_a ≠ hash_b → Sync{a, state_a(c1)}
        // 3. b.receive(Sync{a, c1}) → b merges → b now has c1!

        assert!(b.state.constraints.contains("c1"));
    }

    #[test]
    fn test_bidirectional_sync() {
        let mut a = GossipNode::new("a");
        let mut b = GossipNode::new("b");

        a.add_constraint("c1");
        b.add_constraint("c2");

        // Two rounds of exchange in both directions
        exchange(&mut a, &mut b);
        exchange(&mut b, &mut a);

        // a sent its state to b (via b→a exchange), b sent to a (via a→b exchange)
        // Actually: a→b gives a b's state. b→a gives b a's state.
        // After a→b: a has b's empty state. After b→a: b has a's state (c1+c2? no, just c1)
        // Let's be explicit:

        assert!(a.state.constraints.contains("c2"),
            "a should have c2 from b's sync");
        assert!(b.state.constraints.contains("c1"),
            "b should have c1 from a's sync");
        assert_eq!(a.state.active_constraint_count(), 2);
        assert_eq!(b.state.active_constraint_count(), 2);
    }

    #[test]
    fn test_three_way_eventual_consistency() {
        let mut a = GossipNode::new("a");
        let mut b = GossipNode::new("b");
        let mut c = GossipNode::new("c");

        a.add_constraint("c1");
        b.add_constraint("c2");
        c.add_constraint("c3");

        // Full mesh: each pair exchanges both ways
        exchange(&mut a, &mut b);
        exchange(&mut b, &mut a);
        exchange(&mut b, &mut c);
        exchange(&mut c, &mut b);
        exchange(&mut a, &mut c);
        exchange(&mut c, &mut a);

        assert_eq!(a.state.active_constraint_count(), 3, "a should have all 3");
        assert_eq!(b.state.active_constraint_count(), 3, "b should have all 3");
        assert_eq!(c.state.active_constraint_count(), 3, "c should have all 3");
    }

    #[test]
    fn test_display() {
        let node = GossipNode::new("forgemaster");
        let s = format!("{}", node);
        assert!(s.contains("forgemaster"));
    }
}
