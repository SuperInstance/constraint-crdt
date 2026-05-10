# constraint-crdt

CRDT-backed constraint states for distributed fleet consensus.

## Core insight

CRDTs satisfy three algebraic laws (commutative, associative, idempotent).
Constraint satisfaction requires closure under lattice operations.
**These are the same algebraic structure — a semilattice.**

This crate extracts the CRDT merge protocol from [SmartCRDT](https://github.com/SuperInstance/SmartCRDT) and fuses it with [constraint-theory-core](https://github.com/SuperInstance/constraint-theory-core) semantics.

## What you get

- **`Merge` trait** — the algebraic foundation (semilattice join)
- **`ConstraintGCounter`** — distributed constraint satisfaction counting
- **`ConstraintORSet`** — add-wins constraint tracking with tombstone GC
- **`EisensteinRegister`** — lattice positions as LWW registers (lower norm wins)
- **`FleetTile`** — PLATO tiles as mergeable CRDTs
- **`ConstraintState`** — composite: all sub-CRDTs in one mergeable unit

## The math

Every CRDT merge must be:
- **Commutative**: `a ∘ b == b ∘ a`
- **Associative**: `(a ∘ b) ∘ c == a ∘ (b ∘ c)`
- **Idempotent**: `a ∘ a == a`

These are exactly the properties of a **bounded semilattice** — the same structure underlying constraint satisfaction on lattices. This crate unifies both.

## Usage

```rust
use constraint_crdt::{ConstraintState, Merge};

// Two fleet nodes, independent
let mut node_a = ConstraintState::new("forgemaster");
node_a.add_constraint("bounds_check");
node_a.record_satisfied(1000);

let mut node_b = ConstraintState::new("oracle1");
node_b.add_constraint("holonomy");
node_b.record_satisfied(2000);

// Merge without coordination
let merged = node_a.merged(&node_b);

// All constraints present, metrics aggregated
assert_eq!(merged.metrics.total_satisfied(), 3000);
```

## Tests

36 tests, all verifying CRDT algebraic laws (commutativity, associativity, idempotence) for every type.

## Origin

Extracted from [SmartCRDT](https://github.com/SuperInstance/SmartCRDT)'s OR-Set, G-Counter, and Merge trait, specialized for the Cocapn fleet's constraint theory ecosystem.

## License

MIT
