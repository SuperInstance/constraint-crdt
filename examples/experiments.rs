// One-off experiment runner
use constraint_crdt::*;

fn main() {
    println!("╔══════════════════════════════════════════════════╗");
    println!("║  CONSTRAINT-CRDT NOVEL EXPERIMENTS              ║");
    println!("╚══════════════════════════════════════════════════╝\n");

    experiment_1_bloom();
    experiment_2_geometric();
    experiment_3_decay();
    experiment_4_sketch();
}

fn experiment_1_bloom() {
    println!("=== Experiment 1: Bloom Filter CRDT ===\n");

    for &n in &[1_000, 10_000, 100_000] {
        let mut bf = bloom::BloomCRDT::new(n, 0.01);
        let items: Vec<String> = (0..n).map(|i| format!("constraint_{}", i)).collect();
        for item in &items { bf.insert(item); }

        // Measure false positive rate
        let mut fp = 0;
        let trials = 100_000;
        for i in n..n+trials {
            if bf.contains(&format!("constraint_{}", i)) { fp += 1; }
        }
        let measured_fpr = fp as f64 / trials as f64;

        // Space comparison
        let bloom_bytes = bf.wire_size();
        let exact_bytes = n * 32; // ~32 bytes per constraint ID
        let compression = exact_bytes as f64 / bloom_bytes as f64;

        println!("  n={:>6}: FPR={:.4} (target 0.01), space={:>6} bytes ({:.1}x compression), fill={:.2}%",
            n, measured_fpr, bloom_bytes, compression, bf.fill_ratio() * 100.0);
    }

    // Merge two Bloom filters
    let mut a = bloom::BloomCRDT::new(1000, 0.01);
    let mut b = bloom::BloomCRDT::new(1000, 0.01);
    for i in 0..500 { a.insert(&format!("a_{}", i)); }
    for i in 500..1000 { b.insert(&format!("b_{}", i)); }
    let merged = a.merged(&b);
    let mut all_found = true;
    for i in 0..500 { if !merged.contains(&format!("a_{}", i)) { all_found = false; } }
    for i in 500..1000 { if !merged.contains(&format!("b_{}", i)) { all_found = false; } }
    println!("\n  Merge test: all items found after merge = {}", all_found);
    println!();
}

fn experiment_2_geometric() {
    println!("=== Experiment 2: Eisenstein-Geometric Gossip ===\n");

    for &n in &[4, 8, 16, 32] {
        let max_rounds = match n {
            4 => 20, 8 => 40, 16 => 80, 32 => 150, _ => 200,
        };
        let result = geometric::run_experiment(n, 42, max_rounds);
        
        let rr = result.random_convergence_rounds.unwrap_or(0);
        let gr = result.geometric_convergence_rounds.unwrap_or(0);
        let speedup = if gr > 0 { rr as f64 / gr as f64 } else { 0.0 };
        
        println!("  n={:>2}: random={:>3} rounds ({:>5} msgs), geometric={:>3} rounds ({:>5} msgs), speedup={:.2}x",
            n, rr, result.random_messages, gr, result.geometric_messages, speedup);
    }
    println!();
}

fn experiment_3_decay() {
    println!("=== Experiment 3: Time-Decay Constraint CRDT ===\n");

    let ns = 1_000_000_000u64;
    
    // Simulate a fleet node over 1 hour
    let half_life = 300.0; // 5 minutes
    let mut state = decay::DecayConstraintState::new("forgemaster", half_life);
    
    // Steady state: 100 satisfied per minute, 2 violations per minute
    for t in 0..60 {
        let time = t as u64 * 60 * ns;
        state.record_satisfied(100.0, time);
        state.record_violations(2.0, time);
    }
    
    println!("  After 60 min steady state:");
    println!("    Satisfaction rate at t=60: {:.1}%", state.satisfaction_rate(60 * 60 * ns) * 100.0);
    
    // Burst of violations at minute 55
    let mut state2 = decay::DecayConstraintState::new("oracle1", half_life);
    for t in 0..55 {
        let time = t as u64 * 60 * ns;
        state2.record_satisfied(100.0, time);
        state2.record_violations(2.0, time);
    }
    // Burst: 50 violations at minute 55
    state2.record_violations(50.0, 55 * 60 * ns);
    state2.record_satisfied(100.0, 55 * 60 * ns);
    for t in 56..60 {
        let time = t as u64 * 60 * ns;
        state2.record_satisfied(100.0, time);
        state2.record_violations(2.0, time);
    }
    
    println!("\n  After burst (50 violations at min 55):");
    println!("    Satisfaction rate at t=60: {:.1}%", state2.satisfaction_rate(60 * 60 * ns) * 100.0);
    println!("    Violation weight at t=60: {:.1}", state2.violation_weight(60 * 60 * ns));
    
    // Half-life comparison
    println!("\n  Half-life sensitivity:");
    for &hl in &[30.0, 60.0, 300.0, 3600.0] {
        let mut s = decay::DecayConstraintState::new("test", hl);
        for t in 0..60 {
            s.record_violations(5.0, t as u64 * 60 * ns);
        }
        let weight = s.violation_weight(60 * 60 * ns);
        println!("    half_life={:>4.0}s: violation_weight={:.1}", hl, weight);
    }
    println!();
}

fn experiment_4_sketch() {
    println!("=== Experiment 4: Count-Min Sketch CRDT ===\n");

    // Heavy-hitter detection: which constraints are violated most?
    let mut sketch = sketch::SketchCRDT::new(0.001, 0.01);
    
    // Simulate 1M constraint checks
    let n = 1_000_000;
    let mut exact: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    
    for i in 0..n {
        // Zipfian-ish distribution: a few constraints violated often
        let constraint = if i % 3 == 0 { "bounds_check" }
            else if i % 5 == 0 { "norm_check" }
            else if i % 7 == 0 { "holonomy" }
            else if i % 100 == 0 { "rare_violation" }
            else { "ok" };
        
        if constraint != "ok" {
            sketch.record(constraint, 1);
            *exact.entry(constraint.to_string()).or_insert(0) += 1;
        }
    }
    
    println!("  Heavy-hitter detection (1M checks):");
    println!("  {:>20} {:>10} {:>10} {:>10}", "Constraint", "Exact", "Estimated", "Error%");
    
    let mut items: Vec<_> = exact.iter().collect();
    items.sort_by_key(|(_, &v)| std::cmp::Reverse(v));
    
    for (name, &true_count) in &items {
        let est = sketch.estimate(name);
        let error = if true_count > 0 { (est as f64 - true_count as f64) / true_count as f64 * 100.0 } else { 0.0 };
        println!("  {:>20} {:>10} {:>10} {:>9.1}%", name, true_count, est, error);
    }
    
    println!("\n  Space: sketch = {} bytes, exact hash map ≈ {} bytes",
        sketch.space_bytes(),
        items.len() * 40); // rough estimate
    
    // Merge test
    let mut a = sketch::SketchCRDT::new(0.001, 0.01);
    let mut b = sketch::SketchCRDT::new(0.001, 0.01);
    for _ in 0..1000 { a.record("bounds_check", 1); }
    for _ in 0..500 { b.record("norm_check", 1); }
    let merged = a.merged(&b);
    println!("\n  After merge: bounds_check ≥ {}, norm_check ≥ {}",
        merged.estimate("bounds_check"), merged.estimate("norm_check"));
    println!();
}
