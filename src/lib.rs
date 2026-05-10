//! # Constraint-CRDT
//!
//! CRDT-backed constraint states for distributed fleet consensus.
//!
//! ## Core insight
//!
//! CRDTs satisfy three algebraic laws (commutative, associative, idempotent).
//! Constraint satisfaction requires closure under lattice operations.
//! These are THE SAME algebraic structure — a semilattice.
//!
//! ## Modules
//!
//! - `merge` — The semilattice join trait (C/A/I laws)
//! - `state` — Composite CRDT: all sub-CRDTs in one mergeable unit
//! - `counter` — G-Counter: distributed satisfaction counting
//! - `pncounter` — PN-Counter: positive/negative counting
//! - `orset` — OR-Set: add-wins constraint tracking
//! - `eisenstein` — Lattice positions as LWW registers (lower norm wins)
//! - `tile` — PLATO tiles as mergeable CRDTs
//! - `vclock` — Vector clocks for causal ordering
//! - `delta` — Delta-state CRDTs (send only changes)
//! - `merkle` — State hashes for efficient sync detection
//! - `gossip` — Anti-entropy gossip protocol
//! - `simulation` — Deterministic network simulation
//! - `plato` — HTTP client for PLATO server (feature-gated)

pub mod merge;
pub mod state;
pub mod counter;
pub mod pncounter;
pub mod orset;
pub mod eisenstein;
pub mod tile;
pub mod vclock;
pub mod delta;
pub mod merkle;
pub mod gossip;
pub mod simulation;
pub mod bloom;
pub mod geometric;
pub mod decay;
pub mod sketch;
pub mod ttl_crdt;
#[cfg(feature = "plato")]
pub mod plato;

pub use bloom::BloomCRDT;
pub use geometric::{GeometricNode, GossipExperiment as GeometricExperiment};
pub use decay::{DecayCounter, DecayConstraintState};
pub use sketch::SketchCRDT;
pub use ttl_crdt::{TtlCrdtNode, TtlCrdtConstraint, TtlState, TtlType, EmergenceEvent};

pub use merge::Merge;
pub use state::ConstraintState;
pub use counter::ConstraintGCounter;
pub use pncounter::PNCounter;
pub use orset::ConstraintORSet;
pub use eisenstein::EisensteinRegister;
pub use tile::FleetTile;
pub use vclock::VectorClock;
pub use delta::{ConstraintDelta, DeltaTracker};
pub use merkle::StateHash;
pub use gossip::{GossipNode, GossipMessage, exchange as gossip_exchange};
pub use simulation::Simulation;
#[cfg(feature = "plato")]
pub use plato::PlatoClient;
