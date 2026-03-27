# Benchmark Optimization Context

## Project Understanding
Sparse Merkle Tree (SMT) library for CKB blockchain. The benchmark measures SMT proof verification cycles on the CKB RISC-V VM (ckb-debugger). The C implementation in `c/ckb_smt.h` is used via the `smtc` feature for on-chain verification. Test parameters: 131072 keys, 40 leaves, seed 42.

## Current Best: 2454 K cycles (baseline: 6994, total improvement: 64.9%)

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
10. **Optimize blake2b init - copy only h[] zero rest** (exp 14): Saved 324 K cycles (9.0%).
11. **Skip buf[] zeroing in blake2b_init_fast** (exp 15): Saved 81 K cycles (2.5%). buf is filled by update and padded by final — initial zeroing is redundant.
12. **Remove secure_zero_memory in blake2b_final** (exp 17): Saved 123 K cycles (3.8%). volatile memset ptr prevented compiler optimization — unnecessary for non-keyed SMT.
13. **Direct buf write in _smt_hash_base_node** (exp 21): Saved 20 K cycles (0.7%). Write 65 bytes directly to blake2b buf instead of 3 blake2b_update calls.
14. **Direct buf write in _smt_merge_value_hash** (exp 22): Saved 58 K cycles (1.9%). Write 66 bytes directly to buf.
15. **Direct buf write in _smt_merge + hash output to buf** (exp 23): Saved 250 K cycles (8.4%)! Write 98 bytes directly to buf AND have _smt_merge_value_hash write output directly to target buf position.
16. **Custom _smt_blake2b_final** (exp 24): Saved 290 K cycles (10.6%)! Skip error checks, temp buffer, store64 loop + memcpy. Write h[] directly to output. Also skip outlen/last_node init.

## What Doesn't Work
1. **Batching small blake2b updates** (exp 2): +31 K cycles.
2. **64-bit word zeroing in `_smt_parent_path`** (exp 5): +217 K cycles.
3. **Force-inline blake2b_update/blake2b_final** (exp 10): No effect. Compiler already inlines them.
4. **Specialized 32-byte memset-zero** (exp 12): +37 K cycles. The existing _smt_fast_memset is efficient for n<=32.
5. **Field-by-field struct copy in _smt_merge_with_zero** (exp 16): No effect. Path not hit frequently enough.
6. **Skip temp buffer in blake2b_final** (exp 18): No effect. Compiler already optimized after secure_zero_memory removal.
7. **Define NATIVE_LITTLE_ENDIAN for RISC-V** (exp 19): No effect. Compiler already optimizes byte-shift pattern.
8. **Direct _smt_merge_with_zero in 0x4F** (exp 20): +1.1%. Inlined _smt_merge with const SMT_ZERO was better optimized by compiler.
9. **Force-inline blake2b_compress** (exp 25): No effect. Compiler already inlines it.
10. **Zero buf in init, skip padding in final** (exp 26): +1.5%. The 128-byte memset in init costs more than variable padding in final (30-63 bytes).

## Ideas Backlog

### High Impact (algorithmic / memory)
1. **Direct buf write technique for remaining blake2b sites**: The pattern of writing directly to S->buf and setting buflen has proven very effective. Look for any remaining blake2b_update call sites.
2. **Reduce blake2b calls**: In `_smt_merge_with_zero`, when converting a VALUE to MERGE_WITH_ZERO, it calls `_smt_hash_base_node` doing a full blake2b hash. Can this be deferred?
3. **Optimize blake2b_compress itself**: The G macro, ROUND macro — any RISC-V specific optimizations?

### Medium Impact
4. **Optimize _smt_copy_bits _smt_fast_memset**: In `_smt_copy_bits`, `_smt_fast_memset(source, 0, first_byte)` is called with variable small sizes. Could be optimized for common cases.
5. **Reduce stack memory**: `stack_values[257]` × 97 bytes = ~25 KB on stack. Consider if smaller stack helps cache behavior.

### Lower Impact
6. **Reduce `SMT_STACK_SIZE`**: 257 might be larger than needed for 40 leaves.
7. **Optimize proof parsing**: Reduce branching in the switch statement.

## Approach Categories Tried
| Category | Attempts | Kept | Last Tried |
|----------|----------|------|------------|
| caching | 1 | 1 | exp 1 - precomputed blake2b init |
| io-optimization | 1 | 0 | exp 2 - batch blake2b updates (regressed) |
| memory-layout | 10 | 5 | exp 26 - zero buf in init, skip padding in final (regressed +1.5%) |
| algorithm | 8 | 7 | exp 24 - custom blake2b_final (10.6% win!) |
| compiler-hint | 5 | 2 | exp 25 - force-inline blake2b_compress (no effect) |
