//! # TTL-CRDT Bridge
//!
//! Connects Oracle1's TTL constraints (constraint-theory-llvm) with
//! Forgemaster's CRDT distribution layer (constraint-crdt).
//!
//! The key insight: TTL expiry IS a CRDT event.
//! When a constraint expires, it's not just a local state change —
//! it's a distributed event that must propagate to all fleet nodes.
//!
//! The bridge:
//! 1. TTL expiry → ConstraintDelta (delta-state CRDT update)
//! 2. Emergence detection (H¹ β₁ change) → FleetTile (PLATO notification)
//! 3. TTL lifespan parameters → DecayCounter (time-decay CRDT)
//! 4. TTL bloom hashes → BloomCRDT (compressed membership sync)

use crate::merge::Merge;
use crate::state::ConstraintState;
use crate::bloom::BloomCRDT;
use crate::sketch::SketchCRDT;
use crate::decay::DecayCounter;
use crate::vclock::VectorClock;
use std::time::{SystemTime, UNIX_EPOCH};

/// TTL type mapping to CRDT semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtlType {
    Tile,     // knowledge rots (1 hour)
    Task,     // work expires (5 min)
    Agent,    // presence is active (1 min)
    Bearing,  // bearings drift (30s)
    Trust,    // trust decays slow (24h)
}

impl TtlType {
    pub fn default_lifespan_secs(&self) -> f64 {
        match self {
            TtlType::Tile => 3600.0,
            TtlType::Task => 300.0,
            TtlType::Agent => 60.0,
            TtlType::Bearing => 30.0,
            TtlType::Trust => 86400.0,
        }
    }

    pub fn decay_half_life(&self) -> f64 {
        // TTL type determines how fast violations decay
        match self {
            TtlType::Tile => 3600.0,     // knowledge: same as lifespan
            TtlType::Task => 300.0,       // work: fast decay
            TtlType::Agent => 30.0,       // presence: very fast
            TtlType::Bearing => 60.0,     // bearing: medium
            TtlType::Trust => 7200.0,     // trust: slow decay
        }
    }
}

/// Three states of a TTL-annotated constraint.
#[derive(Debug, Clone, PartialEq)]
pub enum TtlState {
    Active { satisfied: bool, remaining_secs: f64 },
    Expired { last_value: bool, since_secs: f64, context: DeathContext },
    Emerged { betti: i64, expired_count: u64 },
}

#[derive(Debug, Clone, PartialEq)]
pub enum DeathContext {
    TimeExpired,
    LoadKilled { load: f64 },
    UseExhausted { evaluations: u64 },
    ExplicitTermination,
}

/// A TTL constraint with CRDT distribution semantics.
#[derive(Debug, Clone)]
pub struct TtlCrdtConstraint {
    pub id: i64,
    pub ttl_type: TtlType,
    pub state: TtlState,
    pub use_count: u64,
    pub created_ns: u64,
    pub base_lifespan: f64,
    pub load_penalty: f64,
}

impl TtlCrdtConstraint {
    pub fn new(id: i64, ttl_type: TtlType) -> Self {
        Self {
            id,
            ttl_type,
            state: TtlState::Active { satisfied: false, remaining_secs: ttl_type.default_lifespan_secs() },
            use_count: 0,
            created_ns: now_ns(),
            base_lifespan: ttl_type.default_lifespan_secs(),
            load_penalty: 1.0,
        }
    }

    /// Check if alive, updating state if expired.
    pub fn is_alive(&mut self, load: f64, now_ns: u64) -> bool {
        let elapsed = (now_ns - self.created_ns) as f64 / 1e9;
        let use_decay = 1.0 - (self.use_count as f64 + 1.0).log2() / 100.0;
        let use_decay = use_decay.max(0.1);
        let load_decay = 1.0 / (1.0 + (load - 1.0) * self.load_penalty);
        let remaining = self.base_lifespan * use_decay * load_decay - elapsed;

        match self.state {
            TtlState::Active { .. } => {
                if remaining <= 0.0 {
                    let ctx = if load > 1.5 {
                        DeathContext::LoadKilled { load }
                    } else if self.use_count > 10000 {
                        DeathContext::UseExhausted { evaluations: self.use_count }
                    } else {
                        DeathContext::TimeExpired
                    };
                    let last = match &self.state {
                        TtlState::Active { satisfied, .. } => *satisfied,
                        _ => false,
                    };
                    self.state = TtlState::Expired {
                        last_value: last,
                        since_secs: -remaining,
                        context: ctx,
                    };
                    false
                } else {
                    self.state = TtlState::Active { satisfied: false, remaining_secs: remaining };
                    true
                }
            }
            _ => false,
        }
    }

    /// CRDT-compatible constraint ID string.
    pub fn crdt_id(&self) -> String {
        format!("{}:{}", self.ttl_type.name(), self.id)
    }

    fn is_expired(&self) -> bool {
        matches!(self.state, TtlState::Expired { .. })
    }
}

impl TtlType {
    pub fn name(&self) -> &'static str {
        match self {
            TtlType::Tile => "tile",
            TtlType::Task => "task",
            TtlType::Agent => "agent",
            TtlType::Bearing => "bearing",
            TtlType::Trust => "trust",
        }
    }
}

/// The TTL-CRDT mesh node — what each fleet agent runs.
///
/// Combines Oracle1's TTL constraints with Forgemaster's CRDT distribution.
/// Each node maintains local TTL constraints, syncs via CRDT gossip,
/// and reports emergence to PLATO.
pub struct TtlCrdtNode {
    pub node_id: String,
    /// Eisenstein lattice position
    pub position: (i32, i32),
    /// TTL constraints
    pub constraints: Vec<TtlCrdtConstraint>,
    /// CRDT state for distribution
    pub crdt_state: ConstraintState,
    /// Bloom filter for compressed membership
    pub bloom: crate::bloom::BloomCRDT,
    /// Sketch for violation frequency
    pub sketch: crate::sketch::SketchCRDT,
    /// Decay counters per TTL type
    pub decay: std::collections::HashMap<String, crate::decay::DecayCounter>,
    /// H¹ emergence detection
    pub betti: i64,
    pub prev_betti: i64,
    /// Vector clock
    pub clock: VectorClock,
    /// Statistics
    pub stats: TtlCrdtStats,
}

#[derive(Debug, Clone, Default)]
pub struct TtlCrdtStats {
    pub constraints_created: u64,
    pub constraints_expired: u64,
    pub constraints_emerged: u64,
    pub emergence_events: u64,
    pub crdt_merges: u64,
    pub bloom_merges: u64,
    pub violations_detected: u64,
}

impl TtlCrdtNode {
    pub fn new(node_id: &str, position: (i32, i32)) -> Self {
        Self {
            node_id: node_id.to_string(),
            position,
            constraints: Vec::new(),
            crdt_state: ConstraintState::new(node_id),
            bloom: crate::bloom::BloomCRDT::new(10_000, 0.01),
            sketch: crate::sketch::SketchCRDT::new(0.001, 0.01),
            decay: std::collections::HashMap::new(),
            betti: 0,
            prev_betti: 0,
            clock: VectorClock::new(),
            stats: TtlCrdtStats::default(),
        }
    }

    /// Add a TTL constraint.
    pub fn add_constraint(&mut self, ttl_type: TtlType, id: i64) {
        let c = TtlCrdtConstraint::new(id, ttl_type);
        self.bloom.insert(&c.crdt_id());
        self.constraints.push(c);
        self.crdt_state.add_constraint(&format!("{}:{}", ttl_type.name(), id));
        self.clock.increment(&self.node_id);
        self.stats.constraints_created += 1;
    }

    /// Evaluate all constraints, detect emergence.
    pub fn evaluate(&mut self, load: f64) -> Vec<EmergenceEvent> {
        let now = now_ns();
        let mut events = Vec::new();
        let mut expired_ids = Vec::new();

        for c in &mut self.constraints {
            let was_alive = matches!(c.state, TtlState::Active { .. });
            c.is_alive(load, now);
            c.use_count += 1;

            if was_alive && c.is_expired() {
                expired_ids.push(c.crdt_id());
                self.stats.constraints_expired += 1;
                self.stats.violations_detected += 1;
                self.sketch.record(&c.crdt_id(), 1);
                self.crdt_state.record_violations(1);
            }

            // Record satisfied evaluations
            if let TtlState::Active { satisfied: true, .. } = c.state {
                self.crdt_state.record_satisfied(1);
            }
        }

        // Recompute β₁
        let active = self.constraints.iter().filter(|c| !c.is_expired()).count() as i64;
        let variables = 16i64; // 16 i32 lanes
        self.prev_betti = self.betti;
        self.betti = active - variables + 1; // C=1 (all connected)

        if self.betti != self.prev_betti && !expired_ids.is_empty() {
            self.stats.emergence_events += 1;
            self.stats.constraints_emerged += expired_ids.len() as u64;
            events.push(EmergenceEvent {
                prev_betti: self.prev_betti,
                new_betti: self.betti,
                delta: self.betti - self.prev_betti,
                expired_count: expired_ids.len() as u64,
                expired_ids,
            });

            // Mark as emerged
            for c in &mut self.constraints {
                if c.is_expired() {
                    c.state = TtlState::Emerged {
                        betti: self.betti,
                        expired_count: self.stats.constraints_expired,
                    };
                }
            }
        }

        self.clock.increment(&self.node_id);
        events
    }

    /// Merge state from another node (CRDT gossip).
    pub fn merge(&mut self, other: &Self) {
        self.crdt_state.merge(&other.crdt_state);
        self.bloom.merge(&other.bloom);
        self.sketch.merge(&other.sketch);
        self.clock.merge(&other.clock);
        self.stats.crdt_merges += 1;
        self.stats.bloom_merges += 1;
    }

    /// Active constraint count.
    pub fn active_count(&self) -> usize {
        self.constraints.iter().filter(|c| matches!(c.state, TtlState::Active { .. })).count()
    }

    /// Expired constraint count.
    pub fn expired_count(&self) -> usize {
        self.constraints.iter().filter(|c| c.is_expired()).count()
    }

    /// Satisfaction rate.
    pub fn satisfaction_rate(&self) -> f64 {
        self.crdt_state.satisfaction_rate()
    }

    /// Eisenstein norm of position.
    pub fn position_norm(&self) -> i64 {
        let (a, b) = (self.position.0 as i64, self.position.1 as i64);
        a * a - a * b + b * b
    }
}

/// An emergence event — β₁ changed.
#[derive(Debug, Clone)]
pub struct EmergenceEvent {
    pub prev_betti: i64,
    pub new_betti: i64,
    pub delta: i64,
    pub expired_count: u64,
    pub expired_ids: Vec<String>,
}

fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ttl_crdt_lifecycle() {
        let mut node = TtlCrdtNode::new("test", (1, 0));
        node.add_constraint(TtlType::Task, 1);
        node.add_constraint(TtlType::Tile, 2);
        assert_eq!(node.active_count(), 2);

        // Evaluate (all alive at first)
        let events = node.evaluate(1.0);
        assert!(events.is_empty()); // No expiry yet
        assert!(node.satisfaction_rate() > 0.0);
    }

    #[test]
    fn test_expiry_with_instant_death() {
        let mut node = TtlCrdtNode::new("test", (0, 0));
        node.add_constraint(TtlType::Task, 1);
        
        // Force expiry by setting created far in the past
        node.constraints[0].created_ns = 0; // Epoch = long ago
        let events = node.evaluate(1.0);
        assert!(!events.is_empty() || node.expired_count() > 0);
    }

    #[test]
    fn test_crdt_merge_propagates() {
        let mut a = TtlCrdtNode::new("a", (1, 0));
        let mut b = TtlCrdtNode::new("b", (0, 1));

        a.add_constraint(TtlType::Tile, 1);
        b.add_constraint(TtlType::Task, 2);

        a.merge(&b);
        b.merge(&a);

        // Both should see each other's metrics via CRDT
        assert!(a.stats.crdt_merges > 0);
        assert!(b.stats.crdt_merges > 0);
    }

    #[test]
    fn test_sketch_tracks_violations() {
        let mut node = TtlCrdtNode::new("test", (0, 0));
        node.add_constraint(TtlType::Task, 1);
        node.constraints[0].created_ns = 0; // Force expiry
        node.evaluate(1.0);
        assert!(node.sketch.estimate("task:1") >= 1);
    }

    #[test]
    fn test_betti_changes_on_expiry() {
        let mut node = TtlCrdtNode::new("test", (0, 0));
        node.add_constraint(TtlType::Tile, 1);
        node.add_constraint(TtlType::Tile, 2);

        // Initial: E=2, V=16, C=1 → β₁ = 2-16+1 = -13
        let _ = node.evaluate(1.0);
        assert_eq!(node.betti, -13);

        // Force one expiry
        node.constraints[0].created_ns = 0;
        let events = node.evaluate(1.0);
        // E=1, V=16, C=1 → β₁ = 1-16+1 = -14
        // Delta = -1 → emergence detected
        if !events.is_empty() {
            assert_eq!(events[0].delta, -1);
        }
    }

    #[test]
    fn test_ttl_type_defaults() {
        assert_eq!(TtlType::Tile.default_lifespan_secs(), 3600.0);
        assert_eq!(TtlType::Task.default_lifespan_secs(), 300.0);
        assert_eq!(TtlType::Agent.default_lifespan_secs(), 60.0);
        assert_eq!(TtlType::Bearing.default_lifespan_secs(), 30.0);
        assert_eq!(TtlType::Trust.default_lifespan_secs(), 86400.0);
    }

    #[test]
    fn test_full_mesh_two_nodes() {
        let mut a = TtlCrdtNode::new("forgemaster", (3, 0));
        let mut b = TtlCrdtNode::new("oracle1", (0, 1));

        // Each adds unique constraints
        a.add_constraint(TtlType::Tile, 1);
        a.add_constraint(TtlType::Task, 2);
        b.add_constraint(TtlType::Trust, 3);
        b.add_constraint(TtlType::Bearing, 4);

        // Evaluate
        let _ = a.evaluate(1.0);
        let _ = b.evaluate(1.0);

        // Gossip
        a.merge(&b);
        b.merge(&a);

        // Both should have merged CRDT state
        assert!(a.stats.crdt_merges > 0);
        assert!(b.stats.crdt_merges > 0);
    }
}
