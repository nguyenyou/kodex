# Changelog

## [Unreleased]

## [1.1.0] — 2026-03-30

### Changed
- Noise filtering is now **on by default** — `search`, `info`, and `calls` automatically exclude generated code (ScalaPB, protobuf), test files, stdlib symbols, and plumbing methods (apply, map, flatMap, etc.) without any flags
- `--noise-filter` flag renamed to `--include-noise` with inverted semantics — pass it to see everything, including noise
- `--exclude` patterns now combine with default noise filtering instead of replacing it; use `--include-noise --exclude "P"` to disable auto-noise and apply only manual patterns
- `search` command applies baseline `is_noise()` filtering to candidates — previously only filtered when `--noise-filter` was explicitly passed

## [1.0.0] — 2026-03-30

### Added

**Core commands:**
- `overview` — codebase summary: total symbols, files, and all modules sorted by symbol count
- `search` — 9-step symbol resolution cascade: exact FQN → FQN suffix → Owner.member (dotted, nested) → exact name (O(1) hash) → prefix → substring (trigram index) → substring (linear fallback) → CamelCase abbreviation → fuzzy (Damerau-Levenshtein)
- `info` — complete picture of a symbol in one call: metadata, signature, owner, overrides, parents, members, implementations, call graph (callers + callees at depth 1), and full source code
- `calls` — recursive call tree traversal with box-drawing connectors; supports downstream (callees) and upstream (`-r` / `--reverse` callers); configurable `--depth`; marks cross-module boundaries; detects cycles
- `refs` — all reference locations for a symbol, grouped by module then file; shows total counts and module-level summary
- `noise` — analyzes the index and categorizes noisy symbols across 5 categories: effect plumbing, hub utilities, ID factories, store/repository CRUD ops, infrastructure plumbing; outputs ready-to-use `--exclude` pattern
- `index` — build `kodex.idx` from a compiled project's SemanticDB output

**Indexing pipeline:**
- Fuses compiler knowledge (SemanticDB — resolved symbols, types, call targets, overrides) with build-tool knowledge (module structure, dependencies) into a single fast index
- Auto-detects build tool: Mill (`build.mill`/`build.sc`), sbt (`build.sbt`), scala-cli (`.scala-build/`), with generic fallback
- Mill provider reads JSON cache from `out/` for module metadata, artifact names, Scala versions, dependencies, and generated source paths
- sbt provider extracts metadata from `target/` path structure including test detection via `test-meta/`
- File classification at index time: test files (by metadata, module segments, or path patterns), generated files (ScalaPB, protobuf, `src_managed`, `BuildInfo`)
- Cross-compiled sources (`jsSharedSources.dest`, `jvmSharedSources.dest`) correctly classified as source, not generated
- 12-phase index merge: file collection → symbol assignment → owner resolution → references → inheritance → members → overrides → call graph → build metadata → module stats → trigram/hash indexes → reverse dependency graph
- Parallel SemanticDB loading and trigram index construction via rayon
- rkyv zero-copy deserialization for fast index loading

**Search & resolution:**
- Composite ranking: kind priority (class/trait > object > type > method > field), source type (source > test > generated), popularity (log-dampened ref count), name length
- CamelCase matching with two complementary matchers: segment matching (`hcf` → `HttpClientFactory`) and character subsequence (`lpfuse` → `linkProfileForUser`)
- Fuzzy matching via Damerau-Levenshtein distance with configurable threshold
- Trigram index for fast substring search on large codebases
- Module filtering with dotted segment matching (`storage.jvm` matches `modules.storage.storage.jvm`)
- Kind filtering: class, trait, object, method, field, type, constructor

**Call graph:**
- Trait-aware caller resolution: callers of a trait method include callers of all override implementations
- Forward and reverse call graph edges built from SemanticDB occurrences
- Automatic noise filtering in call graphs: stdlib, test files, generated files, plumbing methods, val/var accessors, default parameter accessors, tuple field accessors

**Noise filtering:**
- Built-in filters (always active): stdlib prefixes (scala/\*, java/\*), plumbing methods (apply, map, flatMap, etc.), synthetic names ($anon, derived$, given\_, $default$), boilerplate parents (Object, Product, Serializable)
- `--noise-filter` flag auto-computes dynamic exclude patterns via 5-category noise detection with prefix collapsing
- `--exclude "p1,p2"` for manual comma-separated exclusion patterns matching FQN, name, or owner name

**Platform:**
- Native binary for Darwin-arm64 and Linux-x86\_64 via bootstrap script (`scripts/kodex-cli`)
- Index persistence via rkyv archive format (`.scalex/kodex.idx`)
- `KODEX_IDX` env var and `--idx` flag for custom index path
