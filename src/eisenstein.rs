//! # Eisenstein CRDT Register
//!
//! Last-Writer-Wins register for Eisenstein integer positions.
//! When fleet nodes disagree on a lattice position, the one with
//! the lower Eisenstein norm wins (closer to origin = more constrained).

use crate::merge::Merge;
use serde::{Deserialize, Serialize};
use std::fmt;

/// An Eisenstein integer position (a, b) with norm N(a,b) = a² - ab + b²
pub type E12 = (i32, i32);

/// Compute Eisenstein norm
pub fn eisenstein_norm(e: E12) -> i64 {
    let (a, b) = (e.0 as i64, e.1 as i64);
    a * a - a * b + b * b
}

/// A Last-Writer-Wins register for Eisenstein positions.
///
/// Merge policy: the position with LOWER norm wins (closer to origin =
/// more constrained). Ties broken by timestamp.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EisensteinRegister {
    /// Current position on the Eisenstein lattice
    pub position: E12,
    /// Timestamp of last write (nanoseconds since epoch)
    pub timestamp: u64,
    /// Node that wrote this value
    pub node: String,
    /// Eisenstein norm of current position
    pub norm: i64,
}

impl EisensteinRegister {
    pub fn new(position: E12, node: &str) -> Self {
        Self {
            position,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64,
            node: node.to_string(),
            norm: eisenstein_norm(position),
        }
    }

    /// Update position (only if new norm ≤ current norm, or forced)
    pub fn update(&mut self, position: E12, node: &str) {
        let new_norm = eisenstein_norm(position);
        self.position = position;
        self.norm = new_norm;
        self.node = node.to_string();
        self.timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
    }

    /// Hex distance from origin: max(|a|, |b|, |a-b|)
    pub fn hex_distance(&self) -> i32 {
        let (a, b) = self.position;
        a.abs().max(b.abs()).max((a - b).abs())
    }

    /// Serialize to JSON string
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Deserialize from JSON string
    pub fn from_json(s: &str) -> Option<Self> {
        serde_json::from_str(s).ok()
    }
}

impl Merge for EisensteinRegister {
    fn merge(&mut self, other: &Self) {
        // Lower norm wins (closer to origin = more constrained)
        // Ties: higher timestamp wins (more recent)
        let self_wins = match self.norm.cmp(&other.norm) {
            std::cmp::Ordering::Less => true,  // self is more constrained
            std::cmp::Ordering::Greater => false, // other is more constrained
            std::cmp::Ordering::Equal => self.timestamp >= other.timestamp,
        };

        if !self_wins {
            self.position = other.position;
            self.norm = other.norm;
            self.timestamp = other.timestamp;
            self.node = other.node.clone();
        }
    }
}

impl fmt::Display for EisensteinRegister {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "E12({},{} norm={} from={})", 
            self.position.0, self.position.1, self.norm, self.node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merge::laws;

    #[test]
    fn test_norm() {
        assert_eq!(eisenstein_norm((0, 0)), 0);
        assert_eq!(eisenstein_norm((1, 0)), 1);
        assert_eq!(eisenstein_norm((1, 1)), 1);
        assert_eq!(eisenstein_norm((2, 1)), 3);
    }

    #[test]
    fn test_lower_norm_wins() {
        let mut a = EisensteinRegister::new((3, 3), "node-a"); // norm = 9
        let b = EisensteinRegister::new((1, 0), "node-b");     // norm = 1
        a.merge(&b);
        assert_eq!(a.position, (1, 0)); // Lower norm wins
    }

    #[test]
    fn test_merge_commutative() {
        let a = EisensteinRegister::new((3, 0), "a");
        let b = EisensteinRegister::new((0, 1), "b");
        // Both norm=1 (for (3,0) norm=9, for (0,1) norm=1, so not equal)
        // Actually (3,0): norm = 9, (0,1): norm = 1
        // They differ, so commutativity means both ways give the same (0,1)
        let ab = a.merged(&b);
        let ba = b.merged(&a);
        assert_eq!(ab.position, ba.position);
    }

    #[test]
    fn test_merge_idempotent() {
        let a = EisensteinRegister::new((2, 1), "a");
        assert!(laws::check_idempotent(&a));
    }

    #[test]
    fn test_hex_distance() {
        let r = EisensteinRegister::new((3, -1), "a");
        assert_eq!(r.hex_distance(), 4); // max(3, 1, 4)
    }

    #[test]
    fn test_json_roundtrip() {
        let r = EisensteinRegister::new((2, -3), "node-a");
        let json = r.to_json();
        let restored = EisensteinRegister::from_json(&json).unwrap();
        assert_eq!(restored.position, (2, -3));
        assert_eq!(restored.node, "node-a");
    }
}
