//! # Merkle State Hash
//!
//! Content-addressed state hashes for efficient "what changed?" detection.
//! Instead of sending full state to compare, nodes exchange 32-byte hashes.
//! If hashes match, skip the sync entirely.

use crate::state::ConstraintState;
use crate::tile::FleetTile;
use crate::vclock::VectorClock;
use serde::{Deserialize, Serialize};
use std::fmt;

/// A 32-byte SHA-256-style hash (we use FNV-1a for speed, no crypto needed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StateHash(pub [u8; 32]);

impl StateHash {
    /// Hash a ConstraintState.
    pub fn from_state(state: &ConstraintState) -> Self {
        let mut hasher = FnvHasher::new();
        // Hash constraints (sorted for determinism)
        let mut constraints = state.constraints.active_constraints();
        constraints.sort();
        for c in &constraints {
            hasher.update(c.as_bytes());
        }
        // Hash metrics
        hasher.update(&state.metrics.total_satisfied().to_le_bytes());
        hasher.update(&state.metrics.total_violations().to_le_bytes());
        // Hash position
        hasher.update(&state.position.position.0.to_le_bytes());
        hasher.update(&state.position.position.1.to_le_bytes());
        hasher.update(&state.position.norm.to_le_bytes());
        // Hash version
        hasher.update(&state.version.to_le_bytes());
        StateHash(hasher.finalize())
    }

    /// Hash a FleetTile.
    pub fn from_tile(tile: &FleetTile) -> Self {
        let mut hasher = FnvHasher::new();
        hasher.update(tile.room.as_bytes());
        hasher.update(tile.id.as_bytes());
        hasher.update(&tile.content_hash.to_le_bytes());
        hasher.update(&tile.updated_at.to_le_bytes());
        StateHash(hasher.finalize())
    }

    /// Hash a vector clock.
    pub fn from_clock(clock: &VectorClock) -> Self {
        let mut hasher = FnvHasher::new();
        let mut nodes: Vec<_> = clock.nodes();
        nodes.sort();
        for node in &nodes {
            hasher.update(node.as_bytes());
            hasher.update(&clock.get(node).to_le_bytes());
        }
        StateHash(hasher.finalize())
    }

    /// Zero hash (empty/initial state).
    pub fn zero() -> Self {
        StateHash([0u8; 32])
    }

    /// Is this the zero hash?
    pub fn is_zero(&self) -> bool {
        self.0 == [0u8; 32]
    }

    /// Hex representation.
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

impl fmt::Display for StateHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}…{}", &self.to_hex()[..8], &self.to_hex()[24..32])
    }
}

/// FNV-1a 256-bit hasher (non-cryptographic, deterministic).
struct FnvHasher {
    state: [u64; 4],
}

impl FnvHasher {
    fn new() -> Self {
        // FNV-1a offset basis spread across 4 lanes
        Self {
            state: [
                0xcbf29ce484222325,
                0x100000001b3,
                0x9e3779b97f4a7c15,
                0x6c62272e07bb0142,
            ],
        }
    }

    fn update(&mut self, data: &[u8]) {
        for &byte in data {
            // FNV-1a: XOR then multiply
            self.state[0] ^= byte as u64;
            self.state[0] = self.state[0].wrapping_mul(0x100000001b3);
            self.state[1] ^= byte as u64;
            self.state[1] = self.state[1].wrapping_mul(0x100000001b3).wrapping_add(1);
            self.state[2] ^= byte as u64;
            self.state[2] = self.state[2].wrapping_mul(0x100000001b3).wrapping_add(2);
            self.state[3] ^= byte as u64;
            self.state[3] = self.state[3].wrapping_mul(0x100000001b3).wrapping_add(3);
        }
    }

    fn finalize(&self) -> [u8; 32] {
        let mut result = [0u8; 32];
        for (i, lane) in self.state.iter().enumerate() {
            let bytes = lane.to_le_bytes();
            result[i * 8..(i + 1) * 8].copy_from_slice(&bytes);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_hash_deterministic() {
        let mut s = ConstraintState::new("test");
        s.add_constraint("c1");
        s.record_satisfied(100);
        let h1 = StateHash::from_state(&s);
        let h2 = StateHash::from_state(&s);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_state_hash_changes_on_mutation() {
        let mut s = ConstraintState::new("test");
        let h1 = StateHash::from_state(&s);
        s.add_constraint("c1");
        let h2 = StateHash::from_state(&s);
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_state_hash_order_independent() {
        let mut a = ConstraintState::new("a");
        a.add_constraint("c2");
        a.add_constraint("c1");
        a.record_satisfied(100);

        let mut b = ConstraintState::new("b");
        b.add_constraint("c1");
        b.add_constraint("c2");
        b.record_satisfied(100);

        // Same constraints, same metrics, same position → same hash
        assert_eq!(StateHash::from_state(&a), StateHash::from_state(&b));
    }

    #[test]
    fn test_zero_hash() {
        let z = StateHash::zero();
        assert!(z.is_zero());
    }

    #[test]
    fn test_display() {
        let mut s = ConstraintState::new("test");
        let h = StateHash::from_state(&s);
        let display = format!("{}", h);
        assert!(display.contains("…"));
        assert_eq!(display.chars().count(), 17); // 8 + … + 8 chars
    }

    #[test]
    fn test_tile_hash() {
        let t = FleetTile::new("room", "1", "content", "author");
        let h = StateHash::from_tile(&t);
        assert!(!h.is_zero());
    }

    #[test]
    fn test_clock_hash() {
        let mut vc = VectorClock::new();
        vc.increment("a");
        let h = StateHash::from_clock(&vc);
        assert!(!h.is_zero());
    }
}
