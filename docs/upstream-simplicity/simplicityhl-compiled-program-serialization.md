# Request: CompiledProgram Serialization Support in simplicityhl

## Context

Deadcat is a prediction market protocol built on Liquid/Elements using Simplicity covenants. The `deadcat-core` library compiles Simplicity contracts from `.simf` source via simplicityhl, producing `CompiledProgram` instances used for witness encoding when constructing spending transactions (PSETs).

## Desired Behavior

We want to compile a Simplicity contract once (at contract ingestion time), persist the compiled representation to disk, and reconstruct it later for efficient witness encoding — without recompiling from `.simf` source each time a spending transaction is built.

This would allow us to amortize the compilation cost (~10-100ms per contract) to a one-time event at ingestion, rather than paying it on every PSET construction.

## Current Limitation

`CompiledProgram` is an opaque type with no serialization/deserialization API. The only path to a `CompiledProgram` is:

```rust
let template = TemplateProgram::new(SOURCE)?;
let program = template.instantiate(arguments, false)?;
// program is a CompiledProgram — can satisfy witnesses, but cannot be serialized
```

We investigated alternative paths:
- `CommitNode::encode` / `CommitNode::decode` — these are for consensus serialization, not for reconstructing a satisfiable program
- `RedeemNode::decode` — reconstructs an executable node from serialized bytes, but it cannot be re-satisfied with different witness values
- There is no reverse path from `CommitNode` or `RedeemNode` back to `CompiledProgram`

## Requested API

A serialization round-trip for `CompiledProgram`:

```rust
// Serialize a compiled program to bytes (for disk persistence)
let bytes: Vec<u8> = compiled_program.to_bytes();

// Reconstruct from bytes (for later witness satisfaction)
let restored: CompiledProgram = CompiledProgram::from_bytes(&bytes)?;

// The restored program supports the same operations as the original
let satisfied = restored.satisfy_with_env(witness_values, env)?;
```

The exact API shape is flexible — `serde::Serialize`/`Deserialize` implementations, dedicated methods, or any other approach that enables persist-and-restore would work.

## Impact

This pattern is expected to be common in the Simplicity covenant ecosystem. Any application that manages covenant-controlled assets (prediction markets, stablecoins, governance tokens, synthetic assets) would benefit from compiling once and persisting, rather than recompiling on every transaction construction.

Without this capability, applications must choose between:
- **Recompile on demand** (~10-100ms per PSET build) — acceptable but wasteful
- **In-memory cache** — adds complexity (eviction, interior mutability) for marginal benefit within a single process lifetime
