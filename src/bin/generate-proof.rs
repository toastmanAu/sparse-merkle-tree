// Generate a random SMT and print its root hash, compiled merkle proof, and
// proven key/value pairs in hex format compatible with nostd-runner/src/entry.rs.
//
// Usage:
//   generate-proof [SMT_SIZE [TARGET_LEAVES_COUNT [SEED]]]
//
//   SMT_SIZE            Number of key/value pairs inserted into the SMT (default: 16).
//   TARGET_LEAVES_COUNT Number of leaves to include in the proof (default: 1).
//   SEED                u64 seed for deterministic output (default: random).
//
// Output (single space-separated line, all values hex-encoded):
//   <root_hash> <compiled_proof> <key1> <value1> <key2> <value2> ...
//
// Examples:
//   generate-proof                  # random SMT of 16 entries, 1 leaf proved, random seed
//   generate-proof 500              # random SMT of 500 entries, 1 leaf proved, random seed
//   generate-proof 500 4            # SMT of 500 entries, 4 leaves proved, random seed
//   generate-proof 500 4 42         # SMT of 500 entries, 4 leaves proved, deterministic seed 42
use rand::{rngs::StdRng, thread_rng, Rng, RngCore, SeedableRng};
use sparse_merkle_tree::{
    blake2b::{Blake2b, Blake2bBuilder},
    default_store::DefaultStore,
    traits::Hasher,
    SparseMerkleTree, H256,
};

const DEFAULT_SMT_SIZE: usize = 16;
const DEFAULT_TARGET_LEAVES_COUNT: usize = 1;

// Uses personal "ckb-default-hash" to match the C blake2b implementation in ckb_smt.c
struct CkbBlake2bHasher(Blake2b);

impl Default for CkbBlake2bHasher {
    fn default() -> Self {
        let blake2b = Blake2bBuilder::new(32)
            .personal(b"ckb-default-hash")
            .build();
        CkbBlake2bHasher(blake2b)
    }
}

impl Hasher for CkbBlake2bHasher {
    fn write_byte(&mut self, b: u8) {
        self.0.update(&[b][..]);
    }
    fn write_h256(&mut self, h: &H256) {
        self.0.update(h.as_slice());
    }
    fn finish(self) -> H256 {
        let mut hash = [0u8; 32];
        self.0.finalize(&mut hash);
        hash.into()
    }
}

type SMT = SparseMerkleTree<CkbBlake2bHasher, H256, DefaultStore<H256>>;

fn random_h256(rng: &mut impl Rng) -> H256 {
    let mut buf = [0u8; 32];
    rng.fill(&mut buf);
    buf.into()
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn main() {
    let smt_size = std::env::args()
        .nth(1) // arg 1: SMT_SIZE
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_SMT_SIZE);

    enum AnyRng {
        Seeded(StdRng),
        Random(rand::rngs::ThreadRng),
    }
    impl RngCore for AnyRng {
        fn next_u32(&mut self) -> u32 {
            match self {
                AnyRng::Seeded(r) => r.next_u32(),
                AnyRng::Random(r) => r.next_u32(),
            }
        }
        fn next_u64(&mut self) -> u64 {
            match self {
                AnyRng::Seeded(r) => r.next_u64(),
                AnyRng::Random(r) => r.next_u64(),
            }
        }
        fn fill_bytes(&mut self, dest: &mut [u8]) {
            match self {
                AnyRng::Seeded(r) => r.fill_bytes(dest),
                AnyRng::Random(r) => r.fill_bytes(dest),
            }
        }
        fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
            match self {
                AnyRng::Seeded(r) => r.try_fill_bytes(dest),
                AnyRng::Random(r) => r.try_fill_bytes(dest),
            }
        }
    }
    let target_leaves_count = std::env::args()
        .nth(2) // arg 2: TARGET_LEAVES_COUNT
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_TARGET_LEAVES_COUNT);

    let mut rng = match std::env::args().nth(3).and_then(|s| s.parse::<u64>().ok()) {
        // arg 3: SEED
        Some(seed) => AnyRng::Seeded(StdRng::seed_from_u64(seed)),
        None => AnyRng::Random(thread_rng()),
    };

    let mut smt = SMT::default();
    let mut keys = Vec::with_capacity(smt_size);
    for _ in 0..smt_size {
        let key = random_h256(&mut rng);
        let value = random_h256(&mut rng);
        smt.update(key, value).unwrap();
        keys.push(key);
    }
    keys.dedup();

    let proof_keys: Vec<H256> = keys.into_iter().take(target_leaves_count).collect();

    let leaves: Vec<(H256, H256)> = proof_keys
        .iter()
        .map(|k| (*k, smt.get(k).unwrap()))
        .collect();

    let proof = smt.merkle_proof(proof_keys.clone()).unwrap();
    let compiled_proof: Vec<u8> = proof.compile(proof_keys).unwrap().into();

    let root = smt.root();

    // Output format (all hex): <root_hash> <proof> <key1> <value1> <key2> <value2> ...
    // This matches the argument format consumed by nostd-runner/src/entry.rs
    let mut parts: Vec<String> = vec![to_hex(root.as_slice()), to_hex(&compiled_proof)];
    for (key, value) in &leaves {
        parts.push(to_hex(key.as_slice()));
        parts.push(to_hex(value.as_slice()));
    }

    println!("{}", parts.join(" "));
}
