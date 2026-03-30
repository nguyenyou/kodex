---
name: kodex
description: "Scala code intelligence that fuses build-tool knowledge (module structure, dependencies) with compiler knowledge (resolved symbols, types, call graphs) into a single fast index. Use kodex for structural questions about Scala codebases: who calls X, what does X call, what implements Y, how is the codebase organized, where is this type used. Triggers: 'who calls X', 'what does X call', 'trace the call graph', 'what modules exist', 'where is this type used', 'how is the codebase structured', or when exploring unfamiliar compiled Scala code that has a .scalex/kodex.idx file. Prefer kodex over grep for call graphs, type hierarchies, and cross-module flow tracing. Use proactively when a .scalex/kodex.idx exists."
---

You have access to `kodex`, a Scala code intelligence CLI. It fuses two sources of knowledge into a single fast index:

- **Compiler knowledge** (via SemanticDB) — resolved symbols, actual call targets, types, overrides
- **Build tool knowledge** (via Mill, sbt, or scala-cli metadata) — module structure, dependencies

This lets kodex answer structural questions that text search cannot: who calls a method through trait indirection, what a method calls across module boundaries, what implements a trait including indirect implementations.

kodex includes **source code** in its output — `info` shows the full definition body, so you rarely need a separate file read.

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
./mill __.semanticDbData                    # 1. Generate SemanticDB
kodex index --root .                        # 2. Build index -> .scalex/kodex.idx
```

**sbt projects:**
```bash
echo 'addSbtPlugin("org.scalameta" % "sbt-metals" % "1.6.6")' > project/semanticdb.sbt
sbt 'set ThisBuild / bspEnabled := true' compile
kodex index --root .
rm project/semanticdb.sbt                   # clean up
```

**scala-cli projects:**
```bash
scala-cli compile src/ --scalac-option=-Xsemanticdb   # target source dir, not root
kodex index --root .
```

Re-run both steps after code changes. If `.scalex/kodex.idx` exists and code hasn't changed, skip to querying.

## Commands

kodex has 7 commands: 1 build command and 6 query commands.

### Which command to use

| You want to... | Use |
|---|---|
| See what modules exist, orient in a new codebase | `overview` |
| Find a symbol by name | `search` |
| Understand a symbol completely (signature, members, callers, callees, source) | `info` |
| Trace a call tree deeper than info's depth-1 preview | `calls` |
| See where a type/symbol is referenced across the codebase | `refs` |
| Find noise patterns for `--exclude` | `noise` |

### Session startup — always do this first

At the start of every session, run `overview` to orient:

```bash
kodex overview                    # Learn what modules exist and how big they are
```

The module names shown here work directly with `search --module`.

### Noise filtering

Use `--noise-filter` on `info`, `calls`, and `search` to automatically exclude noisy utility symbols. This is equivalent to running `noise` and pasting its suggested `--exclude` — but in one flag:

```bash
kodex info 'com/example/Service#' --noise-filter
kodex calls 'com/example/Service#process().' --noise-filter
```

**`--noise-filter` vs `--exclude`:**
- `--noise-filter` — auto-computes the noise exclude pattern. Use this by default.
- `--exclude "Pattern1,Pattern2"` — manual control. Use when you want to customize what's excluded.
- If both are passed, `--exclude` takes precedence (noise-filter is ignored).

To see what `--noise-filter` would exclude, run `kodex noise` to inspect the categories and patterns.

### The core workflow

```
overview → search → info → calls/refs
(orient)   (find)  (understand) (trace deeper)
```

1. **`overview`** — see modules and codebase size.
2. **`search`** — find symbols by name. Copy FQNs from the output.
3. **`info`** — paste a FQN, get the complete picture including source code. **Always pass `--noise-filter`.**
4. **`calls`** / **`refs`** — go deeper when `info`'s capped preview isn't enough. **Always pass `--noise-filter` to `calls`.**

---

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

Use the module names from this output with `search --module`.

### `search` — find symbols by name

```bash
kodex search <QUERY> [--kind K] [--module M] [--limit N] [--exclude "p1,p2"]
```

Finds symbols using a 9-step cascade — handles exact names, substrings, CamelCase abbreviations, typos:

```bash
kodex search OrderService                       # exact name
kodex search handleReq                          # substring
kodex search hcf                                # CamelCase: HttpClientFactory
kodex search processPyment                      # typo correction
kodex search OrderService --kind trait           # filter by kind
kodex search OrderService --module storage        # filter by module
kodex search Component.Backend.render            # nested owner.member
```

`--kind`: class, trait, object, method, field, type, constructor
`--module`: substring match, or dotted segments matched in order (e.g. `storage.jvm`)
`--limit`: default 50, use 0 for unlimited

Every result includes an FQN — copy it directly into `info`, `calls`, or `refs`.

### `info` — complete picture in one call

```bash
kodex info <FQN> [--exclude "p1,p2"]
```

The most powerful command. Returns everything about a symbol in a single call:

- **Header**: kind, name, module, file:line range, reference count
- **Metadata**: access, properties, owner (with FQN for navigation up)
- **Signature**: full type signature
- **Overrides / Overridden by**: what this overrides and who overrides this (with FQNs)
- **Extends**: parent types (with FQNs)
- **Members**: for types — all methods, fields, inner types (with FQNs)
- **Implementations**: for traits — concrete subtypes (with FQNs)
- **Call graph (depth 1)**: callers and callees (capped at 15 each, with FQNs). If more exist, info tells you the exact `calls` command to run.
- **Source body**: the full definition read from disk, with line numbers

Every sub-symbol includes its FQN, so you can chain `info` calls without re-searching.

`info` includes source code, so you typically don't need a separate file read after calling it.

### `calls` — call tree traversal

```bash
kodex calls <FQN> [--depth N] [-r|--reverse] [--exclude "p1,p2"]
```

Recursive call tree with box-drawing connectors and module annotations:

```bash
kodex calls 'com/example/OrderService#createOrder().' --depth 3          # downstream
kodex calls 'com/example/PaymentService#process().' -r --depth 3         # upstream (callers)
kodex calls 'com/example/Handler#handle().' --depth 2 --exclude "Logger" # filtered
```

`--depth`: default 3. Cycles are detected.
`--reverse`: walk callers instead of callees.
Cross-module calls are annotated with `— cross-module`.

Use `calls` when `info`'s depth-1 preview (capped at 15) isn't enough.

### `refs` — where is a symbol used?

```bash
kodex refs <FQN> [--limit N]
```

Shows all reference locations grouped by module then file:

```
DocumentService — 30 references across 16 files, 4 modules

By module:
  webapp.webapp.jvm                        4 refs in 2 files
  modules.storage.storage.jvm              18 refs in 10 files

Locations:
  [webapp.webapp.jvm]
    webapp/src/com/example/storage/FileManagerServiceImpl.scala:12,38
  ...
```

`--limit`: default 100 file locations shown (0=unlimited). Header and module summary always show full totals.

`refs` is especially useful for **types** (class/trait) — `info` shows callers only for methods, not types. Use `refs` to see where a type is used across the codebase.

### `noise` — configure --exclude

```bash
kodex noise [--limit N]
```

Analyzes the index and categorizes noisy symbols (5 categories: effect plumbing, hub utilities, ID factories, store ops, infrastructure plumbing). Outputs a ready-to-use `--exclude` string:

```
Suggested --exclude:
  --exclude "ServiceUtils,DatabaseOps,RecordStore"
```

Run once on a new codebase, then pass the exclude pattern to `info` and `calls`.

## FQN format

`info`, `calls`, and `refs` require exact FQNs. Copy them from `search` or `info` output.

| Symbol type | Pattern | Example |
|---|---|---|
| Class / Trait | `path/Name#` | `com/example/OrderService#` |
| Object | `path/Name.` | `com/example/OrderService.` |
| Method (def) | `path/Owner#name().` | `com/example/OrderService#createOrder().` |
| Method (val) | `path/Owner.name.` | `com/example/OrderEndpoints.createOrder.` |

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

## Automatic filtering

kodex filters noise automatically — you don't need to manually exclude these:

- stdlib (scala/*, java/*), test files, generated files
- Plumbing methods (apply, map, flatMap, filter, foreach, get, etc.)
- Synthetic symbols (default params, tuple accessors, $-prefixed names)
- Boilerplate parents (Object, Product, Serializable, AnyRef, Any, Equals)

Use `--exclude` on top for **project-specific** noise. Run `noise` to get suggestions.

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
kodex info 'com/example/PaymentService#process().' --noise-filter
kodex calls 'com/example/PaymentService#process().' -r --depth 2 --noise-filter
kodex refs 'com/example/PaymentService#'
```

**Find all implementations of a trait:**
```bash
kodex search Repository --kind trait
kodex info 'com/example/Repository#' --noise-filter
```

**Explore a specific module:**
```bash
kodex overview                                    # see module names
kodex search Service --kind trait --module auth
```

## Troubleshooting

- **No .semanticdb files**: Run the SemanticDB generation step for your build tool first.
- **Stale results**: Re-run SemanticDB generation, then `kodex index --root .`
- **Index not found**: Run `kodex index --root .`
- **Too much noise**: Run `kodex noise` and use the suggested `--exclude`.
- **Symbol not found**: Try a shorter substring, CamelCase abbreviation, or Owner.member syntax.
- **info/calls/refs "Not found"**: These require exact FQNs. Use `search` first.
