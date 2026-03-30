---
name: kodex
description: "Scala code intelligence that fuses compiler knowledge (resolved symbols, types, call graphs via SemanticDB) with build-tool knowledge (module structure, dependencies) into a single fast index. Use kodex for structural questions about Scala codebases: who calls X, what does X call, what implements Y, how is the codebase organized, where is this type used. Triggers: 'who calls X', 'what does X call', 'trace the call graph', 'what modules exist', 'where is this type used', 'how is the codebase structured', or when exploring unfamiliar compiled Scala code that has a .scalex/kodex.idx file. Prefer kodex over grep for call graphs, type hierarchies, and cross-module flow tracing. Use proactively when a .scalex/kodex.idx exists."
---

You have access to `kodex`, a Scala code intelligence CLI. It fuses two knowledge sources into a single fast index:

- **Compiler knowledge** (SemanticDB) — resolved symbols, call targets, types, overrides
- **Build tool knowledge** (Mill, sbt, scala-cli) — module structure, dependencies

This lets kodex answer structural questions that text search cannot: who calls a method through trait indirection, what a method calls across module boundaries, what implements a trait.

kodex `info` includes **full source code** in its output, so you rarely need a separate file read.

## Parallel-safe: all commands exit 0

Every kodex command always exits with code 0. Errors (unknown flags, missing index, bad arguments) are printed to stdout as values, not as non-zero exit codes. This means you can safely run multiple kodex commands in parallel — one command's error will never cancel sibling parallel calls. **Maximize throughput by running independent queries in parallel.**

## Critical: FQN quoting

FQNs contain `#`, `()`, and `.` — characters the shell interprets. **Always single-quote FQNs:**

```bash
kodex info 'com/example/OrderService#createOrder().'    # correct
kodex info com/example/OrderService#createOrder().       # BROKEN — shell eats #
```

## Setup

A bootstrap script at `scripts/kodex-cli` handles downloading and caching the native binary. **Always use the absolute path:**

```bash
bash "/absolute/path/to/skills/kodex/scripts/kodex-cli" <command> [args]
```

Replace `/absolute/path/to/skills/kodex` with the actual path to the directory containing this SKILL.md.

### Before your first query

Two steps: generate SemanticDB, then index.

**Mill projects:**
```bash
./mill __.semanticDbData
kodex index --root .
```

**sbt projects:**
```bash
echo 'addSbtPlugin("org.scalameta" % "sbt-metals" % "1.6.6")' > project/semanticdb.sbt
sbt 'set ThisBuild / bspEnabled := true' compile
kodex index --root .
rm project/semanticdb.sbt
```

**scala-cli projects:**
```bash
scala-cli compile src/ --scalac-option=-Xsemanticdb
kodex index --root .
```

Re-run both steps after code changes. If `.scalex/kodex.idx` exists and code hasn't changed, skip to querying.

### Cross-project queries

By default kodex looks for `.scalex/kodex.idx` in the current directory. To query a different project:

```bash
kodex search Auth --idx /path/to/other-project/.scalex/kodex.idx
```

Or set the `KODEX_IDX` environment variable. The `--idx` flag is global — it works with all commands.

## Core workflow

```
overview → search → info → calls/refs
(orient)   (find)   (understand) (trace deeper)
```

1. **`overview`** — see modules and codebase size. Always run first.
2. **`search`** — find symbols by name. Copy FQNs from the output.
3. **`info`** — paste an FQN, get the complete picture. Noise is excluded by default.
4. **`calls`** / **`refs`** — go deeper when info's capped preview isn't enough.

**Key distinction:** `info` shows call graph (callers/callees) — best for **methods**. For **types** (class/trait), use `refs` to see where the type is used across the codebase.

## Commands

### `overview` — orient in the codebase

```bash
kodex overview
```

No arguments. Shows total stats and all modules sorted by symbol count:
```
258 modules, 1437905 symbols, 17283 files

Modules:
  modules.orders.orders.jvm             44598 symbols   656 files
  modules.catalog.catalog.jvm           34744 symbols   532 files
  ...
```

Module names shown here work directly with `search --module`.

### `search` — find symbols by name

```bash
kodex search <QUERY> [--kind K] [--module M] [--limit N] [--exclude "p1,p2"] [--include-noise]
```

Finds symbols using a resolution cascade — each step tries progressively fuzzier matching, returning on first hit:

1. **Exact FQN** — trigram-narrowed, O(k) lookup
2. **FQN suffix** — matches end of full FQN
3. **Owner.member** — dotted notation up to 5 levels deep: `Component.Backend.render`
4. **Exact display name** — O(1) hash lookup, case-insensitive
5. **Substring** — trigram-accelerated substring on display name
6. **Substring fallback** — linear scan for short queries where trigrams are unavailable
7. **CamelCase** — two strategies: segment matching (`hcf` → `HttpClientFactory`) and character subsequence (`lpfuse` → `linkProfileForUser`)
8. **Fuzzy** — Damerau-Levenshtein with adaptive threshold, catches typos

```bash
kodex search OrderService                         # exact name
kodex search handleReq                            # substring
kodex search hcf                                  # CamelCase: HttpClientFactory
kodex search processPyment                        # typo correction
kodex search OrderService --kind trait            # filter by kind
kodex search OrderService --module storage        # filter by module
kodex search Component.Backend.render             # nested owner.member
```

**Module-only mode** — list all symbols in a module without a search query:

```bash
kodex search --module auth                        # all symbols in auth module
kodex search --module auth --kind trait           # all traits in auth module
kodex search --module billing.jvm --kind class    # all classes in billing JVM module
```

**Flags:**
- `--kind`: class, trait, object, method, field, type, constructor
- `--module`: substring match, or dotted segments in order (e.g. `storage.jvm` matches `modules.storage.storage.jvm`)
- `--limit`: default 50 (0=unlimited)
- `--include-noise`: include noise (generated code, plumbing methods, etc.) — excluded by default
- `--exclude "p1,p2"`: manual comma-separated exclusion patterns

**Output — single match** (auto-expanded detail view — includes signature and parents):
```
trait OrderService — modules.orders.orders.jvm — src/com/example/OrderService.scala:10-50
  fqn: com/example/OrderService#
  signature: sealed trait OrderService extends BaseService { ... }
  parents: com/example/BaseService#
```

**Output — multiple matches** (narrow with `--kind` or `--module`):
```
5 symbols matching 'Service'
  trait OrderService [sealed] (src/com/example/OrderService.scala:10-50)
    fqn: com/example/OrderService#
  class ServiceImpl [final, case] (src/com/example/impl/ServiceImpl.scala:5)
    fqn: com/example/impl/ServiceImpl#
  ...
```

**Kind-aware suggestions** — when `--kind` yields no results but the query matches other kinds:
```
Not found: No trait found matching 'createOrder'
Found under other kinds:
  method createOrder (src/com/example/OrderService.scala:45-78)
    fqn: com/example/OrderService#createOrder().
```

Every result includes an FQN — copy it directly into `info`, `calls`, or `refs`.

**Ranking:** Results are ranked by a composite score: type-level definitions (class/trait) surface first, source files outrank test/generated, popular symbols (by reference count) rank higher, and shorter names are preferred. For scored steps (CamelCase, fuzzy), match quality is primary, composite is tiebreaker.

### `info` — complete picture in one call

```bash
kodex info '<FQN>' [--include-noise] [--exclude "p1,p2"]
```

The most powerful command. Returns everything about a symbol in structured sections:

```
method createOrder [modules.orders.orders.jvm] — src/com/example/OrderService.scala:45-78
  fqn: com/example/OrderService#createOrder().
  referenced: 42 sites across 3 modules
  access: public
  properties: final

  Signature: def createOrder(req: CreateRequest): Future[Order]

  Owner: trait OrderService
    fqn: com/example/OrderService#

  Overrides (1):                                # what this symbol overrides
    createOrder
      fqn: com/example/BaseService#createOrder().

  Overridden by (2):                            # who overrides this symbol
    method createOrder — src/com/example/impl/OrderServiceImpl.scala
      fqn: com/example/impl/OrderServiceImpl#createOrder().

  Extends: BaseService                          # parents (Object/Product/Serializable filtered out)
    fqn: com/example/BaseService#

  Members (5):                                  # only for types — sorted: types → methods → vals/DI fields
    def validateOrder(req: CreateRequest): Future[Valid]
      fqn: com/example/OrderService#validateOrder().
    val method orderRepository: OrderRepository
      fqn: com/example/OrderService#orderRepository.

  Implementations (3):                          # only for traits/abstract classes
    class OrderServiceImpl — src/com/example/impl/OrderServiceImpl.scala
      fqn: com/example/impl/OrderServiceImpl#

  Call graph (depth 1):                         # only for methods/fields — capped at 50

    Callers — who calls this (5):
      Handler.handle [modules.api.api.jvm] — src/com/example/Handler.scala
        fqn: com/example/Handler#handle().

    Callees — what this calls (3):
      1. OrderService.validateOrder
         fqn: com/example/OrderService#validateOrder().
      2. DB.save [modules.storage.storage.jvm] — cross-module
         fqn: com/example/storage/DB#save().

  Source:
     45 | def createOrder(req: CreateRequest): Future[Order] = {
     46 |   validateOrder(req).flatMap { valid =>
     47 |     DB.save(req.toPersisted)
     ...
     78 | }
```

**What to notice in info output:**
- Every sub-symbol has an FQN — copy-paste to chain `info` calls without re-searching
- Call graph entries marked `cross-module` indicate module boundaries — key for architecture understanding
- When callers/callees exceed 50, info prints the exact `calls` command to run. **Follow that hint.**
- **Full source code** is included (complete method/class body) — you usually don't need a separate file read
- Members are sorted: types first, then methods, then vals (DI injections sink to bottom)

### `calls` — call tree traversal

```bash
kodex calls '<FQN>' --depth 3              # downstream (callees)
kodex calls '<FQN>' -r --depth 3           # upstream (callers)
```

Recursive call tree with box-drawing connectors:
```
createOrder [modules.orders.orders.jvm]
├── validateOrder
│   └── Validator.check
├── DB.save [modules.storage.storage.jvm] — cross-module
│   └── Connection.execute
│       └── Pool.acquire (cycle detected)
└── EventBus.publish [modules.events.events.jvm] — cross-module
```

**Reading the output:**
- Indentation = call depth
- `— cross-module — module.name` = call crosses a module boundary
- Cycle detection prevents infinite traversal at already-visited nodes
- Empty tree: if no callers/callees found, shows the resolved file path and suggests alternative FQNs that DO have call edges — useful when you picked the wrong overload

**Trait-aware callers:** When walking upstream (`-r`), kodex automatically includes callers of the base trait/abstract method, not just the concrete implementation. So `kodex calls -r 'impl/OrderServiceImpl#create().'` will also find callers that call `trait/OrderService#create().` — essential for understanding polymorphic call sites in Scala codebases.

**Flags:**
- `--depth N`: default 3
- `-r, --reverse`: walk callers instead of callees
- `--include-noise`: include noise — excluded by default
- `--exclude "p1,p2"`: manual exclusion patterns

Use `calls` when `info`'s depth-1 preview (capped at 50) isn't enough.

### `refs` — where is a symbol used?

```bash
kodex refs '<FQN>' [--limit N]
```

Shows all reference locations grouped by module then file:

```
OrderService — 30 references across 16 files, 4 modules

By module:
  webapp.webapp.jvm                        4 refs in 2 files
  modules.orders.orders.jvm               18 refs in 10 files

Locations:
  [webapp.webapp.jvm]
    webapp/src/com/example/Handler.scala:12,38
  [modules.orders.orders.jvm]
    orders/src/com/example/OrderManager.scala:23,56,89
  ...
```

**Details:**
- `--limit` caps the number of file locations shown (default 100, 0=unlimited). Header and module summary always show full totals regardless of limit.
- Only shows reference sites — definitions are excluded (you already know where it's defined from `info`).
- Line numbers are deduped and comma-separated per file.

**When to use refs:** `info` shows callers/callees for methods. For **types** (class/trait), `refs` is the way to see usage across the codebase — `info` won't show type references in its call graph.

### `noise` — find noise patterns

```bash
kodex noise [--limit N]
```

Analyzes the index and categorizes noisy symbols in 5 categories:

1. **Effect plumbing** — high fan-in, no callees (loggers, validators)
2. **Hub utilities** — high ref count, wide module spread (Config, common traits)
3. **ID factories** — pure generation methods (generateId, randomUUID)
4. **Store ops** — CRUD methods on Repository/Store/DAO types
5. **Infrastructure plumbing** — high fan-in on cross-cutting owner types

Outputs a ready-to-use `--exclude` pattern. Run once on a new codebase to see what kodex considers noisy — though noise is already excluded by default.

## Noise filtering

Noise is **excluded by default** — no flag needed. This covers:

- **stdlib**: scala/Predef, scala/Option, scala/collection/\*, java/lang/\*, java/util/\*, etc.
- **Plumbing methods**: apply, unapply, map, flatMap, filter, foreach, collect, foldLeft, foldRight, get, getOrElse, orElse, succeed, pure, attempt, traverse, etc.
- **Test files and generated files** (ScalaPB, protobuf, src_managed, BuildInfo)
- **Call graph extras**: val/var accessors (dependency wiring, not real calls), $default$ parameter accessors, tuple accessors (_1, _2), synthetic names
- **Boilerplate parents**: Object, Product, Serializable are filtered from the Extends section

To **include** noise in results, pass `--include-noise`:

```bash
kodex info '<FQN>' --include-noise
kodex calls '<FQN>' --depth 3 --include-noise
kodex search Query --include-noise
```

`--exclude "Pattern1,Pattern2"` gives additional manual control — patterns match against FQN, symbol name, and owner name (substring match).

## FQN format

`info`, `calls`, and `refs` require exact FQNs. Copy them from `search` or `info` output.

| Symbol type | Pattern | Example |
|---|---|---|
| Class / Trait | `path/Name#` | `com/example/OrderService#` |
| Object | `path/Name.` | `com/example/OrderService.` |
| Method (def) | `path/Owner#name().` | `com/example/OrderService#createOrder().` |
| Method (val) | `path/Owner.name.` | `com/example/Endpoints.createOrder.` |

## Options reference

| Flag | Commands | Default | Description |
|---|---|---|---|
| `--kind K` | search | all | class, trait, object, method, field, type, constructor |
| `--module M` | search | all | Substring or dotted segment match on module name |
| `--limit N` | search, refs, noise | 50 / 100 / 15 | Max results (0=unlimited) |
| `--depth N` | calls | 3 | Call tree recursion depth |
| `-r, --reverse` | calls | off | Walk callers instead of callees |
| `--include-noise` | search, info, calls | off | Include noise — excluded by default |
| `--exclude P` | search, info, calls | — | Manual comma-separated exclusion patterns |
| `--root PATH` | index | `.` | Workspace root |
| `--idx PATH` | all | `.scalex/kodex.idx` | Override index path (or `KODEX_IDX` env var) |

## Common patterns

**Understand a new codebase:**
```bash
kodex overview
kodex search MainService
kodex info 'com/example/MainService#'
```

**Answer "how does feature X work?":**
```bash
kodex search createOrder
kodex info 'com/example/Service#createOrder().'
kodex calls 'com/example/Service#createOrder().' --depth 3
```

**Assess change risk ("what breaks if I change X?"):**
```bash
kodex calls 'com/example/PaymentService#process().' -r --depth 2
kodex refs 'com/example/PaymentService#'
```

**Find all implementations of a trait:**
```bash
kodex search Repository --kind trait
kodex info 'com/example/Repository#'                    # Implementations section lists them
```

**Explore a specific module:**
```bash
kodex overview                                          # see module names
kodex search --module auth --kind trait                 # all traits in auth module
kodex search Service --kind trait --module auth         # search within a module
```

**Parallel queries for maximum throughput:**
```bash
# Run these in parallel — all exit 0, safe to parallelize
kodex search LoginService &
kodex search AuthenticationService &
kodex search SessionManager &
wait
```

```bash
# Get info on multiple symbols in parallel
kodex info 'com/example/LoginService#' &
kodex info 'com/example/AuthService#' &
kodex info 'com/example/SessionManager#' &
wait
```

```bash
# Get both call graph and references in parallel
kodex calls 'com/example/Service#create().' --depth 3 &
kodex refs 'com/example/Service#create().' &
wait
```

## Troubleshooting

- **No .semanticdb files**: Run the SemanticDB generation step for your build tool first.
- **Stale results**: Re-run SemanticDB generation, then `kodex index --root .`
- **Index not found**: Run `kodex index --root .`
- **Too much noise**: Noise is excluded by default. For additional control, run `kodex noise` to find patterns for `--exclude`.
- **Symbol not found**: Try a shorter substring, CamelCase abbreviation, or `Owner.member` syntax.
- **info/calls/refs "Not found"**: These need exact FQNs. Run `search` first, then copy the FQN.
- **Shell errors with FQNs**: Single-quote FQNs: `kodex info 'com/example/Foo#bar().'`
- **Wrong overload picked**: `calls` shows an empty tree? Check the diagnostic — it suggests alternative FQNs with call edges.
- **Missing callers for override**: Callers are trait-aware automatically. If callers look incomplete, the base trait method's FQN may differ — check `info`'s Overrides section.
