
<p align="center">
  <img width="555" height="362" alt="kodex-mascot" src="site/banner-transparent.png" />
</p>

# kodex

**K**nowledge + C**ode** + Inde**x** = **Kodex.** Compiler-precise Scala code intelligence for coding agents.

**Best for:** codebase Q&A, knowledge base generation, onboarding to unfamiliar code, production spec sync.

> Our mascot is a raven, inspired by Odin's ravens Huginn and Muninn — who flew across the world each day and returned to whisper everything they had seen. kodex does the same for codebases.

## Why I Built This

I work across many domains and products at work — codebases I don't have deep knowledge about. The usual workflow: figure out how to set something up, realize I don't know how, go find the engineer who owns that domain and ask them. That's friction.

I built kodex so that Claude Code can answer those questions instead. *How do I create X? How does Y work? What are the requirements for Z to be properly set up?* It traces through the compiled codebase and gives me a precise answer.

This tool is built for **reading code, not writing code**.

## Quick Start

**1. 🪶 Summon it** — install the plugin (one-time):

```
/plugin marketplace add nguyenyou/kodex
/plugin install kodex@kodex-marketplace
```

**2. 🚀 Let it fly** — ask away:

```
use kodex to explore how authentication works in this codebase
```

> The coding agent knows how to compile SemanticDB for Mill, sbt, and scala-cli projects — just ask and it will handle the build for you.

## Recommended Workflows

### Knowledge base generator (automated)

A server runs daily/weekly to clone the production codebase, compile SemanticDB, and let an AI agent use kodex to generate product specs. Specs stay in sync with the actual code — generated from code, not the other way around.

### Read-only explorer (what I use)

Keep two clones of the production codebase:
- **Read-only clone** — compiled with SemanticDB, used with kodex for understanding
- **Working clone** — where you write code

Your kodex index stays stable and doesn't get invalidated by in-progress changes.

## scalex vs kodex

```
scalex:  .scala ──▶ Parser ──▶ AST ──▶ Index & Query
kodex:   .scala ──▶ Compiler ──▶ SemanticDB ──▶ Index & Query
                    (expensive)   (precise)
```

|  | **scalex** | **kodex** |
|---|---|---|
| **Setup cost** | None — instant on any Scala codebase | Requires a full compile |
| **Accuracy** | Best-effort (unresolved implicits, overloads) | Compiler-precise — every symbol resolved |
| **Speed to first query** | Seconds | Minutes (compile time) |
| **Best for** | Quick exploration, reading unfamiliar code | Deep understanding, production codebases |

I use both. **scalex** is my default for open source projects and libraries — no setup, instant answers. Opus 4.6 is good enough at reasoning that AST + source code alone is more than enough to explain how a project works. **kodex** is for production codebases where I already compile the code for work, so the cost is paid and I want compiler-precise accuracy.

## Credits

- **[Scala](https://github.com/scala/scala3) & [Scalameta](https://github.com/scalameta/scalameta)** — for building [SemanticDB](https://scalameta.org/docs/semanticdb/guide.html), the compiler output format that makes this tool possible.
- **[Metals](https://github.com/scalameta/metals)** — for showing how to compile SemanticDB across sbt and scala-cli projects.
- Built with **[Claude Code](https://claude.com/claude-code)** (Opus 4.6).

## License

[MIT](LICENSE)
