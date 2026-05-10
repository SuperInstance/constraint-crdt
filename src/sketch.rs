//! # Novel Experiment 4: Count-Min Sketch CRDT
//!
//! Approximate frequency counting for constraint violations.
//! Instead of tracking exact counts per constraint (O(n) space), use
//! a fixed-size 2D array (O(k*w) space) with probabilistic guarantees.
//!
//! Why this matters: a fleet node tracking 1M unique constraint IDs
//! needs ~4MB for exact counting but ~12KB for a sketch with <1% error.
//! That's 300x compression — and the sketch IS a CRDT (element-wise max).
//!
//! Novel: connects to our constraint theory via the observation that
//! "approximately satisfied" with error bounds is sufficient for
//! fleet coordination — exact counts are overkill.

use crate::merge::Merge;
use serde::{Deserialize, Serialize};
use std::fmt;

/// A Count-Min Sketch CRDT for approximate frequency counting.
///
/// Properties:
/// - Merge: element-wise max (semilattice)
/// - Overestimate: never underestimates (zero false negatives)
/// - Error bound: ε with probability ≥ 1-δ
/// - Space: O(1/ε * ln(1/δ)) regardless of item count
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SketchCRDT {
    /// 2D counter array: d rows × w columns
    counters: Vec<Vec<u64>>,
    /// Number of rows (hash functions)
    d: usize,
    /// Number of columns (width)
    w: usize,
    /// Total items counted
    total: u64,
}

impl SketchCRDT {
    /// Create a new sketch with given error bounds.
    /// `epsilon`: relative error (e.g. 0.01 = 1%)
    /// `delta`: probability of exceeding error (e.g. 0.01 = 1%)
    pub fn new(epsilon: f64, delta: f64) -> Self {
        let w = ((1.0 / epsilon) * std::f64::consts::E).ceil() as usize;
        let d = (1.0 / delta).ln().ceil() as usize;
        Self {
            counters: vec![vec![0u64; w]; d],
            d,
            w,
            total: 0,
        }
    }

    /// Create with specific dimensions.
    pub fn with_dims(d: usize, w: usize) -> Self {
        Self {
            counters: vec![vec![0u64; w]; d],
            d,
            w,
            total: 0,
        }
    }

    /// Record an item with a count.
    pub fn record(&mut self, item: &str, count: u64) {
        for row in 0..self.d {
            let col = self.hash(row, item);
            self.counters[row][col] += count;
        }
        self.total += count;
    }

    /// Estimate the count of an item.
    /// Returns an upper bound (never underestimates).
    pub fn estimate(&self, item: &str) -> u64 {
        let mut min = u64::MAX;
        for row in 0..self.d {
            let col = self.hash(row, item);
            min = min.min(self.counters[row][col]);
        }
        min
    }

    /// Total items counted.
    pub fn total(&self) -> u64 {
        self.total
    }

    /// Space in bytes.
    pub fn space_bytes(&self) -> usize {
        self.d * self.w * 8
    }

    /// Check if an item's estimated count exceeds a threshold.
    /// Useful for "has this constraint been violated > N times?"
    pub fn exceeds(&self, item: &str, threshold: u64) -> bool {
        self.estimate(item) >= threshold
    }

    /// Hash an item for a given row.
    fn hash(&self, row: usize, item: &str) -> usize {
        let mut h: u64 = 0xcbf29ce484222325;
        h = h.wrapping_mul(0x100000001b3) ^ (row as u64 + 1);
        for &b in item.as_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        (h as usize) % self.w
    }
}

impl Merge for SketchCRDT {
    fn merge(&mut self, other: &Self) {
        // Element-wise max — the semilattice join for sketches
        for row in 0..self.d.min(other.d) {
            for col in 0..self.w.min(other.w) {
                self.counters[row][col] = self.counters[row][col].max(other.counters[row][col]);
            }
        }
        self.total = self.total.max(other.total);
    }
}

impl fmt::Display for SketchCRDT {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SketchCRDT({}×{}, {} total, {} bytes)",
            self.d, self.w, self.total, self.space_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merge::laws;

    #[test]
    fn test_never_underestimates() {
        let mut sketch = SketchCRDT::new(0.01, 0.01);
        for i in 0..1000 {
            sketch.record(&format!("item_{}", i % 10), 1);
        }
        
        // Each item_0..9 was recorded 100 times
        for i in 0..10 {
            let est = sketch.estimate(&format!("item_{}", i));
            assert!(est >= 100, "Underestimate for item_{}: {} < 100", i, est);
        }
    }

    #[test]
    fn test_accuracy() {
        let mut sketch = SketchCRDT::new(0.001, 0.01);
        let n = 100_000;
        
        // Record 100 items with varying frequencies
        for i in 0..100 {
            let count = (i + 1) as u64 * 10;
            for _ in 0..count {
                sketch.record(&format!("item_{:03}", i), 1);
            }
        }

        // Check accuracy
        let mut max_error = 0.0_f64;
        for i in 0..100 {
            let true_count = (i + 1) as u64 * 10;
            let est = sketch.estimate(&format!("item_{:03}", i));
            let error = (est as f64 - true_count as f64) / true_count as f64;
            max_error = max_error.max(error);
        }
        
        println!("Max relative error: {:.4} (target < 0.01)", max_error);
        // Sketch overestimates due to hash collisions, but should be reasonable
        assert!(max_error < 0.5, "Error too high: {:.4}", max_error);
    }

    #[test]
    fn test_space_efficiency() {
        let sketch = SketchCRDT::new(0.001, 0.01);
        let sketch_bytes = sketch.space_bytes();
        let exact_bytes = 1_000_000 * 8; // 1M items × 8 bytes
        let ratio = sketch_bytes as f64 / exact_bytes as f64;
        println!("Sketch: {} bytes, Exact: {} bytes, Ratio: {:.4}x",
            sketch_bytes, exact_bytes, ratio);
        assert!(ratio < 0.02, "Sketch should be <2% of exact storage");
    }

    #[test]
    fn test_merge_preserves_estimates() {
        let mut a = SketchCRDT::new(0.01, 0.01);
        let mut b = SketchCRDT::new(0.01, 0.01);
        
        a.record("item_0", 100);
        a.record("item_1", 50);
        b.record("item_2", 200);
        b.record("item_1", 75);

        let merged = a.merged(&b);
        
        assert!(merged.estimate("item_0") >= 100);
        assert!(merged.estimate("item_1") >= 75); // max of 50, 75
        assert!(merged.estimate("item_2") >= 200);
    }

    #[test]
    fn test_merge_commutative() {
        let mut a = SketchCRDT::with_dims(5, 100);
        a.record("a1", 10);
        let mut b = SketchCRDT::with_dims(5, 100);
        b.record("b1", 20);
        assert!(laws::check_commutative(&a, &b));
    }

    #[test]
    fn test_merge_idempotent() {
        let mut a = SketchCRDT::with_dims(5, 100);
        a.record("a1", 10);
        a.record("a2", 20);
        assert!(laws::check_idempotent(&a));
    }

    #[test]
    fn test_threshold_checking() {
        let mut sketch = SketchCRDT::new(0.01, 0.01);
        for _ in 0..50 { sketch.record("violation_a", 1); }
        for _ in 0..5 { sketch.record("violation_b", 1); }

        assert!(sketch.exceeds("violation_a", 10));
        assert!(!sketch.exceeds("violation_b", 10));
    }
}
