//! # CLI for constraint-crdt
//!
//! Inspect, merge, and simulate CRDT constraint states from the command line.

use constraint_crdt::*;


fn main() {
    let args: Vec<String> = std::env::args().collect();
    
    if args.len() < 2 {
        print_help();
        return;
    }

    match args[1].as_str() {
        "demo" => run_demo(),
        "merge" => run_merge_demo(),
        "fleet" => run_fleet_sim(),
        "vclock" => run_vclock_demo(),
        "delta" => run_delta_demo(),
        "bench" => run_bench(),
        "help" | "--help" | "-h" => print_help(),
        _ => {
            eprintln!("Unknown command: {}", args[1]);
            print_help();
        }
    }
}

fn print_help() {
    println!(r#"constraint-crdt CLI v0.2.0

USAGE:
  constraint-crdt <command>

COMMANDS:
  demo     Show basic CRDT operations
  merge    Demonstrate state merging across nodes
  fleet    Simulate multi-node fleet consensus
  vclock   Vector clock causality demo
  delta    Delta-state CRDT demo
  bench    Run micro-benchmarks
  help     Show this help

CRDT TYPES:
  ConstraintGCounter  Distributed satisfaction counting
  PNCounter           Positive/negative counting
  ConstraintORSet     Add-wins constraint tracking
  EisensteinRegister  Lattice position (lower norm wins)
  FleetTile           PLATO tile with content integrity
  VectorClock         Causal ordering
  ConstraintState     Composite of all above
"#);
}

fn run_demo() {
    println!("=== Constraint-CRDT Demo ===\n");

    // G-Counter
    let mut counter = ConstraintGCounter::new();
    counter.record_satisfied("forgemaster", 1000);
    counter.record_satisfied("oracle1", 2000);
    counter.record_violations("forgemaster", 5);
    println!("Counter: {}", counter);

    // PN-Counter
    let mut pn = PNCounter::new();
    pn.increment("a", 100);
    pn.decrement("a", 30);
    pn.increment("b", 50);
    println!("PN-Counter: {}", pn);

    // OR-Set
    let mut orset = ConstraintORSet::new();
    orset.add("bounds_check", "forgemaster");
    orset.add("norm_check", "oracle1");
    orset.add("holonomy", "jetsonclaw1");
    println!("OR-Set: {} (active: {:?})", orset, orset.active_constraints());

    // Eisenstein register
    let reg = EisensteinRegister::new((3, -1), "forgemaster");
    println!("Position: {} (hex distance: {})", reg, reg.hex_distance());

    // Fleet tile
    let mut tile = FleetTile::new("fleet-ops", "tile-1", "All systems nominal", "forgemaster");
    tile.tag("status");
    tile.tag("verified");
    tile.constraints.add("integrity", "forgemaster");
    println!("Tile: {}", tile);
    println!("Integrity OK: {}", tile.verify_integrity());
}

fn run_merge_demo() {
    println!("=== State Merge Demo ===\n");

    let mut a = ConstraintState::new("forgemaster");
    a.add_constraint("bounds_check");
    a.add_constraint("norm_check");
    a.record_satisfied(1000);
    a.record_violations(5);
    a.update_position((3, 0));
    println!("Node A: {}", a);

    let mut b = ConstraintState::new("oracle1");
    b.add_constraint("holonomy");
    b.add_constraint("consensus");
    b.record_satisfied(2000);
    b.record_violations(10);
    b.update_position((0, 1));
    println!("Node B: {}", b);

    let mut c = ConstraintState::new("jetsonclaw1");
    c.add_constraint("cuda_kernel");
    c.record_satisfied(500);
    c.record_violations(2);
    c.update_position((1, 1));
    println!("Node C: {}", c);

    // Three-way merge
    let merged = a.merged(&b).merged(&c);
    println!("\nMerged: {}", merged);
    println!("Active constraints: {:?}", merged.constraints.active_constraints());
    println!("Satisfaction rate: {:.1}%", merged.satisfaction_rate() * 100.0);
}

fn run_fleet_sim() {
    println!("=== Fleet Consensus Simulation ===\n");

    let nodes = ["forgemaster", "oracle1", "jetsonclaw1", "zeroclaw"];
    let constraints = [
        "bounds_check", "norm_check", "holonomy", "consensus",
        "cuda_kernel", "integrity", "drift_check", "eisenstein_norm",
    ];

    let mut states: Vec<ConstraintState> = nodes.iter()
        .map(|n| ConstraintState::new(n))
        .collect();

    // Round 1: Each node adds constraints independently
    println!("--- Round 1: Independent operation ---");
    for (i, state) in states.iter_mut().enumerate() {
        let start = i * 2;
        let end = (start + 3).min(constraints.len());
        for c in &constraints[start..end] {
            state.add_constraint(c);
        }
        state.record_satisfied(500 + (i as u64) * 200);
        state.record_violations((i as u64) * 3);
        println!("  {}: {} active, {:.0}% satisfied",
            nodes[i], state.active_constraint_count(), state.satisfaction_rate() * 100.0);
    }

    // Round 2: Pairwise merge (gossip protocol)
    println!("\n--- Round 2: Gossip merge ---");
    for i in 0..states.len() {
        for j in (i+1)..states.len() {
            let snapshot_j = states[j].clone();
            let snapshot_i = states[i].clone();
            states[i].merge(&snapshot_j);
            states[j].merge(&snapshot_i);
        }
    }
    for (i, state) in states.iter().enumerate() {
        println!("  {}: {} active, {:.0}% satisfied",
            nodes[i], state.active_constraint_count(), state.satisfaction_rate() * 100.0);
    }

    // Verify convergence
    let counts: Vec<usize> = states.iter().map(|s| s.active_constraint_count()).collect();
    let all_equal = counts.windows(2).all(|w| w[0] == w[1]);
    println!("\nConverged: {} (all nodes see {} constraints)",
        if all_equal { "YES" } else { "NO" }, counts[0]);
}

fn run_vclock_demo() {
    println!("=== Vector Clock Demo ===\n");

    let mut vc_a = VectorClock::new();
    vc_a.increment("forgemaster");
    println!("After forgemaster event: {}", vc_a);

    let mut vc_b = vc_a.clone();
    vc_b.increment("oracle1");
    println!("After oracle1 event: {}", vc_b);

    println!("A happened-before B: {}", vc_a.happened_before_or_equal(&vc_b));
    println!("B happened-before A: {}", vc_b.happened_before_or_equal(&vc_a));
    println!("A concurrent B: {}", vc_a.is_concurrent(&vc_b));

    let mut vc_c = VectorClock::new();
    vc_c.increment("jetsonclaw1");
    println!("\nConcurrent event: {}", vc_c);
    println!("A concurrent C: {}", vc_a.is_concurrent(&vc_c));
    println!("B concurrent C: {}", vc_b.is_concurrent(&vc_c));

    let mut merged = vc_a.clone();
    merged.merge(&vc_b);
    merged.merge(&vc_c);
    println!("\nMerged clock: {}", merged);
}

fn run_delta_demo() {
    println!("=== Delta-State CRDT Demo ===\n");

    let mut tracker = DeltaTracker::new();

    // First snapshot
    let d1 = tracker.generate("forgemaster", 100, 5, (1, 0), &["bounds".into()], &[]);
    println!("Delta 1: {} ({} bytes)", d1, d1.wire_size());

    // Second snapshot (incremental)
    let d2 = tracker.generate("forgemaster", 150, 8, (2, 0), &["norm".into()], &[]);
    println!("Delta 2: {} ({} bytes)", d2, d2.wire_size());

    // No change
    let d3 = tracker.generate("forgemaster", 150, 8, (2, 0), &[], &[]);
    println!("Delta 3: {} (empty={})", d3, d3.is_empty());

    // Full state for comparison
    let full = ConstraintState::new("forgemaster");
    let full_json = serde_json::to_string(&full).unwrap_or_default();
    println!("\nFull state: {} bytes", full_json.len());
    println!("Delta overhead: ~{}x smaller", full_json.len() / d2.wire_size().max(1));
}

fn run_bench() {
    println!("=== Micro-Benchmarks ===\n");

    // G-Counter merge
    let iterations = 100_000;
    let mut a = ConstraintGCounter::new();
    a.record_satisfied("node-a", 100);
    let b = ConstraintGCounter::new();
    
    let start = std::time::Instant::now();
    for _ in 0..iterations {
        let mut c = a.clone();
        c.merge(&b);
        std::hint::black_box(&c);
    }
    let elapsed = start.elapsed();
    let ns_per = elapsed.as_nanos() as f64 / iterations as f64;
    println!("G-Counter merge: {:.0} ns/op ({:.0} ops/sec)", ns_per, 1e9 / ns_per);

    // OR-Set merge
    let mut a = ConstraintORSet::new();
    for i in 0..100 { a.add(&format!("c{}", i), "node-a"); }
    let mut b = ConstraintORSet::new();
    for i in 50..150 { b.add(&format!("c{}", i), "node-b"); }

    let start = std::time::Instant::now();
    for _ in 0..iterations {
        let mut c = a.clone();
        c.merge(&b);
        std::hint::black_box(&c);
    }
    let elapsed = start.elapsed();
    let ns_per = elapsed.as_nanos() as f64 / iterations as f64;
    println!("OR-Set merge (100+150 elements): {:.0} ns/op ({:.0} ops/sec)", ns_per, 1e9 / ns_per);

    // Full state merge
    let mut a = ConstraintState::new("node-a");
    for i in 0..50 { a.add_constraint(&format!("c{}", i)); }
    a.record_satisfied(10000);
    let mut b = ConstraintState::new("node-b");
    for i in 25..75 { b.add_constraint(&format!("c{}", i)); }
    b.record_satisfied(5000);

    let start = std::time::Instant::now();
    for _ in 0..iterations {
        let mut c = a.clone();
        c.merge(&b);
        std::hint::black_box(&c);
    }
    let elapsed = start.elapsed();
    let ns_per = elapsed.as_nanos() as f64 / iterations as f64;
    println!("Full state merge (50+50 constraints): {:.0} ns/op ({:.0} ops/sec)", ns_per, 1e9 / ns_per);

    // Vector clock compare
    let mut vc_a = VectorClock::new();
    for n in &["a", "b", "c", "d", "e"] { vc_a.increment(n); }
    let mut vc_b = vc_a.clone();
    vc_b.increment("f");

    let start = std::time::Instant::now();
    for _ in 0..iterations {
        std::hint::black_box(vc_a.compare(&vc_b));
    }
    let elapsed = start.elapsed();
    let ns_per = elapsed.as_nanos() as f64 / iterations as f64;
    println!("Vector clock compare (6 nodes): {:.0} ns/op ({:.0} ops/sec)", ns_per, 1e9 / ns_per);

    // Delta generation
    let mut tracker = DeltaTracker::new();
    tracker.generate("a", 100, 5, (0, 0), &[], &[]); // warm up
    
    let start = std::time::Instant::now();
    for i in 0..iterations {
        let val = (i % 100) as u64;
        std::hint::black_box(tracker.generate("a", val, val / 10, (0, 0), &[], &[]));
    }
    let elapsed = start.elapsed();
    let ns_per = elapsed.as_nanos() as f64 / iterations as f64;
    println!("Delta generation: {:.0} ns/op ({:.0} ops/sec)", ns_per, 1e9 / ns_per);
}
