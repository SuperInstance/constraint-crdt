//! # Fleet Tile — PLATO tile as a CRDT
//!
//! A PLATO tile represents a unit of knowledge contributed by a fleet agent.
//! By making tiles CRDTs, multiple agents can contribute to the same room
//! without coordination — tiles merge automatically.

use crate::merge::Merge;
use crate::counter::ConstraintGCounter;
use crate::orset::ConstraintORSet;
use crate::eisenstein::EisensteinRegister;
use serde::{Deserialize, Serialize};
use std::fmt;

/// A PLATO tile — the fundamental unit of fleet knowledge.
///
/// Each tile has:
/// - An ID (room + position)
/// - Content (the actual knowledge)
/// - A constraint state (what constraints this tile satisfies)
/// - An Eisenstein position (where on the lattice this knowledge lives)
/// - Provenance (which agent contributed it, when)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetTile {
    /// Tile ID (unique within a room)
    pub id: String,
    /// PLATO room this tile belongs to
    pub room: String,
    /// Tile content (markdown, JSON, or raw text)
    pub content: String,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// Constraint satisfaction state for this tile
    pub constraints: ConstraintORSet,
    /// Aggregate metrics for this tile
    pub metrics: ConstraintGCounter,
    /// Position in constraint space (if applicable)
    pub position: Option<EisensteinRegister>,
    /// Agent that created this tile
    pub author: String,
    /// Creation timestamp (epoch ms)
    pub created_at: u64,
    /// Last update timestamp
    pub updated_at: u64,
    /// Merkle hash of content (for integrity)
    pub content_hash: u64,
}

impl FleetTile {
    pub fn new(room: &str, id: &str, content: &str, author: &str) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        Self {
            id: id.to_string(),
            room: room.to_string(),
            content: content.to_string(),
            tags: Vec::new(),
            constraints: ConstraintORSet::new(),
            metrics: ConstraintGCounter::new(),
            position: None,
            author: author.to_string(),
            created_at: now,
            updated_at: now,
            content_hash: Self::hash_content(content),
        }
    }

    /// Add a tag
    pub fn tag(&mut self, tag: &str) {
        if !self.tags.contains(&tag.to_string()) {
            self.tags.push(tag.to_string());
        }
    }

    /// Check if tile has a tag
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }

    /// Simple FNV-1a hash for content integrity
    fn hash_content(content: &str) -> u64 {
        let mut hash: u64 = 0xcbf29ce484222325;
        for byte in content.bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash
    }

    /// Update content (refreshes hash and timestamp)
    pub fn update_content(&mut self, content: &str) {
        self.content = content.to_string();
        self.content_hash = Self::hash_content(content);
        self.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
    }

    /// Verify content integrity
    pub fn verify_integrity(&self) -> bool {
        Self::hash_content(&self.content) == self.content_hash
    }

    /// Serialize to JSON
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

impl Merge for FleetTile {
    fn merge(&mut self, other: &Self) {
        // Must be same tile
        if self.id != other.id || self.room != other.room {
            return;
        }

        // Merge constraints
        self.constraints.merge(&other.constraints);

        // Merge metrics
        self.metrics.merge(&other.metrics);

        // Merge tags (union)
        for tag in &other.tags {
            self.tag(tag);
        }

        // Content: latest wins (by timestamp)
        if other.updated_at > self.updated_at {
            self.content = other.content.clone();
            self.content_hash = other.content_hash;
            self.updated_at = other.updated_at;
        }

        // Position: merge via Eisenstein register rules (lower norm wins)
        match (&mut self.position, &other.position) {
            (Some(mine), Some(theirs)) => mine.merge(theirs),
            (None, Some(theirs)) => self.position = Some(theirs.clone()),
            _ => {}
        }

        // Created at: take earliest
        self.created_at = self.created_at.min(other.created_at);
    }
}

impl fmt::Display for FleetTile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Tile({}/{} by={}, {} bytes, {} constraints, {})",
            self.room, self.id, self.author,
            self.content.len(),
            self.constraints.len(),
            self.metrics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tile_creation() {
        let t = FleetTile::new("fleet-ops", "tile-1", "Hello fleet", "forgemaster");
        assert_eq!(t.id, "tile-1");
        assert_eq!(t.room, "fleet-ops");
        assert!(t.verify_integrity());
    }

    #[test]
    fn test_content_integrity() {
        let mut t = FleetTile::new("test", "1", "original", "a");
        assert!(t.verify_integrity());
        t.update_content("modified");
        assert!(t.verify_integrity());
        // Tamper with content
        let mut t2 = t.clone();
        t2.content = "tampered".to_string();
        assert!(!t2.verify_integrity());
    }

    #[test]
    fn test_tagging() {
        let mut t = FleetTile::new("test", "1", "x", "a");
        t.tag("critical");
        t.tag("verified");
        t.tag("critical"); // duplicate
        assert_eq!(t.tags.len(), 2);
        assert!(t.has_tag("critical"));
    }

    #[test]
    fn test_merge_tiles() {
        let mut a = FleetTile::new("room", "1", "content-a", "agent-a");
        a.constraints.add("bounds", "agent-a");
        a.metrics.record_satisfied("agent-a", 100);

        let mut b = FleetTile::new("room", "1", "content-b", "agent-b");
        b.constraints.add("norm", "agent-b");
        b.metrics.record_satisfied("agent-b", 200);

        a.merge(&b);

        // Both constraints should be present
        assert!(a.constraints.contains("bounds"));
        assert!(a.constraints.contains("norm"));

        // Both metrics merged
        assert_eq!(a.metrics.total_satisfied(), 300);
    }

    #[test]
    fn test_merge_different_ids_ignored() {
        let mut a = FleetTile::new("room", "1", "a", "a");
        let b = FleetTile::new("room", "2", "b", "b");
        a.merge(&b);
        assert_eq!(a.content, "a"); // Unchanged
    }

    #[test]
    fn test_json_roundtrip() {
        let mut t = FleetTile::new("room", "1", "test content", "agent");
        t.tag("important");
        t.constraints.add("bounds", "node-a");
        let json = t.to_json();
        assert!(json.contains("room"));
        assert!(json.contains("important"));
    }
}
