# Benchmark Optimization Context

## Project Understanding
Sparse Merkle Tree (SMT) library for CKB blockchain. The benchmark measures SMT proof verification cycles on the CKB RISC-V VM (ckb-debugger). The C implementation in `c/ckb_smt.h` is used via the `smtc` feature for on-chain verification. Test parameters: 131072 keys, 40 leaves, seed 42.

## Current Best: 3600 K cycles (baseline: 6994, total improvement: 48.5%)

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

### blake2b Usage
- `ckb_blake2b_init()`: Replaced with precomputed state memcpy
- Each hash: init(memcpy) + 2-4 updates(small) + final(compress+extract)
- The blake2b_compress function (12 rounds of G mixing) is the irreducible core cost

### Memory Operations
- `_smt_fast_memcpy` / `_smt_fast_memset`: Custom musl-based implementations
- Stack arrays: `stack_keys[257][32]`, `stack_values[257]` (each ~97 bytes), `stack_heights[257]`

## What Works
1. **Precomputed blake2b init state** (exp 1): Saved 253 K cycles (3.6%).
2. **64-bit word comparisons in `_smt_is_zero_hash`** (exp 3): Saved 4 K cycles (0.1%).
3. **Single byte mask in `_smt_copy_bits`** (exp 4): Saved 627 K cycles (9.3%)!
4. **Force inlining `_smt_merge_with_zero` and `_smt_merge`** (exp 6): Saved 1064 K cycles (17.4%)!
5. **Incremental parent_path in 0x4F loop** (exp 7): Saved 704 K cycles (13.9%)!
6. **Eliminate redundant parent_key in 0x50/0x51/0x48** (exp 8): Saved 124 K cycles (2.9%).
7. **`__builtin_expect` for unlikely error paths** (exp 9): Saved 16 K cycles (0.4%). Small but real.
8. **Specialized 32-byte memcpy** (exp 11): Saved 595 K cycles (14.2%)!
9. **Specialized 32-byte memcmp** (exp 13): Saved 7 K cycles (0.2%). Small win with uint64_t XOR comparisons.

## What Doesn't Work
1. **Batching small blake2b updates** (exp 2): +31 K cycles.
2. **64-bit word zeroing in `_smt_parent_path`** (exp 5): +217 K cycles.
3. **Force-inline blake2b_update/blake2b_final** (exp 10): No effect. Compiler already inlines them.
4. **Specialized 32-byte memset-zero** (exp 12): +37 K cycles. The existing _smt_fast_memset is efficient for n<=32.

## Ideas Backlog

### High Impact (algorithmic / memory)
1. **Apply _smt_memcpy32 to more call sites**: The blake2b_init_fast still uses _smt_fast_memcpy for sizeof(blake2b_state) — not 32 bytes though. Also _smt_merge_with_zero copies sizeof(_smt_merge_value_t) which is ~97 bytes.
2. **Specialized memset32**: Similar to _smt_memcpy32 but for zeroing — use 4x uint64_t zero stores for the common 32-byte memset(0) case in _smt_merge_value_zero and _smt_merge_with_zero.
3. **Reduce blake2b calls**: In `_smt_merge_with_zero`, when converting a VALUE to MERGE_WITH_ZERO, it calls `_smt_hash_base_node` doing a full blake2b hash. Can this be deferred?

### Medium Impact
4. **Optimize merge_with_zero struct copy**: When `out != v` and extending MergeWithZero, we copy the full 97-byte struct. Use _smt_memcpy32 for the value and zero_bits fields separately.
5. **Specialized 32-byte memcmp**: Replace memcmp in 0x48 with 64-bit word comparison like _smt_is_zero_hash.

### Lower Impact
6. **Reduce `SMT_STACK_SIZE`**: 257 might be larger than needed for 40 leaves.
7. **Avoid the proof[proof_index] copy**: In the 0x51 sibling copy from proof, data may be unaligned — check if the RISC-V target handles unaligned loads efficiently.

## Approach Categories Tried
| Category | Attempts | Kept | Last Tried |
|----------|----------|------|------------|
| caching | 1 | 1 | exp 1 - precomputed blake2b init |
| io-optimization | 1 | 0 | exp 2 - batch blake2b updates (regressed) |
| memory-layout | 4 | 3 | exp 13 - specialized memcmp (0.2% win), exp 12 memset regressed |
| algorithm | 4 | 3 | exp 8 - eliminate redundant parent_key (2.9% win) |
| compiler-hint | 3 | 2 | exp 10 - inline blake2b (no effect, discarded) |
