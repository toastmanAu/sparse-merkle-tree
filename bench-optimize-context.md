# Benchmark Optimization Context

## Project Understanding
Sparse Merkle Tree (SMT) library for CKB blockchain. The benchmark measures SMT proof verification cycles on the CKB RISC-V VM (ckb-debugger). The C implementation in `c/ckb_smt.h` is used via the `smtc` feature for on-chain verification. Test parameters: 131072 keys, 40 leaves, seed 42.

## Current Best: 4342 K cycles (baseline: 6994, total improvement: 37.9%)

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
1. **Precomputed blake2b init state** (exp 1): Saved 253 K cycles (3.6%). Memcpy of precomputed state replaces ckb_blake2b_init() calls.
2. **64-bit word comparisons in `_smt_is_zero_hash`** (exp 3): Saved 4 K cycles (0.1%). Small but simplifies code.
3. **Single byte mask in `_smt_copy_bits`** (exp 4): Saved 627 K cycles (9.3%)! Replaced per-bit clearing loop with single AND mask.
4. **Force inlining `_smt_merge_with_zero` and `_smt_merge`** (exp 6): Saved 1064 K cycles (17.4%)! Massive win. Function call overhead on RISC-V is expensive.
5. **Incremental parent_path in 0x4F loop** (exp 7): Saved 704 K cycles (13.9%)! Replaced full `_smt_parent_path` call per iteration with single `_smt_clear_bit`. Since heights increase monotonically, each iteration only needs to clear one additional bit.

## What Doesn't Work
1. **Batching small blake2b updates into contiguous buffers** (exp 2): +31 K cycles. blake2b_update is already efficient for small inputs.
2. **64-bit word zeroing in `_smt_parent_path`** (exp 5): +217 K cycles. Loop-based word zeroing slower than `_smt_fast_memset`.

## Ideas Backlog

### High Impact (algorithmic / memory)
1. **Specialized 32-byte memcpy**: Use 4x uint64_t loads/stores for the very common 32-byte copy case instead of generic _smt_fast_memcpy.
2. **Reduce redundant parent_key computation**: In opcodes 0x50/0x51, `_smt_parent_path` is called twice (once for parent_key, once for key). Could compute once and reuse (just memcpy parent_key to key).
3. **Inline blake2b_update/blake2b_final**: Force-inline the blake2b functions. If they aren't already inlined, this could give a similar win to exp 6.

### Medium Impact (compiler hints)
4. **`__builtin_expect` for unlikely error paths**: Branch prediction hints to move error handling out of the hot path.
5. **Optimize merge_with_zero fast path**: When extending a MergeWithZero (already has base_node), avoid the memcpy when out==v by restructuring the branch.

### Lower Impact (memory/micro)
6. **Reduce `SMT_STACK_SIZE`**: 257 might be larger than needed for 40 leaves. Smaller stack = less memory pressure.
7. **Eliminate parent_key local variable**: In 0x50/0x51, compute parent_path in-place on key, then use key as parent_key for merge.

## Approach Categories Tried
| Category | Attempts | Kept | Last Tried |
|----------|----------|------|------------|
| caching | 1 | 1 | exp 1 - precomputed blake2b init |
| io-optimization | 1 | 0 | exp 2 - batch blake2b updates (regressed) |
| memory-layout | 1 | 1 | exp 3 - 64-bit zero hash check |
| algorithm | 3 | 2 | exp 7 - incremental parent_path (huge win) |
| compiler-hint | 1 | 1 | exp 6 - always_inline merge functions (huge win) |
