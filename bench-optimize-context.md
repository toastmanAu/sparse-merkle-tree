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

## What Doesn't Work
(None yet)

## Ideas Backlog

### High Impact (blake2b optimization)
1. **Combine small blake2b_updates into single buffer**: Many hash calls do 3-4 small updates (1 byte + 32 bytes + 32 bytes + ...). Buffer these and do a single update to avoid per-update overhead and buffer management.
2. **Batch hash inputs for _smt_merge**: In `_smt_merge()`, we do blake2b(MERGE_NORMAL(1) + height(1) + node_key(32) + lhs_hash(32) + rhs_hash(32)) = 98 bytes total. Build this in a contiguous buffer and do a single blake2b_update.
3. **Similar batching for _smt_hash_base_node**: height(1) + key(32) + value(32) = 65 bytes in one update.
4. **Similar batching for _smt_merge_value_hash**: MERGE_ZEROS(1) + value(32) + zero_bits(32) + zero_count(1) = 66 bytes in one update.

### Medium Impact (algorithmic)
5. **Optimize `_smt_parent_path()`**: Called frequently. The `_smt_copy_bits()` inside it does byte-by-byte bit clearing which could be done more efficiently with word-level ops.
6. **Optimize `_smt_is_zero_hash()`**: Use 64-bit word comparisons instead of byte-by-byte loop.

### Lower Impact (memory/micro)
7. **Reduce stack size**: `SMT_STACK_SIZE=257` might be larger than needed for 40 leaves. Smaller stack = better cache behavior.
8. **Force function inlining**: Add `__attribute__((always_inline))` to hot path functions.

## Approach Categories Tried
| Category | Attempts | Kept | Last Tried |
|----------|----------|------|------------|
| caching | 1 | 1 | exp 1 - precomputed blake2b init |
