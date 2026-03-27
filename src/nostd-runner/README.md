# smt-nostd-runner

This is a Sparse Merkle Tree (SMT) verification runner on ckb-vm, with test and benchmark purpose.
It is compiled with Rust no-std support targeting `riscv64imac-unknown-none-elf`.

## Usage

The binary accepts command-line arguments in hex format:

```
<root_hash> <proof> [<key_1> <value_1> <key_2> <value_2> ... <key_N> <value_N>]
```

- `root_hash` — 32-byte expected SMT root, hex-encoded (64 hex chars)
- `proof` — compiled Merkle proof, hex-encoded (variable length)
- `key_N` / `value_N` — 32-byte key-value pairs to verify membership, hex-encoded (64 hex chars each)

All key-value pairs must be provided together; an odd number of remaining arguments is an error.

## Build

```bash
make build
```

## Benchmark

make SMT_COUNT=16 SMT_LEAVES=1 test
make SMT_COUNT=131072 SMT_LEAVES=1 test

| Key/Value Pairs in Merkle Tree | Key/Value Pairs Verified at the Same Time | Cycles |
| -------------------------------- | ----------------------------------------- | ------ |
| 16                               | 1                                         | 116 K  |
| 256                              | 1                                         | 155 K  |
| 2,048                            | 1                                         | 150 K  |
| 16,384                           | 1                                         | 174 K  |
| 131,072                          | 1                                         | 203 K  |
| 131,072                          | 10                                        | 1,923 K |
| 131,072                          | 20                                        | 3,632 K |
| 131,072                          | 40                                        | 6,919 K |
