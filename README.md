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
| `ConstraintGCounter` | Distributed satisfaction counting |
| `PNCounter` | Positive/negative counting |
| `ConstraintORSet` | Add-wins constraint tracking with tombstone GC |
| `EisensteinRegister` | Lattice positions as LWW registers (lower norm wins) |
| `FleetTile` | PLATO tiles as mergeable CRDTs with content integrity |
| `ConstraintState` | Composite: all sub-CRDTs in one mergeable unit |
| `VectorClock` | Causal ordering (happened-before, concurrent detection) |
| `DeltaTracker` | Send only changes, not full state |
| `StateHash` | Merkle-style state hashes for efficient sync detection |
| `GossipNode` | Anti-entropy gossip protocol state machine |
| `Simulation` | Deterministic network simulation (loss, latency, partitions) |
| `PlatoClient` | HTTP client for PLATO tile server (`plato` feature) |

## Usage

```rust
use constraint_crdt::*;

// Two fleet nodes, independent
let mut node_a = ConstraintState::new("forgemaster");
node_a.add_constraint("bounds_check");
node_a.record_satisfied(1000);

let mut node_b = ConstraintState::new("oracle1");
node_b.add_constraint("holonomy");
node_b.record_satisfied(2000);

// Merge without coordination — always consistent
let merged = node_a.merged(&node_b);
assert_eq!(merged.metrics.total_satisfied(), 3000);

// Gossip protocol for automatic convergence
let mut a = GossipNode::new("forgemaster");
let mut b = GossipNode::new("oracle1");
a.add_constraint("c1");
b.add_constraint("c2");
gossip_exchange(&mut a, &mut b);
gossip_exchange(&mut b, &mut a);
// Both nodes now have both constraints

// Deterministic simulation with message loss
let mut sim = Simulation::new(5, 42).with_loss_rate(0.3);
for i in 0..5 { sim.add_constraint(i, &format!("c{}", i)); }
let converged_at = sim.run_until_converged(100);
assert!(converged_at.is_some());
```

## Benchmarks (Ryzen AI 9 HX 370)

| Operation | Latency | Throughput |
|-----------|---------|------------|
| G-Counter merge | 76 ns | 13.1M ops/s |
| Full state merge (50+50 constraints) | 12 µs | 82.8K ops/s |
| Vector clock compare (6 nodes) | 369 ns | 2.7M ops/s |
| Delta generation | 76 ns | 13.2M ops/s |

## CLI

```bash
constraint-crdt demo     # Basic CRDT operations
constraint-crdt fleet    # 4-node gossip simulation
constraint-crdt bench    # Micro-benchmarks
constraint-crdt delta    # Delta-state demo
constraint-crdt vclock   # Vector clock causality
```

## Tests

**85 tests** — every CRDT type verified for commutativity, associativity, idempotence. Gossip convergence tested with 2, 3, 5 nodes. Simulation tested with 30% message loss.

## Origin

Extracted from [SmartCRDT](https://github.com/SuperInstance/SmartCRDT)'s OR-Set, G-Counter, PN-Counter, and Merge trait, specialized for the Cocapn fleet's constraint theory ecosystem.

## License

MIT
