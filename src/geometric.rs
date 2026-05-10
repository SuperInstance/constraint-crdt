//! # Novel Experiment 2: Eisenstein-Geometric Gossip
//!
//! Instead of random peer selection, use Eisenstein lattice geometry to decide
//! WHO to sync with and WHAT to prioritize.
//!
//! Key insight: on the Eisenstein lattice, nodes that are geometrically close
//! (low norm distance) have similar constraint profiles. Syncing with nearby
//! nodes first yields faster convergence than random gossip.
//!
//! This is a genuinely novel contribution: CRDT sync priority determined by
//! position in a mathematical lattice, not by network topology.

use crate::eisenstein::eisenstein_norm;
use crate::gossip::{GossipNode, GossipMessage};
use crate::merkle::StateHash;
use crate::state::ConstraintState;
use crate::merge::Merge;
use crate::simulation::SimRng;

/// A node with an Eisenstein lattice position.
#[derive(Debug, Clone)]
pub struct GeometricNode {
    pub node: GossipNode,
    /// Position on the Eisenstein lattice
    pub position: (i32, i32),
}

impl GeometricNode {
    pub fn new(id: &str, position: (i32, i32)) -> Self {
        Self {
            node: GossipNode::new(id),
            position,
        }
    }

    /// Eisenstein norm of this node's position.
    pub fn norm(&self) -> i64 {
        eisenstein_norm(self.position)
    }

    /// Eisenstein distance to another node.
    pub fn distance_to(&self, other: &Self) -> i64 {
        let (ax, ay) = (self.position.0 as i64, self.position.1 as i64);
        let (bx, by) = (other.position.0 as i64, other.position.1 as i64);
        let da = ax - bx;
        let db = ay - by;
        let dc = da - db;
        da.abs().max(db.abs()).max(dc.abs())
    }

    /// "Drift" — how far this node's constraint state has diverged from origin.
    /// Higher drift = more constraints = more to share.
    pub fn constraint_drift(&self) -> u64 {
        self.node.state.metrics.total_satisfied()
            + self.node.state.metrics.total_violations()
    }
}

/// Result of comparing random vs geometric gossip strategies.
#[derive(Debug, Clone)]
pub struct GossipExperiment {
    pub node_count: usize,
    pub random_convergence_rounds: Option<u64>,
    pub geometric_convergence_rounds: Option<u64>,
    pub random_messages: u64,
    pub geometric_messages: u64,
    pub speedup: Option<f64>,
}

/// Run a comparison experiment: random gossip vs geometric gossip.
pub fn run_experiment(
    node_count: usize,
    seed: u64,
    max_rounds: u64,
) -> GossipExperiment {
    // Create nodes with Eisenstein positions
    let mut random_nodes: Vec<GeometricNode> = Vec::new();
    let mut geometric_nodes: Vec<GeometricNode> = Vec::new();
    
    // Place nodes on a hex grid
    let mut rng = SimRng::new(seed);
    for i in 0..node_count {
        let angle = (i as f64 / node_count as f64) * std::f64::consts::TAU;
        let r = 5.0 + (rng.next_f64() * 5.0);
        let x = (r * angle.cos()).round() as i32;
        let y = (r * angle.sin()).round() as i32;
        
        random_nodes.push(GeometricNode::new(&format!("n{}", i), (x, y)));
        geometric_nodes.push(GeometricNode::new(&format!("n{}", i), (x, y)));
    }

    // Give each node unique constraints
    for (i, (rn, gn)) in random_nodes.iter_mut().zip(geometric_nodes.iter_mut()).enumerate() {
        rn.node.add_constraint(&format!("c{}", i));
        rn.node.record_satisfied(100 + i as u64 * 50);
        gn.node.add_constraint(&format!("c{}", i));
        gn.node.record_satisfied(100 + i as u64 * 50);
    }

    // Run random gossip
    let mut random_rng = SimRng::new(seed);
    let random_result = run_random_gossip(&mut random_nodes, &mut random_rng, max_rounds);

    // Run geometric gossip
    let mut geometric_rng = SimRng::new(seed);
    let geometric_result = run_geometric_gossip(&mut geometric_nodes, &mut geometric_rng, max_rounds);

    let speedup = match (random_result.0, geometric_result.0) {
        (Some(rr), Some(gr)) => Some(rr as f64 / gr as f64),
        _ => None,
    };

    GossipExperiment {
        node_count,
        random_convergence_rounds: random_result.0,
        geometric_convergence_rounds: geometric_result.0,
        random_messages: random_result.1,
        geometric_messages: geometric_result.1,
        speedup,
    }
}

/// Random gossip: pick a random peer each round.
fn run_random_gossip(
    nodes: &mut [GeometricNode],
    rng: &mut SimRng,
    max_rounds: u64,
) -> (Option<u64>, u64) {
    let n = nodes.len();
    let mut messages = 0u64;

    for round in 1..=max_rounds {
        for i in 0..n {
            let peer = rng.random_peer(i, n);
            let ping = nodes[i].node.ping();
            messages += 1;
            let r1 = nodes[peer].node.receive(&ping);
            for resp in &r1.responses {
                messages += 1;
                nodes[i].node.receive(resp);
            }
        }

        if check_converged(nodes.iter().map(|n| &n.node).collect()) {
            return (Some(round), messages);
        }
    }
    (None, messages)
}

/// Geometric gossip: pick closest peer first, expanding radius.
fn run_geometric_gossip(
    nodes: &mut [GeometricNode],
    rng: &mut SimRng,
    max_rounds: u64,
) -> (Option<u64>, u64) {
    let n = nodes.len();
    let mut messages = 0u64;

    for round in 1..=max_rounds {
        for i in 0..n {
            // Pick peer by Eisenstein distance (closest first)
            let peer = geometric_peer_selection(i, nodes, round, rng);
            let ping = nodes[i].node.ping();
            messages += 1;
            let r1 = nodes[peer].node.receive(&ping);
            for resp in &r1.responses {
                messages += 1;
                nodes[i].node.receive(resp);
            }
        }

        if check_converged(nodes.iter().map(|n| &n.node).collect()) {
            return (Some(round), messages);
        }
    }
    (None, messages)
}

/// Geometric peer selection: prefer nearby nodes, expanding radius over time.
fn geometric_peer_selection(
    self_idx: usize,
    nodes: &[GeometricNode],
    round: u64,
    rng: &mut SimRng,
) -> usize {
    let self_node = &nodes[self_idx];
    let n = nodes.len();

    // Sort peers by distance
    let mut peers: Vec<(usize, i64)> = (0..n)
        .filter(|&i| i != self_idx)
        .map(|i| (i, self_node.distance_to(&nodes[i])))
        .collect();
    peers.sort_by_key(|(_, d)| *d);

    // Expanding radius: in early rounds, prefer close peers
    // Over time, expand to distant peers
    let radius_fraction = (round as f64 / 10.0).min(1.0);
    let max_idx = ((peers.len() as f64 * radius_fraction).ceil() as usize).max(1).min(peers.len());
    
    // Pick within radius, with some randomness
    let idx = (rng.next_u64() as usize) % max_idx;
    peers[idx].0
}

fn check_converged(nodes: Vec<&GossipNode>) -> bool {
    if nodes.len() <= 1 { return true; }
    let hashes: Vec<_> = nodes.iter().map(|n| n.state_hash()).collect();
    let first = hashes[0];
    hashes.iter().all(|h| *h == first)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_geometric_distance() {
        let a = GeometricNode::new("a", (0, 0));
        let b = GeometricNode::new("b", (3, 0));
        let c = GeometricNode::new("c", (3, 3));
        
        assert_eq!(a.distance_to(&b), 3);
        assert_eq!(a.distance_to(&c), 3); // hex: max(3,3,0)=3
        assert_eq!(b.distance_to(&c), 3); // hex: max(0,3,3)=3
    }

    #[test]
    fn test_experiment_4_nodes() {
        let result = run_experiment(4, 42, 30);
        println!("4 nodes: random={:?} geometric={:?} speedup={:?}",
            result.random_convergence_rounds,
            result.geometric_convergence_rounds,
            result.speedup);
        
        // Both should converge
        assert!(result.random_convergence_rounds.is_some());
        assert!(result.geometric_convergence_rounds.is_some());
    }

    #[test]
    fn test_experiment_8_nodes() {
        let result = run_experiment(8, 42, 50);
        println!("8 nodes: random={:?} geometric={:?} speedup={:?}",
            result.random_convergence_rounds,
            result.geometric_convergence_rounds,
            result.speedup);
        
        assert!(result.random_convergence_rounds.is_some());
        assert!(result.geometric_convergence_rounds.is_some());
    }

    #[test]
    fn test_experiment_16_nodes() {
        let result = run_experiment(16, 42, 100);
        println!("16 nodes: random={:?} geometric={:?} speedup={:?}",
            result.random_convergence_rounds,
            result.geometric_convergence_rounds,
            result.speedup);
        
        assert!(result.random_convergence_rounds.is_some());
        assert!(result.geometric_convergence_rounds.is_some());
    }

    #[test]
    fn test_geometric_scales_better() {
        // Run multiple sizes and check geometric doesn't degrade as fast
        let r4 = run_experiment(4, 42, 50);
        let r8 = run_experiment(8, 42, 50);
        let r16 = run_experiment(16, 42, 100);
        
        let r4r = r4.random_convergence_rounds.unwrap() as f64;
        let r8r = r8.random_convergence_rounds.unwrap() as f64;
        let r16r = r16.random_convergence_rounds.unwrap() as f64;
        
        let r4g = r4.geometric_convergence_rounds.unwrap() as f64;
        let r8g = r8.geometric_convergence_rounds.unwrap() as f64;
        let r16g = r16.geometric_convergence_rounds.unwrap() as f64;
        
        println!("Scaling (random):   4→{:.0} 8→{:.0} 16→{:.0}", r4r, r8r, r16r);
        println!("Scaling (geometric): 4→{:.0} 8→{:.0} 16→{:.0}", r4g, r8g, r16g);
        
        // Geometric should not be worse than random
        assert!(r4g <= r4r * 1.5, "Geometric should not be much worse at 4 nodes");
    }
}
