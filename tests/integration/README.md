# Integration Tests

End-to-end tests that compile real Scala projects with real build tools, index them with kodex, and verify the results.

## Running

```bash
# Run all integration tests
./tests/integration/run-all.sh

# Run a specific test suite
./tests/integration/run-all.sh mill-cross-platform

# Use a custom kodex binary
KODEX=./target/debug/kodex ./tests/integration/run-all.sh
```

## Prerequisites

- kodex release binary (`cargo build --release`)
- JDK 17+ (for compiling Scala projects)
- Internet connection (first run downloads build tool wrappers + dependencies)

## Test Suites

| Suite | Build Tool | What It Tests |
|-------|-----------|---------------|
| `mill-cross-platform` | Mill | JVM + Scala.js cross-compilation with shared sources |

## Adding a New Test Suite

1. Create a fixture project under `fixtures/<name>/`
2. Create a test script `<name>.sh` in this directory
3. Register the suite in `run-all.sh` by adding `run_suite "<name>"`

### Test Script Structure

Each test script should:
1. Compile the fixture project (generating SemanticDB)
2. Run `kodex index` on the fixture
3. Run assertions using the helper functions

Available helpers:
- `assert_eq <name> <expected> <actual>` — strict equality
- `assert_eq_known_issue <name> <expected> <actual> <issue>` — tracks known bugs (yellow, non-blocking)
- `assert_contains <name> <needle> <haystack>` — substring match
- `assert_not_contains <name> <needle> <haystack>` — negative substring match

### Fixture Projects

Fixtures are real Scala projects with minimal dependencies. Keep them small — they need to compile in CI. Each fixture has its own `.gitignore` to exclude build artifacts.

## Design Decisions

- **Shell scripts over Rust tests**: Integration tests need real build tools (Mill, sbt, scala-cli) and JDK. Shell scripts are simpler to maintain and debug than Rust tests that shell out.
- **Known issues**: `assert_eq_known_issue` lets us document known bugs without blocking CI. When a bug is fixed, the assertion turns green automatically.
- **Fixture isolation**: Each fixture is a standalone project. No shared state between suites.
