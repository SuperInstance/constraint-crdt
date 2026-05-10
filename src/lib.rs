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
//! ## What this gives us
//!
//! - `ConstraintState` — a CRDT that tracks which constraints are satisfied
//! - `ConstraintGCounter` — distributed count of satisfied/violated constraints
//! - `ConstraintORSet` — set of applied constraints with conflict-free removal
//! - `EisensteinState` — Eisenstein integer positions as CRDT registers
//! - `FleetTile` — PLATO tile as a CRDT that merges across nodes

pub mod merge;
pub mod state;
pub mod counter;
pub mod orset;
pub mod eisenstein;
pub mod tile;

pub use merge::Merge;
pub use state::ConstraintState;
pub use counter::ConstraintGCounter;
pub use orset::ConstraintORSet;
pub use eisenstein::EisensteinRegister;
pub use tile::FleetTile;
