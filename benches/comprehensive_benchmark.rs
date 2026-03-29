#[macro_use]
extern crate criterion;

use criterion::Criterion;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::cell::Cell;
use std::time::{Duration, Instant};

use sparse_merkle_tree::{
    blake2b::Blake2bHasher,
    default_store::DefaultStore,
    error::Error,
    traits::{StoreReadOps, StoreWriteOps},
    BranchKey, BranchNode,
    SparseMerkleTree, H256,
};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[allow(clippy::upper_case_acronyms)]
type SMT = SparseMerkleTree<Blake2bHasher, H256, DefaultStore<H256>>;

type CountingSMT = SparseMerkleTree<Blake2bHasher, H256, CountingStore>;

// ---------------------------------------------------------------------------
// Counting store wrapper
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct StoreCounters {
    branch_get: Cell<u64>,
    branch_insert: Cell<u64>,
    branch_remove: Cell<u64>,
    leaf_get: Cell<u64>,
    leaf_insert: Cell<u64>,
    leaf_remove: Cell<u64>,
}

impl StoreCounters {
    fn reset(&self) {
        self.branch_get.set(0);
        self.branch_insert.set(0);
        self.branch_remove.set(0);
        self.leaf_get.set(0);
        self.leaf_insert.set(0);
        self.leaf_remove.set(0);
    }

    fn snapshot(&self) -> StoreSnapshot {
        StoreSnapshot {
            branch_get: self.branch_get.get(),
            branch_insert: self.branch_insert.get(),
            branch_remove: self.branch_remove.get(),
            leaf_get: self.leaf_get.get(),
            leaf_insert: self.leaf_insert.get(),
            leaf_remove: self.leaf_remove.get(),
        }
    }
}

#[derive(Debug, Clone, Default)]
struct StoreSnapshot {
    branch_get: u64,
    branch_insert: u64,
    branch_remove: u64,
    leaf_get: u64,
    leaf_insert: u64,
    leaf_remove: u64,
}

#[derive(Debug, Clone, Default)]
struct CountingStore {
    inner: DefaultStore<H256>,
    counters: StoreCounters,
}

impl CountingStore {
    fn counters(&self) -> &StoreCounters {
        &self.counters
    }
}

impl StoreReadOps<H256> for CountingStore {
    fn get_branch(&self, branch_key: &BranchKey) -> Result<Option<BranchNode>, Error> {
        self.counters.branch_get.set(self.counters.branch_get.get() + 1);
        self.inner.get_branch(branch_key)
    }
    fn get_leaf(&self, leaf_key: &H256) -> Result<Option<H256>, Error> {
        self.counters.leaf_get.set(self.counters.leaf_get.get() + 1);
        self.inner.get_leaf(leaf_key)
    }
}

impl StoreWriteOps<H256> for CountingStore {
    fn insert_branch(&mut self, branch_key: BranchKey, branch: BranchNode) -> Result<(), Error> {
        self.counters.branch_insert.set(self.counters.branch_insert.get() + 1);
        self.inner.insert_branch(branch_key, branch)
    }
    fn insert_leaf(&mut self, leaf_key: H256, leaf: H256) -> Result<(), Error> {
        self.counters.leaf_insert.set(self.counters.leaf_insert.get() + 1);
        self.inner.insert_leaf(leaf_key, leaf)
    }
    fn remove_branch(&mut self, branch_key: &BranchKey) -> Result<(), Error> {
        self.counters.branch_remove.set(self.counters.branch_remove.get() + 1);
        self.inner.remove_branch(branch_key)
    }
    fn remove_leaf(&mut self, leaf_key: &H256) -> Result<(), Error> {
        self.counters.leaf_remove.set(self.counters.leaf_remove.get() + 1);
        self.inner.remove_leaf(leaf_key)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn random_h256(rng: &mut impl Rng) -> H256 {
    let mut buf = [0u8; 32];
    rng.fill(&mut buf);
    buf.into()
}

fn seeded_rng() -> StdRng {
    StdRng::seed_from_u64(42)
}

/// Build a tree of `n` random key-value pairs, returning the tree and the keys.
fn build_smt(n: usize, rng: &mut impl Rng) -> (SMT, Vec<H256>) {
    let mut smt = SMT::default();
    let mut keys = Vec::with_capacity(n);
    for _ in 0..n {
        let key = random_h256(rng);
        let value = random_h256(rng);
        smt.update(key, value).unwrap();
        keys.push(key);
    }
    (smt, keys)
}

/// Build a counting tree of `n` random key-value pairs.
fn build_counting_smt(n: usize, rng: &mut impl Rng) -> (CountingSMT, Vec<H256>) {
    let store = CountingStore::default();
    let mut smt = SparseMerkleTree::new_with_store(store).unwrap();
    let mut keys = Vec::with_capacity(n);
    for _ in 0..n {
        let key = random_h256(rng);
        let value = random_h256(rng);
        smt.update(key, value).unwrap();
        keys.push(key);
    }
    (smt, keys)
}

fn format_duration(d: Duration) -> String {
    let nanos = d.as_nanos();
    if nanos < 1_000 {
        format!("{} ns", nanos)
    } else if nanos < 1_000_000 {
        format!("{:.2} us", nanos as f64 / 1_000.0)
    } else if nanos < 1_000_000_000 {
        format!("{:.2} ms", nanos as f64 / 1_000_000.0)
    } else {
        format!("{:.2} s", nanos as f64 / 1_000_000_000.0)
    }
}

fn format_rate(count: f64, d: Duration) -> String {
    let secs = d.as_secs_f64();
    if secs == 0.0 {
        return "N/A".to_string();
    }
    let rate = count / secs;
    if rate >= 1_000_000.0 {
        format!("{:.2}M/s", rate / 1_000_000.0)
    } else if rate >= 1_000.0 {
        format!("{:.2}K/s", rate / 1_000.0)
    } else {
        format!("{:.2}/s", rate)
    }
}

fn format_bytes(b: usize) -> String {
    if b >= 1_048_576 {
        format!("{:.2} MiB", b as f64 / 1_048_576.0)
    } else if b >= 1_024 {
        format!("{:.2} KiB", b as f64 / 1_024.0)
    } else {
        format!("{} B", b)
    }
}

/// Compute proof size in bytes (bitmap H256s + merkle path merge values).
fn proof_size_bytes(proof: &sparse_merkle_tree::MerkleProof) -> usize {
    let bitmap_bytes = proof.leaves_bitmap().len() * 32;
    // Each MergeValue is either Value(H256)=32 bytes or MergeWithZero{base_node, zero_bits, zero_count}=65 bytes.
    // We approximate by counting elements * 32 (lower bound) -- but for a more accurate measure
    // we count via the compiled proof bytes.
    let path_count = proof.merkle_path().len();
    // Conservative estimate: each merge value contains at least one H256
    bitmap_bytes + path_count * 32
}

/// Measure the median duration of `iterations` runs.
fn measure<F: FnMut()>(mut f: F, iterations: u32) -> Duration {
    let mut times = Vec::with_capacity(iterations as usize);
    for _ in 0..iterations {
        let start = Instant::now();
        f();
        times.push(start.elapsed());
    }
    times.sort();
    times[times.len() / 2]
}

// ---------------------------------------------------------------------------
// Report generation (non-criterion, deterministic)
// ---------------------------------------------------------------------------

fn generate_report() {
    use chrono::Utc;

    let iterations = 5u32;

    // Gather system info
    let date = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let hostname = std::fs::read_to_string("/etc/hostname")
        .unwrap_or_else(|_| "unknown".into())
        .trim()
        .to_string();
    let arch = std::env::consts::ARCH;
    let cpu_model = std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|info| {
            info.lines()
                .find(|l| l.starts_with("model name"))
                .map(|l| l.splitn(2, ':').nth(1).unwrap_or("unknown").trim().to_string())
        })
        .unwrap_or_else(|| "unknown".into());
    let rustc_version = std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into());

    let mut report = String::new();
    report.push_str("# Sparse Merkle Tree Performance Benchmark Report\n\n");
    report.push_str(&format!("**Date:** {}\n", date));
    report.push_str(&format!("**Host:** {} -- {}\n", hostname, arch));
    report.push_str(&format!("**CPU:** {}\n", cpu_model));
    report.push_str(&format!("**Rust:** {}\n", rustc_version));
    report.push_str("**SMT Version:** 0.6.2\n");
    report.push_str("**Hash Backend:** blake2b-rs\n\n");

    // -----------------------------------------------------------------------
    // Section 1: Tree Operations Scaling
    // -----------------------------------------------------------------------
    report.push_str("## 1. Tree Operations Scaling\n\n");
    report.push_str("| Tree Size | Update (single) | Update All (batch) | Get (single) | Ops/sec (update) | Ops/sec (get) |\n");
    report.push_str("|-----------|-----------------|-------------------|--------------|------------------|---------------|\n");

    let tree_sizes_ops: &[usize] = &[100, 1_000, 10_000, 50_000, 100_000];

    for &size in tree_sizes_ops {
        eprintln!("[report] Section 1: tree size = {}", size);

        // --- update (single) : build tree from scratch, measure total time, divide by count ---
        let update_dur = measure(
            || {
                let mut rng = seeded_rng();
                let mut smt = SMT::default();
                for _ in 0..size {
                    let k = random_h256(&mut rng);
                    let v = random_h256(&mut rng);
                    smt.update(k, v).unwrap();
                }
            },
            iterations,
        );
        let per_update = update_dur / size as u32;

        // --- update_all (batch) ---
        let update_all_dur = measure(
            || {
                let mut rng = seeded_rng();
                let kvs: Vec<(H256, H256)> = (0..size)
                    .map(|_| (random_h256(&mut rng), random_h256(&mut rng)))
                    .collect();
                let mut smt = SMT::default();
                smt.update_all(kvs).unwrap();
            },
            iterations,
        );
        let per_update_all = update_all_dur / size as u32;

        // --- get (single lookup in a pre-built tree) ---
        let mut rng = seeded_rng();
        let (smt, keys) = build_smt(size, &mut rng);
        let lookup_key = keys[size / 2];
        let get_dur = measure(
            || {
                smt.get(&lookup_key).unwrap();
            },
            iterations.max(20),
        );

        let ops_update = format_rate(1.0, per_update);
        let ops_get = format_rate(1.0, get_dur);

        report.push_str(&format!(
            "| {:>9} | {:>15} | {:>17} | {:>12} | {:>16} | {:>13} |\n",
            size,
            format_duration(per_update),
            format_duration(per_update_all),
            format_duration(get_dur),
            ops_update,
            ops_get,
        ));
    }
    report.push('\n');

    // -----------------------------------------------------------------------
    // Section 2: Proof Generation & Verification Scaling
    // -----------------------------------------------------------------------
    report.push_str("## 2. Proof Generation & Verification\n\n");
    report.push_str("| Tree Size | Leaves | Gen Time | Verify Time | Proof Size | Verify/sec |\n");
    report.push_str("|-----------|--------|----------|-------------|------------|------------|\n");

    let tree_sizes_proof: &[usize] = &[100, 1_000, 10_000, 100_000];
    let leaf_counts: &[usize] = &[1, 5, 10, 20, 40];

    for &size in tree_sizes_proof {
        eprintln!("[report] Section 2: tree size = {}", size);
        let mut rng = seeded_rng();
        let (smt, keys) = build_smt(size, &mut rng);

        for &leaf_count in leaf_counts {
            if leaf_count > keys.len() {
                continue;
            }
            let proof_keys: Vec<H256> = keys.iter().take(leaf_count).cloned().collect();
            let leaves: Vec<(H256, H256)> = proof_keys
                .iter()
                .map(|k| (*k, smt.get(k).unwrap()))
                .collect();

            // Generate proof once to measure size
            let proof = smt.merkle_proof(proof_keys.clone()).unwrap();
            let psize = proof_size_bytes(&proof);

            // Measure generation
            let gen_dur = measure(
                || {
                    let _ = smt.merkle_proof(proof_keys.clone()).unwrap();
                },
                iterations,
            );

            // Measure verification
            let root = *smt.root();
            let verify_dur = measure(
                || {
                    let p = smt.merkle_proof(proof_keys.clone()).unwrap();
                    let valid = p.verify::<Blake2bHasher>(&root, leaves.clone());
                    assert!(valid.expect("verify result"));
                },
                iterations,
            );
            // Subtract gen time for a cleaner verify-only estimate
            let verify_only = if verify_dur > gen_dur {
                verify_dur - gen_dur
            } else {
                verify_dur
            };

            let verify_per_sec = if verify_only.as_secs_f64() > 0.0 {
                format_rate(1.0, verify_only)
            } else {
                "N/A".to_string()
            };

            report.push_str(&format!(
                "| {:>9} | {:>6} | {:>8} | {:>11} | {:>10} | {:>10} |\n",
                size,
                leaf_count,
                format_duration(gen_dur),
                format_duration(verify_only),
                format_bytes(psize),
                verify_per_sec,
            ));
        }
    }
    report.push('\n');

    // -----------------------------------------------------------------------
    // Section 3: Store Operation Profile
    // -----------------------------------------------------------------------
    report.push_str("## 3. Store Operation Profile\n\n");
    report.push_str("Store operation counts when inserting N keys via `update()` into an empty tree.\n\n");
    let store_sizes: &[usize] = &[100, 1_000, 10_000, 100_000];

    report.push_str("| Operation | ");
    for s in store_sizes {
        report.push_str(&format!("Tree {} | ", s));
    }
    report.push('\n');
    report.push_str("|-----------|");
    for _ in store_sizes {
        report.push_str("----------|");
    }
    report.push('\n');

    // Collect snapshots
    let mut snapshots: Vec<StoreSnapshot> = Vec::new();
    for &size in store_sizes {
        eprintln!("[report] Section 3: tree size = {}", size);
        let mut rng = seeded_rng();
        let (smt, _keys) = build_counting_smt(size, &mut rng);
        snapshots.push(smt.store().counters().snapshot());
    }

    let ops: &[(&str, Box<dyn Fn(&StoreSnapshot) -> u64>)] = &[
        ("branch_get", Box::new(|s: &StoreSnapshot| s.branch_get)),
        ("branch_insert", Box::new(|s: &StoreSnapshot| s.branch_insert)),
        ("branch_remove", Box::new(|s: &StoreSnapshot| s.branch_remove)),
        ("leaf_get", Box::new(|s: &StoreSnapshot| s.leaf_get)),
        ("leaf_insert", Box::new(|s: &StoreSnapshot| s.leaf_insert)),
        ("leaf_remove", Box::new(|s: &StoreSnapshot| s.leaf_remove)),
    ];

    for (name, getter) in ops {
        report.push_str(&format!("| {:>13} | ", name));
        for snap in &snapshots {
            report.push_str(&format!("{:>8} | ", getter(snap)));
        }
        report.push('\n');
    }
    report.push('\n');

    // Also profile a single-key get after tree is built
    report.push_str("Store operation counts for a single `get()` on a tree of size N.\n\n");
    report.push_str("| Operation | ");
    for s in store_sizes {
        report.push_str(&format!("Tree {} | ", s));
    }
    report.push('\n');
    report.push_str("|-----------|");
    for _ in store_sizes {
        report.push_str("----------|");
    }
    report.push('\n');

    let mut get_snapshots: Vec<StoreSnapshot> = Vec::new();
    for &size in store_sizes {
        let mut rng = seeded_rng();
        let (smt, keys) = build_counting_smt(size, &mut rng);
        smt.store().counters().reset();
        let _val = smt.get(&keys[size / 2]).unwrap();
        get_snapshots.push(smt.store().counters().snapshot());
    }

    for (name, getter) in ops {
        report.push_str(&format!("| {:>13} | ", name));
        for snap in &get_snapshots {
            report.push_str(&format!("{:>8} | ", getter(snap)));
        }
        report.push('\n');
    }
    report.push('\n');

    // -----------------------------------------------------------------------
    // Section 4: Throughput Summary
    // -----------------------------------------------------------------------
    report.push_str("## 4. Throughput Summary\n\n");
    report.push_str("| Metric | Value | Notes |\n");
    report.push_str("|--------|-------|-------|\n");

    // Batch update throughput at various sizes
    let batch_sizes: &[usize] = &[100, 1_000, 10_000, 50_000, 100_000];
    let mut peak_update_rate = 0.0f64;
    let mut peak_update_batch = 0usize;

    for &bs in batch_sizes {
        eprintln!("[report] Section 4 batch throughput: size = {}", bs);
        let dur = measure(
            || {
                let mut rng = seeded_rng();
                let kvs: Vec<(H256, H256)> = (0..bs)
                    .map(|_| (random_h256(&mut rng), random_h256(&mut rng)))
                    .collect();
                let mut smt = SMT::default();
                smt.update_all(kvs).unwrap();
            },
            iterations,
        );
        let rate = bs as f64 / dur.as_secs_f64();
        if rate > peak_update_rate {
            peak_update_rate = rate;
            peak_update_batch = bs;
        }
    }

    report.push_str(&format!(
        "| Peak batch update throughput | {} | batch size {} |\n",
        format_rate(peak_update_rate, Duration::from_secs(1)),
        peak_update_batch,
    ));

    // Verification throughput (tree=10_000, leaves=20)
    {
        let mut rng = seeded_rng();
        let (smt, keys) = build_smt(10_000, &mut rng);
        let proof_keys: Vec<H256> = keys.iter().take(20).cloned().collect();
        let leaves: Vec<(H256, H256)> = proof_keys
            .iter()
            .map(|k| (*k, smt.get(k).unwrap()))
            .collect();
        let root = *smt.root();

        let count = 50u32;
        let start = Instant::now();
        for _ in 0..count {
            let proof = smt.merkle_proof(proof_keys.clone()).unwrap();
            let valid = proof.verify::<Blake2bHasher>(&root, leaves.clone());
            assert!(valid.expect("verify result"));
        }
        let total = start.elapsed();
        let rate = count as f64 / total.as_secs_f64();

        report.push_str(&format!(
            "| Proof verify throughput (20 leaves, 10K tree) | {:.2}/s | gen+verify combined |\n",
            rate,
        ));
    }

    // Single-key update throughput on existing large tree
    {
        let mut rng = seeded_rng();
        let (mut smt, _keys) = build_smt(50_000, &mut rng);
        let count = 200u32;
        let start = Instant::now();
        for _ in 0..count {
            let k = random_h256(&mut rng);
            let v = random_h256(&mut rng);
            smt.update(k, v).unwrap();
        }
        let total = start.elapsed();
        let rate = count as f64 / total.as_secs_f64();

        report.push_str(&format!(
            "| Single update on 50K tree | {:.2}/s | incremental insert |\n",
            rate,
        ));
    }

    report.push('\n');

    // -----------------------------------------------------------------------
    // Comparison Baseline
    // -----------------------------------------------------------------------
    report.push_str("## Comparison Baseline\n\n");
    report.push_str("Reference: Quake's C SMT optimization (ckb_smt.h), 36 experiments tracked via commits.\n\n");
    report.push_str("| Key/Value Pairs in Tree | Leaves Verified | Cycles (before) | Cycles (after) | Reduction |\n");
    report.push_str("|---|---|---|---|---|\n");
    report.push_str("| 16 | 1 | 116 K | 116 K | baseline |\n");
    report.push_str("| 131,072 | 40 | 6,919 K | 1,703 K | 75.4% |\n");
    report.push_str("\n");
    report.push_str("Hot paths targeted: `smt_calculate_root()` and `_smt_merge()` in `c/ckb_smt.h`.\n");
    report.push_str("The Rust SMT timings above can be cross-referenced with these cycle counts\n");
    report.push_str("when evaluating optimization impact on the same algorithmic operations.\n");

    // Write report
    let report_dir = "/home/phill/nervos_optimizations/benchmarks/reports";
    std::fs::create_dir_all(report_dir).expect("failed to create reports directory");
    let report_path = format!("{}/smt-report.md", report_dir);
    std::fs::write(&report_path, &report).expect("failed to write report");
    eprintln!("\n[report] Written to {}", report_path);
}

// ---------------------------------------------------------------------------
// Criterion benchmarks (statistical, for regression detection)
// ---------------------------------------------------------------------------

fn criterion_tree_ops(c: &mut Criterion) {
    c.bench_function_over_inputs(
        "comprehensive/update_single",
        |b, &&size| {
            b.iter(|| {
                let mut rng = seeded_rng();
                let mut smt = SMT::default();
                for _ in 0..size {
                    let k = random_h256(&mut rng);
                    let v = random_h256(&mut rng);
                    smt.update(k, v).unwrap();
                }
            });
        },
        &[100, 1_000, 10_000],
    );

    c.bench_function_over_inputs(
        "comprehensive/update_all",
        |b, &&size| {
            b.iter(|| {
                let mut rng = seeded_rng();
                let kvs: Vec<(H256, H256)> = (0..size)
                    .map(|_| (random_h256(&mut rng), random_h256(&mut rng)))
                    .collect();
                let mut smt = SMT::default();
                smt.update_all(kvs).unwrap();
            });
        },
        &[100, 1_000, 10_000],
    );

    c.bench_function_over_inputs(
        "comprehensive/get_single",
        |b, &&size| {
            let mut rng = seeded_rng();
            let (smt, keys) = build_smt(size, &mut rng);
            let lookup_key = keys[size / 2];
            b.iter(|| {
                smt.get(&lookup_key).unwrap();
            });
        },
        &[100, 1_000, 10_000],
    );
}

fn criterion_proof_gen_1(c: &mut Criterion) { criterion_proof_gen_n(c, 1); }
fn criterion_proof_gen_5(c: &mut Criterion) { criterion_proof_gen_n(c, 5); }
fn criterion_proof_gen_10(c: &mut Criterion) { criterion_proof_gen_n(c, 10); }
fn criterion_proof_gen_20(c: &mut Criterion) { criterion_proof_gen_n(c, 20); }
fn criterion_proof_gen_40(c: &mut Criterion) { criterion_proof_gen_n(c, 40); }

fn criterion_proof_gen_n(c: &mut Criterion, leaf_count: usize) {
    let tree_size = 10_000usize;
    let mut rng = seeded_rng();
    let (smt, keys) = build_smt(tree_size, &mut rng);
    let proof_keys: Vec<H256> = keys.iter().take(leaf_count).cloned().collect();

    c.bench_function(
        &format!("comprehensive/proof_gen/leaves_{}", leaf_count),
        move |b| {
            b.iter(|| {
                let _ = smt.merkle_proof(proof_keys.clone()).unwrap();
            });
        },
    );
}

fn criterion_proof_verify_1(c: &mut Criterion) { criterion_proof_verify_n(c, 1); }
fn criterion_proof_verify_5(c: &mut Criterion) { criterion_proof_verify_n(c, 5); }
fn criterion_proof_verify_10(c: &mut Criterion) { criterion_proof_verify_n(c, 10); }
fn criterion_proof_verify_20(c: &mut Criterion) { criterion_proof_verify_n(c, 20); }
fn criterion_proof_verify_40(c: &mut Criterion) { criterion_proof_verify_n(c, 40); }

fn criterion_proof_verify_n(c: &mut Criterion, leaf_count: usize) {
    let tree_size = 10_000usize;
    let mut rng = seeded_rng();
    let (smt, keys) = build_smt(tree_size, &mut rng);
    let proof_keys: Vec<H256> = keys.iter().take(leaf_count).cloned().collect();
    let leaves: Vec<(H256, H256)> = proof_keys
        .iter()
        .map(|k| (*k, smt.get(k).unwrap()))
        .collect();
    let root = *smt.root();

    c.bench_function(
        &format!("comprehensive/proof_verify/leaves_{}", leaf_count),
        move |b| {
            b.iter(|| {
                let proof = smt.merkle_proof(proof_keys.clone()).unwrap();
                let valid = proof.verify::<Blake2bHasher>(&root, leaves.clone());
                assert!(valid.expect("verify result"));
            });
        },
    );
}

fn criterion_throughput(c: &mut Criterion) {
    c.bench_function("comprehensive/batch_update_50k", |b| {
        b.iter(|| {
            let mut rng = seeded_rng();
            let kvs: Vec<(H256, H256)> = (0..50_000)
                .map(|_| (random_h256(&mut rng), random_h256(&mut rng)))
                .collect();
            let mut smt = SMT::default();
            smt.update_all(kvs).unwrap();
        });
    });
}

criterion_group!(
    name = criterion_benches;
    config = Criterion::default().sample_size(10);
    targets = criterion_tree_ops,
        criterion_proof_gen_1, criterion_proof_gen_5, criterion_proof_gen_10,
        criterion_proof_gen_20, criterion_proof_gen_40,
        criterion_proof_verify_1, criterion_proof_verify_5, criterion_proof_verify_10,
        criterion_proof_verify_20, criterion_proof_verify_40,
        criterion_throughput
);

// ---------------------------------------------------------------------------
// Main: run report generation first, then criterion
// ---------------------------------------------------------------------------

fn main() {
    // Check if the user passed --report-only (custom flag) via env var
    let report_only = std::env::var("SMT_REPORT_ONLY").is_ok();

    eprintln!("[comprehensive_benchmark] Generating markdown report...");
    generate_report();
    eprintln!("[comprehensive_benchmark] Report generation complete.");

    if report_only {
        eprintln!("[comprehensive_benchmark] SMT_REPORT_ONLY set, skipping criterion benchmarks.");
        return;
    }

    // Run criterion benchmarks (this never returns, it calls process::exit)
    eprintln!("[comprehensive_benchmark] Running criterion statistical benchmarks...");
    criterion_benches();
}
