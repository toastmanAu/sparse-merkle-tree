# Benchmark Optimization Context

## Project Understanding
Sparse Merkle Tree (SMT) library for CKB blockchain. The benchmark measures SMT proof verification cycles on the CKB RISC-V VM (ckb-debugger). The C implementation in `c/ckb_smt.h` is used via the `smtc` feature for on-chain verification. Test parameters: 131072 keys, 40 leaves, seed 42.

## Architecture Notes

### Hot Path: `smt_calculate_root()` (c/ckb_smt.h:527)
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
- `ckb_blake2b_init()`: Called on every hash. Sets up the param block (memsets, personalizes with "ckb-default-hash"), then calls `blake2b_init_param()`
- `blake2b_init_param()`: Calls `blake2b_init0()` which does `memset(S, 0, sizeof(blake2b_state))` — 256-byte zero fill!
- Each hash: init(expensive) + 2-4 updates(small) + final(compress+extract)
- The init path: memset 256 bytes, XOR 8 uint64s, set personal bytes, etc.

### Memory Operations
- `_smt_fast_memcpy` / `_smt_fast_memset`: Custom musl-based implementations. Handle 32-byte key/value copies frequently.
- Stack arrays: `stack_keys[257][32]`, `stack_values[257]` (each ~97 bytes), `stack_heights[257]` — significant stack usage

## What Works
(None yet — baseline established)

## What Doesn't Work
(None yet)

## Ideas Backlog

### High Impact (blake2b optimization)
1. **Precompute blake2b init state**: `ckb_blake2b_init()` always uses same params (outlen=32, personal="ckb-default-hash"). Pre-compute the initialized state and memcpy it instead of recalculating every time. Saves: memset + param setup + XOR loop per hash.
2. **Avoid `blake2b_init0` full memset**: The `blake2b_init0()` zeros all 256 bytes of `blake2b_state`, but `blake2b_init_param()` immediately overwrites `h[0..8]`. Only need to zero `t`, `f`, `buf`, `buflen`, `outlen`, `last_node`.
3. **Combine small blake2b_updates into single buffer**: Many hash calls do 3-4 small updates (1 byte + 32 bytes + 32 bytes + ...). Buffer these and do a single update to avoid per-update overhead.

### Medium Impact (algorithmic)
4. **Optimize `_smt_merge_with_zero` base node hashing**: When converting VALUE to MERGE_WITH_ZERO, it calls `_smt_hash_base_node()` which does a full blake2b. Could we defer this hash?
5. **Inline `_smt_merge_value_hash()`**: It's called in the hot merge path. Inlining could save function call overhead on RISC-V.
6. **Optimize `_smt_parent_path()`**: Called frequently. The `_smt_copy_bits()` inside it does byte-by-byte bit clearing which could be done more efficiently with word-level ops.

### Lower Impact (memory/micro)
7. **Use `__builtin_memcpy` / `__builtin_memset`** instead of custom fast versions for known-size copies (32 bytes) — compiler may emit optimal RISC-V instructions.
8. **Reduce stack size**: `SMT_STACK_SIZE=257` might be larger than needed for 40 leaves. Smaller stack = better cache behavior.
9. **Optimize `_smt_is_zero_hash()`**: Use 64-bit word comparisons instead of byte-by-byte loop.

## Approach Categories Tried
| Category | Attempts | Kept | Last Tried |
|----------|----------|------|------------|
