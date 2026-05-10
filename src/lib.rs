//! # Constraint-CRDT
//!
//! CRDT-backed constraint states for distributed fleet consensus.
//!
//! Extracts the CRDT merge protocol from SmartCRDT and fuses it with
//! constraint theory semantics. Every constraint state is a CRDT that
//! merges without coordination — the mathematical foundation for
//! holonomy-consensus across fleet nodes.
//!
//! ## Core insight
//!
//! CRDTs satisfy three algebraic laws (commutative, associative, idempotent).
//! Constraint satisfaction requires closure under lattice operations.
//! These are THE SAME algebraic structure — a semilattice.
//!
//! ## What you get
//!
//! - `Merge` — the semilattice join trait
//! - `ConstraintState` — composite CRDT combining all sub-CRDTs
//! - `ConstraintGCounter` — distributed satisfaction counting
//! - `PNCounter` — positive/negative counting (decrement support)
//! - `ConstraintORSet` — add-wins constraint tracking with tombstone GC
//! - `EisensteinRegister` — lattice positions as LWW registers
//! - `FleetTile` — PLATO tiles as mergeable CRDTs
//! - `VectorClock` — causal ordering across nodes
//! - `DeltaTracker` / `ConstraintDelta` — send only changes, not full state
//! - `PlatoClient` — HTTP client for PLATO tile server
//!
//! ## Algebraic laws verified
//!
//! Every CRDT type is tested for:
//! - **Commutativity**: `a ∘ b == b ∘ a`
//! - **Associativity**: `(a ∘ b) ∘ c == a ∘ (b ∘ c)`
//! - **Idempotence**: `a ∘ a == a`

pub mod merge;
pub mod state;
pub mod counter;
pub mod pncounter;
pub mod orset;
pub mod eisenstein;
pub mod tile;
pub mod vclock;
pub mod delta;
#[cfg(feature = "plato")]
pub mod plato;

pub use merge::Merge;
pub use state::ConstraintState;
pub use counter::ConstraintGCounter;
pub use pncounter::PNCounter;
pub use orset::ConstraintORSet;
pub use eisenstein::EisensteinRegister;
pub use tile::FleetTile;
pub use vclock::VectorClock;
pub use delta::{ConstraintDelta, DeltaTracker};
#[cfg(feature = "plato")]
pub use plato::PlatoClient;
