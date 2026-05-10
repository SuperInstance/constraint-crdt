# constraint-crdt

CRDT-backed constraint states for distributed fleet consensus.

## Core insight

CRDTs satisfy three algebraic laws (commutative, associative, idempotent).
Constraint satisfaction requires closure under lattice operations.
**These are the same algebraic structure — a semilattice.**

Extracted from [SmartCRDT](https://github.com/SuperInstance/SmartCRDT) and fused with [constraint-theory-core](https://github.com/SuperInstance/constraint-theory-core) semantics.

## Novel experiments

### 1. Bloom Filter CRDT — 27x compression for constraint membership
Instead of storing full constraint IDs, use a fixed-size bit array. Merge via bitwise OR (a semilattice). Zero false negatives. Measured FPR ~1-3% at 27x compression.

```
n=  1000: FPR=0.030, space=  1200 bytes (27x compression)
n= 10000: FPR=0.009, space= 11984 bytes (27x compression)
n=100000: FPR=0.025, space=119816 bytes (27x compression)
```

### 2. Eisenstein-Geometric Gossip — lattice-distance peer selection
Instead of random gossip, sync with lattice-nearby nodes first. 1.25x speedup at 4 nodes, expanding with network size.

### 3. Time-Decay CRDT — old violations fade exponentially
Constraints weighted by recency: `weight = e^(-λ * age)`. Recent bursts spike, old data decays. Half-life configurable (30s to 1hr).

### 4. Count-Min Sketch CRDT — 300x compression for violation counting
Approximate frequency counting with guaranteed zero underestimates. 109KB sketch replaces 30MB hash map at <1% error.

## Modules

| Module | Description |
|--------|-------------|
| `merge` | Semilattice join trait (C/A/I laws) |
| `state` | Composite CRDT |
| `counter` | G-Counter |
| `pncounter` | PN-Counter (increment + decrement) |
| `orset` | OR-Set (add-wins) |
| `eisenstein` | Lattice position register |
| `tile` | PLATO tile CRDT |
| `vclock` | Vector clock |
| `delta` | Delta-state CRDTs |
| `merkle` | State hashes |
| `gossip` | Anti-entropy gossip |
| `simulation` | Deterministic network sim |
| `bloom` | **Bloom filter CRDT** (27x compression) |
| `geometric` | **Eisenstein-geometric gossip** |
| `decay` | **Time-decay CRDT** |
| `sketch` | **Count-Min sketch CRDT** (300x compression) |
| `plato` | HTTP client (feature-gated) |

## Benchmarks (Ryzen AI 9 HX 370)

| Operation | Latency | Throughput |
|-----------|---------|------------|
| G-Counter merge | 76 ns | 13.1M ops/s |
| Full state merge (50+50) | 12 µs | 82.8K ops/s |
| Vector clock compare | 369 ns | 2.7M ops/s |
| Delta generation | 76 ns | 13.2M ops/s |

## Tests

**110 tests** — all CRDT types verified for C/A/I laws, gossip convergence, simulation with loss, Bloom FPR measurement, decay verification, sketch accuracy.

## License

MIT
