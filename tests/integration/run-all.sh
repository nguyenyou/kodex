#!/bin/bash
# Integration test runner for kodex.
# Compiles real Scala projects with different build tools, indexes them with kodex,
# and runs assertions against the indexed data.
#
# Usage:
#   ./run-all.sh                    # run all integration tests
#   ./run-all.sh mill-cross-platform  # run a specific test
#
# Environment:
#   KODEX    path to kodex binary (default: ../../target/release/kodex)
#
# Prerequisites:
#   - JDK 17+ (for Mill/sbt/scala-cli)
#   - Internet connection (first run downloads Mill wrapper + dependencies)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
KODEX="${KODEX:-$SCRIPT_DIR/../../target/release/kodex}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

# Verify kodex binary
if [ ! -x "$KODEX" ]; then
    echo -e "${RED}kodex binary not found at $KODEX${NC}"
    echo "Run: cargo build --release"
    exit 1
fi

# Verify JDK
if ! command -v java &> /dev/null; then
    echo -e "${RED}java not found — JDK 17+ required${NC}"
    exit 1
fi

TOTAL_PASS=0
TOTAL_FAIL=0
TOTAL_SKIP=0
FILTER="${1:-}"

run_suite() {
    local name="$1"
    local script="$SCRIPT_DIR/$name.sh"

    if [ -n "$FILTER" ] && [ "$FILTER" != "$name" ]; then
        TOTAL_SKIP=$((TOTAL_SKIP + 1))
        return
    fi

    if [ ! -f "$script" ]; then
        echo -e "${RED}Test script not found: $script${NC}"
        TOTAL_FAIL=$((TOTAL_FAIL + 1))
        return
    fi

    echo -e "${CYAN}━━━ $name ━━━${NC}"
    if KODEX="$KODEX" bash "$script"; then
        TOTAL_PASS=$((TOTAL_PASS + 1))
    else
        TOTAL_FAIL=$((TOTAL_FAIL + 1))
    fi
    echo ""
}

echo "kodex integration tests"
echo "  binary: $KODEX"
echo ""

# ── Test suites ──
# Add new suites here as they are created.
run_suite "mill-cross-platform"
# run_suite "sbt-cross-platform"     # future
# run_suite "scala-cli-basic"        # future

# Summary
echo "========================================"
echo -e "Suites: ${GREEN}$TOTAL_PASS passed${NC}, ${RED}$TOTAL_FAIL failed${NC}, $TOTAL_SKIP skipped"
echo "========================================"
if [ $TOTAL_FAIL -gt 0 ]; then exit 1; fi
