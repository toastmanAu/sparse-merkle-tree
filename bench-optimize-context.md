# Benchmark Optimization Context

## Project Understanding
Sparse Merkle Tree (SMT) library for CKB blockchain. The benchmark measures SMT proof verification cycles on the CKB RISC-V VM (ckb-debugger). The C implementation in `c/ckb_smt.h` is used via the `smtc` feature for on-chain verification. Test parameters: 131072 keys, 40 leaves, seed 42.

## Architecture Notes

### Hot Path: `smt_calculate_root()` (c/ckb_smt.h:549)
This is the core verification function. It processes a proof (byte stream of opcodes) using a stack-based interpreter:
- **Opcodes**: `0x4C` (push leaf), `0x50` (merge with H256 sibling), `0x51` (merge with MergeWithZero sibling), `0x48` (join two stack entries), `0x4F` (merge with N zeros)
- Each merge operation involves blake2b hashing
- The `_smt_merge()` function is called on every opcode except leaf push

### Key Data Flow
1. Proof bytes are parsed opcode-by-opcode
2. For each opcode, stack entries (key + merge_value) are manipulated
3. `_smt_merge()` determines merge strategy (zero+zero, zero+value, value+zero, value+value)
4. Most merges call `_smt_merge_with_zero()` which either extends a MergeWithZero or hashes a base node
5. Blake2b is the dominant cost — each hash = init + multiple updates + final

### blake2b Usage (the likely bottleneck)
- `ckb_blake2b_init()`: Now replaced with precomputed state memcpy in SMT code
- Each hash: init(memcpy) + 2-4 updates(small) + final(compress+extract)
- The blake2b_compress function itself (12 rounds of G mixing) is the irreducible core cost

### Memory Operations
- `_smt_fast_memcpy` / `_smt_fast_memset`: Custom musl-based implementations. Handle 32-byte key/value copies frequently.
- Stack arrays: `stack_keys[257][32]`, `stack_values[257]` (each ~97 bytes), `stack_heights[257]` — significant stack usage

## What Works
1. **Precomputed blake2b init state** (exp 1): Saved 253 K cycles (3.6%). Memcpy of precomputed state replaces ckb_blake2b_init() calls. Confirms blake2b init overhead was significant.
2. **64-bit word comparisons in `_smt_is_zero_hash`** (exp 3): Saved 4 K cycles (0.1%). Small but simplifies code.
3. **Single byte mask in `_smt_copy_bits`** (exp 4): Saved 627 K cycles (9.3%)! Replaced per-bit clearing loop with single AND mask. `_smt_parent_path` is called extremely frequently — the bit loop was a major bottleneck.

## What Doesn't Work
1. **Batching small blake2b updates into contiguous buffers** (exp 2): 6772 vs 6741 (+31 K cycles). The extra memcpy cost to build the batch buffer outweighs the saved per-update overhead. blake2b_update is already efficient for small inputs since data < 128 bytes never triggers compression — it just copies into the internal buffer.

## Ideas Backlog

### High Impact (algorithmic / memory)
1. **Optimize `_smt_parent_path` further**: Now that `_smt_copy_bits` is fast, consider inlining `_smt_parent_path` entirely or optimizing the memset+mask combo. For small heights, the memset zeros 0-3 bytes which has overhead for the generic memset path.
2. **Word-level `_smt_parent_path`**: Instead of memset + byte mask, use 64-bit stores to zero the prefix. For height < 64, just zero the first uint64 partially and done.
3. **Avoid redundant `_smt_parent_path` calls in opcode 0x4F loop**: The loop calls `_smt_parent_path(parent_key, height_u16)` each iteration, but parent_path of a parent_path could be computed incrementally (just clear one more bit).

### Medium Impact (compiler hints)
4. **Force function inlining**: Add `__attribute__((always_inline))` to hot path functions like `_smt_merge`, `_smt_merge_with_zero`, `_smt_merge_value_hash`, `_smt_get_bit`, etc.
5. **Mark hot/cold paths**: Use `__builtin_expect` for unlikely error paths.

### Lower Impact (memory/micro)
6. **Reduce stack size**: `SMT_STACK_SIZE=257` might be larger than needed for 40 leaves. Smaller stack = better cache behavior.
7. **Optimize `_smt_fast_memcpy` for 32-byte fixed-size copies**: Use specialized 32-byte copy using 64-bit loads/stores.

## Approach Categories Tried
| Category | Attempts | Kept | Last Tried |
|----------|----------|------|------------|
| caching | 1 | 1 | exp 1 - precomputed blake2b init |
| io-optimization | 1 | 0 | exp 2 - batch blake2b updates (regressed) |
| memory-layout | 1 | 1 | exp 3 - 64-bit zero hash check |
| algorithm | 1 | 1 | exp 4 - single byte mask in _smt_copy_bits |
