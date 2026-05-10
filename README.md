# constraint-crdt

CRDT-backed constraint states for distributed fleet consensus.

## Core insight

CRDTs satisfy three algebraic laws (commutative, associative, idempotent).
Constraint satisfaction requires closure under lattice operations.
**These are the same algebraic structure — a semilattice.**

This crate extracts the CRDT merge protocol from [SmartCRDT](https://github.com/SuperInstance/SmartCRDT) and fuses it with [constraint-theory-core](https://github.com/SuperInstance/constraint-theory-core) semantics.

## What you get

| Type | Description |
|------|-------------|
| `Merge` trait | Semilattice join (C/A/I laws verified) |
| `ConstraintGCounter` | Distributed satisfaction counting (per-node max merge) |
| `PNCounter` | Positive/negative counting (increment + decrement) |
| `ConstraintORSet` | Add-wins constraint tracking with tombstone GC |
| `EisensteinRegister` | Lattice positions as LWW registers (lower norm wins) |
| `FleetTile` | PLATO tiles as mergeable CRDTs with content integrity |
| `ConstraintState` | Composite: all sub-CRDTs in one mergeable unit |
| `VectorClock` | Causal ordering across fleet nodes |
| `DeltaTracker` | Send only changes, not full state |
| `PlatoClient` | HTTP client for PLATO tile server (`plato` feature) |

## Usage

```rust
use constraint_crdt::{ConstraintState, Merge, VectorClock};

// Two fleet nodes, independent
let mut node_a = ConstraintState::new("forgemaster");
node_a.add_constraint("bounds_check");
node_a.record_satisfied(1000);

let mut node_b = ConstraintState::new("oracle1");
node_b.add_constraint("holonomy");
node_b.record_satisfied(2000);

// Merge without coordination — always consistent
let mut merged = node_a.merged(&node_b);
assert_eq!(merged.metrics.total_satisfied(), 3000);

// Vector clocks for causal ordering
let mut vc_a = VectorClock::new();
vc_a.increment("forgemaster");
let mut vc_b = vc_a.clone();
vc_b.increment("oracle1");
assert!(vc_a.happened_before_or_equal(&vc_b));
```

## Delta-state CRDTs

Don't send full state on every heartbeat — send only what changed:

```rust
use constraint_crdt::DeltaTracker;

let mut tracker = DeltaTracker::new();
let delta = tracker.generate("forgemaster", 150, 5, (2, 1), &["new_constraint"], &[]);
println!("Wire size: {} bytes", delta.wire_size());
```

## Tests

**65 tests**, all verifying CRDT algebraic laws (commutativity, associativity, idempotence) for every type. Vector clock causality tested across 1-3 node scenarios.

## Features

- `default` — core CRDT types (zero network deps)
- `plato` — adds `PlatoClient` for HTTP integration with PLATO tile server

## Origin

Extracted from [SmartCRDT](https://github.com/SuperInstance/SmartCRDT)'s OR-Set, G-Counter, PN-Counter, and Merge trait, specialized for the Cocapn fleet's constraint theory ecosystem.

## Ecosystem

- [constraint-theory-core](https://github.com/SuperInstance/constraint-theory-core) — Rust library (184 tests)
- [constraint-theory](https://github.com/SuperInstance/constraint-theory) — Python bindings
- [eisenstein-c](https://github.com/SuperInstance/eisenstein-c) — C library (integer overflow safe)
- [flux-lucid](https://github.com/SuperInstance/flux-lucid) — Unified constraint theory ecosystem
- [holonomy-consensus](https://github.com/SuperInstance/holonomy-consensus) — Distributed agreement protocol

## License

MIT
