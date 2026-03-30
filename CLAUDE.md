# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Purpose

kodex is a compiler-precise Scala code intelligence CLI, written in Rust. It indexes SemanticDB data from compiled Scala projects and answers structural questions (call graphs, type hierarchies, cross-module references) in ~50ms. This tool is for **reading code, not writing code** — do not add features for code modification, refactoring, or write-side workflows.

## Build & Test

```bash
cargo test                    # all tests (unit + integration + insta snapshots)
cargo build --release         # release binary (required before shell snapshot tests)
cargo insta review            # accept new/changed snapshots
make coverage                 # test coverage summary (cargo-llvm-cov)
make bench                    # criterion benchmarks
```

Shell snapshot tests run against a real compiled project:
```bash
PROJECT=/path/to/project tests/snapshot/run.sh
```
**Always `cargo build --release` after source edits and before running shell snapshot tests.**

## Architecture

```
src/
├── main.rs           CLI entry (clap). All cmd_* functions return String; main wraps with print!
├── model.rs          Index schema — rkyv zero-copy types (KodexIndex, Symbol, EdgeList, etc.)
├── ingest/           SemanticDB → index pipeline
│   ├── provider.rs   BuildProvider trait + auto-detection (Mill/sbt/scala-cli/fallback)
│   ├── merge.rs      build_index() — 12-phase pipeline. validate_index() checks invariants.
│   ├── classify.rs   Test/generated file classification
│   ├── interner.rs   String deduplication table
│   ├── mill.rs       Mill adapter (reads out/ JSON caches, zero CLI calls)
│   ├── sbt.rs        sbt adapter
│   └── scala_cli.rs  scala-cli adapter
├── index/
│   ├── reader.rs     mmap + rkyv::access for zero-copy reads
│   └── writer.rs     Atomic write (.idx.tmp → rename), magic header "KODEX\x00\x00\x00"
└── query/
    ├── symbol.rs     resolve_symbols() — 9-step cascade (exact FQN → fuzzy)
    ├── filter.rs     Kind/module/exclude filtering + built-in noise lists
    ├── format.rs     Pretty-printing
    ├── graph.rs      Call/inheritance graph traversal
    └── commands/     search, info, calls, refs, overview, noise — each returns String
```

**Key data flow:** `.semanticdb` files → `provider.rs` discovers → `merge.rs` builds index → `writer.rs` serializes → `reader.rs` mmaps → `commands/` queries

## Project Stage

Early stage — breaking changes are expected and welcome. Do not add backwards-compatibility shims, aliases, or deprecation paths. Just change things directly.

## Code Conventions

- `#![deny(unused)]` in main.rs — no dead code allowed
- `#![warn(clippy::pedantic)]` in lib.rs with targeted allows
- Use `std::fmt::Write` + `writeln!(out, ...)` for building output in cmd_* functions
- `edges_from()` does binary search on sorted edge lists — edge lists MUST stay sorted by `from`
- All strings go through `intern()` during index build (string table deduplication)
- Index version: `KODEX_INDEX_VERSION = 10` in model.rs — bump on schema changes

## Testing

- `validate_index()` in `src/ingest/merge.rs` checks structural invariants (ID bounds, sorted edges, graph symmetry). Called via `debug_assert!` in `build_index()`. Call it in new tests.
- Shared fixture: `tests/common/mod.rs` — `make_billing_test_docs()` builds a billing system fixture (1 module, 5 symbols with trait, impl, override, call graph)
- `insta` for snapshot testing. `cargo insta review` to accept changes.
- Shell snapshots in `tests/snapshot/` test against a real project; Rust tests in `tests/` are self-contained.

## Branch Policy

**Never push directly to main.** All changes must go through a pull request. Create a feature branch, commit there, and open a PR.

## Release Workflow

1. Update `CHANGELOG.md` — move `[Unreleased]` content to `[X.Y.Z] — YYYY-MM-DD` section
2. Bump version in `Cargo.toml` and run `cargo check` to update `Cargo.lock`
3. Commit, merge to main
4. Tag and push: `git tag vX.Y.Z && git push origin vX.Y.Z`
5. GitHub Actions builds binaries (macOS ARM64, macOS x64, Linux x64) and creates release with changelog body extracted via awk
6. Update bootstrap script checksums: download `.sha256` files from the release, update `EXPECTED_VERSION` and `CHECKSUM_*` vars in the kodex skill's `scripts/kodex-cli`
7. Bump `version` in `.claude-plugin/marketplace.json` to match `Cargo.toml`

**Changelog format:** Keep a Changelog style — `## [VERSION] — DATE` headers, `### Added`/`### Changed`/`### Fixed` sections. The release workflow extracts the matching version section with awk.
