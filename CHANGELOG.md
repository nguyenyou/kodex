# Changelog

## [Unreleased]

## [1.6.0] — 2026-03-31

### Fixed
- **Method-level noise patterns** — `NoiseCandidate::method()` now emits `Owner#method` (using the FQN separator) instead of `Owner` as the exclude pattern. Previously, detecting `ServiceActor#userId` as effect plumbing would emit `ServiceActor`, filtering the entire type and all its members from search/info results. Now emits `ServiceActor#userId` which targets only the specific noisy method.
- **Hub utilities thresholds too aggressive** — raised `HUB_MIN_REFS` from 100 to 300 and made `HUB_MIN_MODULES` adaptive: `max(total_modules / 2, 10)` instead of fixed 3. Core domain types like `ServiceActor` (203 refs, 25 modules) are no longer classified as noise in large codebases.
- **FQN separator mismatch in noise patterns** — method noise patterns now use `#` for class/trait members and `.` for object members, matching the actual SemanticDB FQN format. Previously used `.` unconditionally, which silently failed to filter class member methods via `matches_exclude`.

### Added
- **FQN-aware not-found suggestions** — when `info`, `calls`, `refs`, or `trace` receives a not-found FQN, `suggest_similar` now extracts the method/type name and searches globally. Shows matching symbols with copyable `fqn:` lines. Example: passing `LoginService#loginWithPassword().` now suggests `LoginWithMFAService#loginWithPassword().` instead of showing no suggestions.

## [1.5.0] — 2026-03-30

### Added
- **`--kind case-class`** — filters search results to only case classes (uses `PROP_CASE` property bit)
- **`--kind enum`** — filters search results to only Scala 3 enums (uses `PROP_ENUM` property bit); also matches `Interface`-kinded enums from SemanticDB
- **`display_kind()`** — property-aware display function: shows "case class" and "enum" in all output (search, info, calls, trace, noise) instead of generic "class"
- **`.scalex/noise.conf`** — noise exclude patterns are now written to a config file during `kodex index`. Agents can edit the file to remove false positives. `kodex noise --init` regenerates it. Commands read from the file instead of re-computing noise on every invocation; falls back to auto-compute if the file is missing.

### Changed
- **`--kind class` is now strict** — excludes case classes and enums. Use `--kind case-class` or `--kind enum` for those. Each kind filter is now disjoint (breaking change).

## [1.4.0] — 2026-03-30

### Added
- **`calls --cross-module-only`** — filters the call tree to show only edges that cross module boundaries, hiding intra-module calls. Useful for understanding a method's external dependencies at a glance
- **`trace` command** — call tree with `info`-level detail (kind, FQN, signature, source code) at each node. Like running `info` recursively down the call chain. Supports `--depth`, `--reverse`, and `--cross-module-only`
- Integration test infrastructure (`tests/integration/`) — end-to-end tests that compile real Scala projects with real build tools, index with kodex, and verify results. First suite: Mill cross-platform (JVM + Scala.js) with 20 assertions

### Fixed
- **Shared-source URI rewriting** — in Mill cross-platform builds, shared sources copied to `out/.../jsSharedSources.dest/` now resolve to their canonical `shared/src/` path. Previously, kodex showed `out/` paths for shared symbols and double-counted shared files across modules
- Mill adapter now detects shared-source copies by comparing `generatedSources` paths (in `out/`) with `sources` paths across sibling modules, then rewrites SemanticDB URIs during loading
- On a real 1.4M-symbol codebase: 61 rewrite rules eliminated ~2,000 duplicate file entries (~11% reduction)

## [1.3.0] — 2026-03-30

### Changed
- All commands now always exit with code 0 — errors (unknown flags, missing index, bad arguments) are printed to stdout as values instead of causing non-zero exits. This prevents cascading failures when LLM agents run multiple kodex commands in parallel (fixes #10).

## [1.2.0] — 2026-03-30

### Added
- `search --module <M>` without a query argument — lists all symbols in a module, filtered by `--kind` and `--limit`
- Kind-aware suggestions — when `--kind` filter yields no results but the query matches symbols of other kinds, shows "Found under other kinds" with matching symbols

### Changed
- `--kind` filter in `search` is now strict — returns no results (with suggestions) instead of silently falling back to unfiltered results

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
