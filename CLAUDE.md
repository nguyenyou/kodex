# kodex

Compiler-precise Scala **knowledge base generator and explorer**, written in Rust.
Indexes SemanticDB data from compiled Scala projects. Queries complete in ~50ms on 1.4M-symbol codebases.

## Purpose

kodex exists to help AI coding agents (and humans) **understand and explore** codebases as fast as possible. Every feature must serve this goal — answering "how does X work?" questions with minimal tool calls. Do NOT add features for code modification, refactoring, migration, or any write-side workflow. If a feature doesn't help someone understand the codebase, it doesn't belong in kodex.

## Build & Test

- `cargo test` — run all tests (unit + integration + insta snapshots)
- `cargo build --release` — build release binary (required before shell snapshots)
- `make coverage` — test coverage summary (cargo-llvm-cov)
- `make bench` — criterion benchmarks
- Shell snapshot tests: `PROJECT=/path/to/project tests/snapshot/run.sh`
- IMPORTANT: Always `cargo build --release` AFTER source edits and BEFORE running shell snapshot tests.

## Architecture

- **Ingest**: `src/ingest/` — discovery, semanticdb parsing, merge (index building), mill metadata, printer
- **Index**: `src/index/` — reader (mmap + rkyv zero-copy), writer (atomic file write)
- **Query**: `src/query/` — symbol resolution (trigram + hash indexes), graph (callers), filter, format
- **Commands**: `src/query/commands/` — search, info, call_graph, noise
- All `cmd_*` functions return `String` (not println). `main.rs` wraps with `print!`.

## Project stage

Early stage — breaking changes are expected and welcome. Do not add backwards-compatibility shims, aliases, or deprecation paths. Just change things directly.

## Testing

- `validate_index()` in `src/ingest/merge.rs` — checks structural invariants (ID bounds, sorted edges, graph symmetry). Called via `debug_assert!` in `build_index()`. Call it in new tests.
- Shared test fixture: `tests/common/mod.rs` — `make_rich_test_docs()` (2 modules, 8 symbols), `build_and_load_index()` (roundtrip helper)
- Shell snapshots in `tests/snapshot/` test against a real project; Rust tests in `tests/` are self-contained.
- `insta` crate available for snapshot testing. Use `cargo insta review` to accept new snapshots.

## Code Conventions

- `#![deny(unused)]` enforced in main.rs — no dead code allowed
- Use `std::fmt::Write` + `writeln!(out, ...)` for building output strings in cmd_* functions
- `edges_from()` does binary search on sorted edge lists — edge lists MUST stay sorted by `from`
- String table deduplication: all strings go through `intern()` during index build
