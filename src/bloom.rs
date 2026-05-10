//! # Novel Experiment 1: Bloom Filter CRDT
//!
//! A CRDT where set membership is tested via Bloom filter.
//! Probabilistic constraint checking — "probably satisfied" with guaranteed
//! zero false negatives.
//!
//! Why this matters: instead of storing full constraint IDs (O(n) space),
//! store a fixed-size bit array (O(k) space). For fleet nodes with thousands
//! of constraints, this is 10-100x smaller on the wire.
//!
//! Novel contribution: Bloom filter merge is element-wise OR (a semilattice!).
//! This IS a CRDT. And it connects directly to our Bloom filter proof in
//! the Galois Unification Principle (Part 3).

use crate::merge::Merge;
use serde::{Deserialize, Serialize};
use std::fmt;

/// A Bloom filter CRDT for approximate constraint membership.
///
/// Properties:
/// - Merge: bitwise OR (commutative, associative, idempotent)
/// - False positive rate: tunable via bit count and hash count
/// - False negative rate: ZERO (if it says "not present", it's definitely not)
/// - Space: O(bits) regardless of how many constraints are tracked
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BloomCRDT {
    /// Bit array
    bits: Vec<u64>,
    /// Number of hash functions
    k: usize,
    /// Total constraints inserted (for FPR estimation)
    count: usize,
    /// Number of bits
    m: usize,
}

impl BloomCRDT {
    /// Create a new Bloom filter CRDT.
    /// `expected_items`: estimated number of constraints
    /// `fp_rate`: target false positive rate (e.g. 0.01 = 1%)
    pub fn new(expected_items: usize, fp_rate: f64) -> Self {
        let m = optimal_m(expected_items, fp_rate);
        let k = optimal_k(m, expected_items);
        let words = (m + 63) / 64;
        Self {
            bits: vec![0u64; words],
            k,
            count: 0,
            m,
        }
    }

    /// Create with specific parameters.
    pub fn with_params(m: usize, k: usize) -> Self {
        let words = (m + 63) / 64;
        Self {
            bits: vec![0u64; words],
            k,
            count: 0,
            m,
        }
    }

    /// Insert a constraint ID.
    pub fn insert(&mut self, item: &str) {
        let hashes = self.hash(item);
        for h in hashes {
            let (word, bit) = self.index(h);
            self.bits[word] |= 1u64 << bit;
        }
        self.count += 1;
    }

    /// Check if a constraint is probably present.
    /// Returns true = "probably present" (may be false positive)
    /// Returns false = "definitely not present" (zero false negatives)
    pub fn contains(&self, item: &str) -> bool {
        let hashes = self.hash(item);
        for h in hashes {
            let (word, bit) = self.index(h);
            if self.bits[word] & (1u64 << bit) == 0 {
                return false;
            }
        }
        true
    }

    /// Estimated false positive rate at current fill level.
    pub fn estimated_fpr(&self) -> f64 {
        let set_bits = self.set_bit_count();
        let ratio = set_bits as f64 / self.m as f64;
        ratio.powi(self.k as i32)
    }

    /// Number of bits set.
    fn set_bit_count(&self) -> usize {
        self.bits.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// Fill ratio (0.0 - 1.0).
    pub fn fill_ratio(&self) -> f64 {
        self.set_bit_count() as f64 / self.m as f64
    }

    /// Space in bytes.
    pub fn space_bytes(&self) -> usize {
        self.bits.len() * 8
    }

    /// Wire size (just the bit array, no metadata overhead).
    pub fn wire_size(&self) -> usize {
        self.space_bytes()
    }

    /// Number of items inserted.
    pub fn count(&self) -> usize {
        self.count
    }

    /// Hash an item to k positions using double hashing.
    fn hash(&self, item: &str) -> Vec<usize> {
        let mut result = Vec::with_capacity(self.k);
        let (h1, h2) = double_hash(item);
        for i in 0..self.k {
            result.push(((h1 as usize).wrapping_add(i.wrapping_mul(h2 as usize))) % self.m);
        }
        result
    }

    fn index(&self, pos: usize) -> (usize, usize) {
        (pos / 64, pos % 64)
    }
}

impl Merge for BloomCRDT {
    fn merge(&mut self, other: &Self) {
        // Bitwise OR — the semilattice join for Bloom filters
        for i in 0..self.bits.len().min(other.bits.len()) {
            self.bits[i] |= other.bits[i];
        }
        // Take max count (approximate)
        self.count = self.count.max(other.count);
    }
}

/// Optimal bit count: m = -n * ln(p) / (ln(2))^2
fn optimal_m(n: usize, p: f64) -> usize {
    let m = -(n as f64) * p.ln() / (2.0_f64.ln().powi(2));
    m.ceil() as usize
}

/// Optimal hash count: k = (m/n) * ln(2)
fn optimal_k(m: usize, n: usize) -> usize {
    let k = (m as f64 / n as f64) * 2.0_f64.ln();
    k.ceil() as usize
}

/// Double hashing using FNV-1a.
fn double_hash(item: &str) -> (u64, u64) {
    let mut h1: u64 = 0xcbf29ce484222325;
    let mut h2: u64 = 0x9e3779b97f4a7c15;
    for &b in item.as_bytes() {
        h1 ^= b as u64;
        h1 = h1.wrapping_mul(0x100000001b3);
        h2 ^= b as u64;
        h2 = h2.wrapping_mul(0x100000001b3);
    }
    (h1, h2)
}

impl fmt::Display for BloomCRDT {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BloomCRDT({} items, {} bits, k={}, FPR={:.4}, {} bytes)",
            self.count, self.m, self.k, self.estimated_fpr(), self.space_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merge::laws;

    #[test]
    fn test_no_false_negatives() {
        let mut bf = BloomCRDT::new(1000, 0.01);
        let items: Vec<String> = (0..1000).map(|i| format!("constraint_{}", i)).collect();
        for item in &items { bf.insert(item); }
        
        // Every inserted item must be found
        for item in &items {
            assert!(bf.contains(item), "False negative for {}", item);
        }
    }

    #[test]
    fn test_measured_false_positive_rate() {
        let mut bf = BloomCRDT::new(1000, 0.01);
        for i in 0..1000 { bf.insert(&format!("item_{}", i)); }

        let mut false_positives = 0;
        let trials = 100_000;
        for i in 1000..1000 + trials {
            if bf.contains(&format!("item_{}", i)) {
                false_positives += 1;
            }
        }

        let measured_fpr = false_positives as f64 / trials as f64;
        println!("Target FPR: 0.01, Measured FPR: {:.6}", measured_fpr);
        assert!(measured_fpr < 0.05, "FPR too high: {:.4}", measured_fpr);
    }

    #[test]
    fn test_merge_preserves_membership() {
        let mut a = BloomCRDT::new(100, 0.01);
        let mut b = BloomCRDT::new(100, 0.01);
        a.insert("constraint_a");
        b.insert("constraint_b");

        assert!(a.contains("constraint_a"));
        assert!(!a.contains("constraint_b"));

        let merged = a.merged(&b);
        assert!(merged.contains("constraint_a"));
        assert!(merged.contains("constraint_b"));
    }

    #[test]
    fn test_merge_commutative() {
        let mut a = BloomCRDT::new(100, 0.01);
        a.insert("a1"); a.insert("a2");
        let mut b = BloomCRDT::new(100, 0.01);
        b.insert("b1"); b.insert("b2");
        assert!(laws::check_commutative(&a, &b));
    }

    #[test]
    fn test_merge_idempotent() {
        let mut a = BloomCRDT::new(100, 0.01);
        a.insert("a1"); a.insert("a2");
        assert!(laws::check_idempotent(&a));
    }

    #[test]
    fn test_merge_associative() {
        let mut a = BloomCRDT::new(100, 0.01);
        a.insert("a1");
        let mut b = BloomCRDT::new(100, 0.01);
        b.insert("b1");
        let mut c = BloomCRDT::new(100, 0.01);
        c.insert("c1");
        assert!(laws::check_associative(&a, &b, &c));
    }

    #[test]
    fn test_space_efficiency() {
        // 10,000 constraints at 1% FPR
        let bf = BloomCRDT::new(10_000, 0.01);
        let bits_per_item = bf.m as f64 / 10_000.0;
        println!("Bits per item: {:.1} (optimal ~9.6)", bits_per_item);
        assert!(bits_per_item < 12.0, "Too many bits per item");
    }

    #[test]
    fn test_wire_size_comparison() {
        // 10,000 constraints: Bloom vs exact storage
        let bf = BloomCRDT::new(10_000, 0.01);
        let bloom_bytes = bf.wire_size();
        let exact_bytes = 10_000 * 32; // Assuming ~32 byte constraint IDs
        let ratio = bloom_bytes as f64 / exact_bytes as f64;
        println!("Bloom: {} bytes, Exact: {} bytes, Ratio: {:.2}x", 
            bloom_bytes, exact_bytes, ratio);
        assert!(ratio < 0.15, "Bloom should be much smaller");
    }
}
