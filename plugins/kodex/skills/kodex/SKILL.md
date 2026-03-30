---
name: kodex
description: "Scala code intelligence that fuses compiler knowledge (resolved symbols, types, call graphs via SemanticDB) with build-tool knowledge (module structure, dependencies) into a single fast index. Use kodex for structural questions about Scala codebases: who calls X, what does X call, what implements Y, how is the codebase organized, where is this type used. Triggers: 'who calls X', 'what does X call', 'trace the call graph', 'what modules exist', 'where is this type used', 'how is the codebase structured', or when exploring unfamiliar compiled Scala code that has a .scalex/kodex.idx file. Prefer kodex over grep for call graphs, type hierarchies, and cross-module flow tracing. Use proactively when a .scalex/kodex.idx exists."
---

You have access to `kodex`, a Scala code intelligence CLI. It fuses two knowledge sources into a single fast index:

- **Compiler knowledge** (SemanticDB) — resolved symbols, call targets, types, overrides
- **Build tool knowledge** (Mill, sbt, scala-cli) — module structure, dependencies

This lets kodex answer structural questions that text search cannot: who calls a method through trait indirection, what a method calls across module boundaries, what implements a trait.

kodex `info` includes **source code** in its output, so you rarely need a separate file read.

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

## Core workflow

```
overview → search → info → calls/refs
(orient)   (find)   (understand) (trace deeper)
```

1. **`overview`** — see modules and codebase size. Always run first.
2. **`search`** — find symbols by name. Copy FQNs from the output.
3. **`info`** — paste an FQN, get the complete picture. Always use `--noise-filter`.
4. **`calls`** / **`refs`** — go deeper when info's capped preview isn't enough. Always use `--noise-filter` with `calls`.

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
kodex search <QUERY> [--kind K] [--module M] [--limit N] [--exclude "p1,p2"] [--noise-filter]
```

Finds symbols using a 9-step cascade — exact names, substrings, CamelCase abbreviations, typos:

```bash
kodex search OrderService                         # exact name
kodex search handleReq                            # substring
kodex search hcf                                  # CamelCase: HttpClientFactory
kodex search processPyment                        # typo correction
kodex search OrderService --kind trait            # filter by kind
kodex search OrderService --module storage        # filter by module
kodex search Component.Backend.render             # nested owner.member
```

**Flags:**
- `--kind`: class, trait, object, method, field, type, constructor
- `--module`: substring match, or dotted segments in order (e.g. `storage.jvm` matches `modules.storage.storage.jvm`)
- `--limit`: default 50 (0=unlimited)
- `--noise-filter`: excludes noisy utility symbols from results
- `--exclude "p1,p2"`: manual comma-separated exclusion patterns

**Output — single match** (enough to proceed, no further search needed):
```
trait OrderService — modules.orders.orders.jvm — src/com/example/OrderService.scala:10-50
  fqn: com/example/OrderService#
  referenced: 123 sites across 4 modules
```

**Output — multiple matches** (narrow with `--kind` or `--module`):
```
5 symbols matching 'Service'
  trait OrderService — modules.orders.orders.jvm — src/com/example/OrderService.scala
  class ServiceImpl — modules.server.server.jvm — src/com/example/impl/ServiceImpl.scala
  ... and 3 more (use --limit 0 for all)
```

Every result includes an FQN — copy it directly into `info`, `calls`, or `refs`.

### `info` — complete picture in one call

```bash
kodex info '<FQN>' --noise-filter
```

The most powerful command. Returns everything about a symbol in structured sections:

```
method createOrder — modules.orders.orders.jvm — src/com/example/OrderService.scala:45-78
  fqn: com/example/OrderService#createOrder().
  referenced: 42 sites across 3 modules
  access: public
  properties: final

  Signature: def createOrder(req: CreateRequest): Future[Order]

  Owner: trait OrderService
    fqn: com/example/OrderService#

  Overrides (1):
    createOrder — fqn: com/example/BaseService#createOrder().

  Extends: BaseService
    fqn: com/example/BaseService#

  Members (5):                              # (only for types — class/trait/object)
    def validateOrder ...
      fqn: com/example/OrderService#validateOrder().
    ...

  Implementations (3):                      # (only for traits/abstract classes)
    class OrderServiceImpl ...
      fqn: com/example/impl/OrderServiceImpl#

  Call graph (depth 1):
    Callers — who calls this (5):
      Handler.handle — modules.api.api.jvm — src/com/example/Handler.scala
        fqn: com/example/Handler#handle().

    Callees — what this calls (3):
      1. validateOrder
         fqn: com/example/OrderService#validateOrder().
      2. DB.save — cross-module — modules.storage.storage.jvm
         fqn: com/example/storage/DB#save().

    15+ callers/callees? Run: kodex calls 'com/example/OrderService#createOrder().' --depth 2

  Source:
     45 | def createOrder(req: CreateRequest): Future[Order] = {
     46 |   validateOrder(req).flatMap { valid =>
     47 |     DB.save(req.toPersisted)
     ...
```

**What to notice in info output:**
- Every sub-symbol has an FQN — copy-paste to chain `info` calls without re-searching
- Call graph entries marked `cross-module` indicate module boundaries — key for architecture understanding
- When callers/callees are capped at 15, info prints the exact `calls` command to run. **Follow that hint.**
- Source code is included — you usually don't need a separate file read

**Flags:**
- `--noise-filter`: auto-excludes noisy utilities (recommended always)
- `--exclude "p1,p2"`: manual exclusion (overrides --noise-filter if both set)

### `calls` — call tree traversal

```bash
kodex calls '<FQN>' --depth 3 --noise-filter           # downstream (callees)
kodex calls '<FQN>' -r --depth 3 --noise-filter         # upstream (callers)
```

Recursive call tree with box-drawing connectors:
```
createOrder
├── validateOrder
│   └── Validator.check
├── DB.save — cross-module — modules.storage.storage.jvm
│   └── Connection.execute
│       └── Pool.acquire (cycle detected)
└── EventBus.publish — cross-module — modules.events.events.jvm
```

**Reading the output:**
- Indentation = call depth
- `— cross-module — module.name` = call crosses a module boundary
- `(cycle detected)` = recursion stopped, already visited

**Flags:**
- `--depth N`: default 3
- `-r, --reverse`: walk callers instead of callees
- `--noise-filter`: auto-exclude noise (recommended always)
- `--exclude "p1,p2"`: manual exclusion

Use `calls` when `info`'s depth-1 preview (capped at 15) isn't enough.

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

`--limit`: default 100 file locations (0=unlimited). Header and module summary always show full totals.

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

Outputs a ready-to-use `--exclude` pattern. Run once on a new codebase, or just use `--noise-filter` which computes this automatically.

## Noise filtering

`--noise-filter` auto-computes and applies noise exclusion. Use it by default on `info`, `calls`, and `search`:

```bash
kodex info '<FQN>' --noise-filter
kodex calls '<FQN>' --depth 3 --noise-filter
kodex search Query --noise-filter
```

`--exclude "Pattern1,Pattern2"` gives manual control. If both are passed, `--exclude` takes precedence.

kodex also **automatically** filters (no flag needed): stdlib (scala/\*, java/\*), test files, generated files, plumbing methods (apply, map, flatMap, etc.), synthetic symbols, boilerplate parents (Object, Product, Serializable).

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
| `--noise-filter` | search, info, calls | off | Auto-exclude noise (use by default) |
| `--exclude P` | search, info, calls | — | Manual comma-separated patterns (overrides --noise-filter) |
| `--root PATH` | index | `.` | Workspace root |
| `--idx PATH` | all | `.scalex/kodex.idx` | Override index path (or `KODEX_IDX` env var) |

## Common patterns

**Understand a new codebase:**
```bash
kodex overview
kodex search MainService
kodex info 'com/example/MainService#' --noise-filter
```

**Answer "how does feature X work?":**
```bash
kodex search createOrder
kodex info 'com/example/Service#createOrder().' --noise-filter
kodex calls 'com/example/Service#createOrder().' --depth 3 --noise-filter
```

**Assess change risk ("what breaks if I change X?"):**
```bash
kodex calls 'com/example/PaymentService#process().' -r --depth 2 --noise-filter
kodex refs 'com/example/PaymentService#'
```

**Find all implementations of a trait:**
```bash
kodex search Repository --kind trait
kodex info 'com/example/Repository#' --noise-filter    # Implementations section lists them
```

**Explore a specific module:**
```bash
kodex overview                                          # see module names
kodex search Service --kind trait --module auth
```

## Troubleshooting

- **No .semanticdb files**: Run the SemanticDB generation step for your build tool first.
- **Stale results**: Re-run SemanticDB generation, then `kodex index --root .`
- **Index not found**: Run `kodex index --root .`
- **Too much noise**: Use `--noise-filter`, or run `kodex noise` for manual `--exclude`.
- **Symbol not found**: Try a shorter substring, CamelCase abbreviation, or `Owner.member` syntax.
- **info/calls/refs "Not found"**: These need exact FQNs. Run `search` first, then copy the FQN.
- **Shell errors with FQNs**: Single-quote FQNs: `kodex info 'com/example/Foo#bar().'`
