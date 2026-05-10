//! # CRDT Merge Trait
//!
//! The fundamental algebraic structure underlying both CRDTs and constraint lattices:
//! a **semilattice**. Every CRDT merge must be:
//! - **Commutative**: a.merge(b) == b.merge(a)
//! - **Associative**: a.merge(b).merge(c) == a.merge(b.merge(c))
//! - **Idempotent**: a.merge(a) == a
//!
//! These are EXACTLY the properties needed for distributed constraint satisfaction.

use std::fmt::Debug;

/// The fundamental merge operation — a bounded semilattice join.
///
/// This trait unifies CRDT merges, constraint lattice joins,
/// and PLATO tile reconciliation under one algebraic structure.
pub trait Merge: Clone + Debug {
    /// Merge another state into this one (in-place).
    ///
    /// Postconditions:
    /// - `a.merge(b)` ≡ `b.merge(a)` (commutative)
    /// - `a.merge(b).merge(c)` ≡ `a.merge(b.merge(c))` (associative)
    /// - `a.merge(a)` ≡ `a` (idempotent)
    fn merge(&mut self, other: &Self);

    /// Create a merged copy without modifying self.
    fn merged(&self, other: &Self) -> Self
    where
        Self: Sized,
    {
        let mut copy = self.clone();
        copy.merge(other);
        copy
    }

    /// Check if this state subsumes (is greater than or equal to) another.
    /// In lattice terms: self ≥ other.
    fn subsumes(&self, other: &Self) -> bool
    where
        Self: PartialEq,
    {
        self.merged(other) == *self
    }
}

#[cfg(test)]
pub mod laws {
    use super::*;

    /// Verify commutativity: a ∘ b == b ∘ a
    pub fn check_commutative<T: Merge + PartialEq>(a: &T, b: &T) -> bool {
        a.merged(b) == b.merged(a)
    }

    /// Verify associativity: (a ∘ b) ∘ c == a ∘ (b ∘ c)
    pub fn check_associative<T: Merge + PartialEq>(a: &T, b: &T, c: &T) -> bool {
        a.merged(b).merged(c) == a.merged(&b.merged(c))
    }

    /// Verify idempotence: a ∘ a == a
    pub fn check_idempotent<T: Merge + PartialEq>(a: &T) -> bool {
        a.merged(a) == *a
    }
}
