#!/bin/bash
# Integration test: Mill cross-platform (JVM + Scala.js) shared source deduplication.
#
# This test verifies kodex behavior when indexing a Mill project with cross-compiled
# modules that share source files. The shared sources are compiled by both JVM and JS
# targets, producing separate SemanticDB files that reference the same source code.
#
# Issues tested:
#   1. Shared symbols should not appear twice in search results (FQN dedup)
#   2. Shared symbols should be accessible via --module filter for BOTH modules
#   3. Platform-specific symbols should appear only in their respective module
#   4. File paths should be canonical (not out/ build artifacts)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
FIXTURE_DIR="$SCRIPT_DIR/fixtures/mill-cross-platform"
KODEX="${KODEX:-$SCRIPT_DIR/../../target/release/kodex}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

PASS=0
FAIL=0
KNOWN_FAIL=0

assert_eq() {
    local name="$1"
    local expected="$2"
    local actual="$3"
    if [ "$expected" = "$actual" ]; then
        echo -e "  ${GREEN}PASS${NC}  $name"
        PASS=$((PASS + 1))
    else
        echo -e "  ${RED}FAIL${NC}  $name"
        echo "        expected: $expected"
        echo "        actual:   $actual"
        FAIL=$((FAIL + 1))
    fi
}

# Like assert_eq but for known issues — FAIL doesn't cause exit 1
assert_eq_known_issue() {
    local name="$1"
    local expected="$2"
    local actual="$3"
    local issue="$4"
    if [ "$expected" = "$actual" ]; then
        echo -e "  ${GREEN}PASS${NC}  $name"
        PASS=$((PASS + 1))
    else
        echo -e "  ${YELLOW}KNOWN${NC} $name"
        echo "        expected: $expected"
        echo "        actual:   $actual"
        echo "        issue:    $issue"
        KNOWN_FAIL=$((KNOWN_FAIL + 1))
    fi
}

assert_contains() {
    local name="$1"
    local needle="$2"
    local haystack="$3"
    if echo "$haystack" | grep -q "$needle"; then
        echo -e "  ${GREEN}PASS${NC}  $name"
        PASS=$((PASS + 1))
    else
        echo -e "  ${RED}FAIL${NC}  $name"
        echo "        expected to contain: $needle"
        echo "        actual output (first 5 lines):"
        echo "$haystack" | head -5 | sed 's/^/          /'
        FAIL=$((FAIL + 1))
    fi
}

assert_not_contains() {
    local name="$1"
    local needle="$2"
    local haystack="$3"
    if echo "$haystack" | grep -q "$needle"; then
        echo -e "  ${RED}FAIL${NC}  $name"
        echo "        should NOT contain: $needle"
        echo "        matching lines:"
        echo "$haystack" | grep "$needle" | head -3 | sed 's/^/          /'
        FAIL=$((FAIL + 1))
    else
        echo -e "  ${GREEN}PASS${NC}  $name"
        PASS=$((PASS + 1))
    fi
}

# ── Step 1: Compile the fixture project ──
echo "Step 1: Compiling fixture project..."
cd "$FIXTURE_DIR"

# Download Mill wrapper if needed
if [ ! -f mill ]; then
    curl -sL "https://raw.githubusercontent.com/com-lihaoyi/mill/main/mill" > mill
    chmod +x mill
fi

# Compile both JVM and JS targets (generates SemanticDB)
./mill mylib.bar.jvm.compile 2>&1 | tail -1
./mill mylib.bar.js.compile 2>&1 | tail -1
echo ""

# ── Step 2: Generate SemanticDB data and index ──
echo "Step 2: Generating SemanticDB and indexing..."
./mill __.semanticDbData 2>&1 | tail -1

rm -f .scalex/kodex.idx
"$KODEX" index --root . 2>&1 | tail -3
echo ""

IDX="$FIXTURE_DIR/.scalex/kodex.idx"
if [ ! -f "$IDX" ]; then
    echo -e "${RED}Index not created${NC}"
    exit 1
fi

# ── Step 3: Assertions ──
echo "Step 3: Running assertions..."
echo ""

# ── 3a: Basic discoverability ──
echo "  -- Basic discoverability --"
overview=$("$KODEX" overview --idx "$IDX" 2>&1)
assert_contains "overview shows 2 modules" "2 modules" "$overview"
assert_contains "overview has mylib.bar.jvm" "mylib.bar.jvm" "$overview"
assert_contains "overview has mylib.bar.js" "mylib.bar.js" "$overview"

search_error=$("$KODEX" search AppError --idx "$IDX" 2>&1)
assert_contains "search finds AppError" "AppError" "$search_error"

search_svc=$("$KODEX" search SharedService --idx "$IDX" 2>&1)
assert_contains "search finds SharedService" "SharedService" "$search_svc"

search_jvm=$("$KODEX" search JvmService --idx "$IDX" 2>&1)
assert_contains "search finds JvmService" "JvmService" "$search_jvm"

search_js=$("$KODEX" search JsService --idx "$IDX" 2>&1)
assert_contains "search finds JsService" "JsService" "$search_js"
echo ""

# ── 3b: FQN deduplication ──
echo "  -- FQN deduplication --"
search_all=$("$KODEX" search AppError --limit 0 --idx "$IDX" 2>&1)
trait_count=$(echo "$search_all" | grep -c "trait AppError" || echo "0")
assert_eq "AppError trait appears exactly once" "1" "$trait_count"

search_svc_all=$("$KODEX" search SharedService --limit 0 --idx "$IDX" 2>&1)
svc_trait_count=$(echo "$search_svc_all" | grep -c "trait SharedService" || echo "0")
assert_eq "SharedService trait appears exactly once" "1" "$svc_trait_count"
echo ""

# ── 3c: Platform-specific symbols not duplicated ──
echo "  -- Platform-specific symbols --"
# grep for "class JvmService" at start of line (header), not in signature line
jvm_count=$(echo "$search_jvm" | grep -c "^class JvmService\|^  class JvmService" || echo "0")
assert_eq "JvmService appears exactly once" "1" "$jvm_count"

js_count=$(echo "$search_js" | grep -c "^class JsService\|^  class JsService" || echo "0")
assert_eq "JsService appears exactly once" "1" "$js_count"
echo ""

# ── 3d: Canonical file paths (no out/ artifacts) ──
# Shared source files are copied to out/.../jsSharedSources.dest/ for JS compilation.
# kodex should prefer the canonical path (shared/src/) over the out/ copy.
echo "  -- Canonical file paths --"
info_error=$("$KODEX" search AppError --kind trait --idx "$IDX" 2>&1)
assert_eq_known_issue \
    "AppError path should be canonical (not out/)" \
    "0" "$(echo "$info_error" | grep -c "out/" || echo "0")" \
    "JS generatedSources copies to out/; kodex uses out/ path instead of shared/src/"

info_svc=$("$KODEX" search SharedService --kind trait --idx "$IDX" 2>&1)
assert_eq_known_issue \
    "SharedService path should be canonical (not out/)" \
    "0" "$(echo "$info_svc" | grep -c "out/" || echo "0")" \
    "JS generatedSources copies to out/; kodex uses out/ path instead of shared/src/"
echo ""

# ── 3e: Module assignment for shared symbols ──
# Shared symbols are compiled by both JVM and JS. They should be
# discoverable when filtering by either module.
echo "  -- Module assignment (shared symbols) --"
search_jvm_shared=$("$KODEX" search AppError --module jvm --idx "$IDX" 2>&1)
# Known issue: shared symbols are assigned to only one module (last-writer-wins by FQN).
# JS module gets 64 symbols while JVM gets only 6 (platform-specific ones).
jvm_has_shared=$(echo "$search_jvm_shared" | grep -c "trait AppError" || echo "0")
assert_eq_known_issue \
    "AppError findable via --module jvm" \
    "1" "$jvm_has_shared" \
    "shared symbols assigned to last-processed module only (JS wins)"

search_js_shared=$("$KODEX" search AppError --module js --idx "$IDX" 2>&1)
js_has_shared=$(echo "$search_js_shared" | grep -c "trait AppError" || echo "0")
assert_eq "AppError findable via --module js" "1" "$js_has_shared"

# File count: should be 4 (2 shared + 1 jvm + 1 js), not 6
# Overview line: "2 modules, 70 symbols, 6 files"
file_count=$(echo "$overview" | head -1 | sed 's/.*, //' | sed 's/ files.*//')
assert_eq_known_issue \
    "overview shows 4 files (not 6 — shared counted once)" \
    "4" "$file_count" \
    "shared files counted separately for JVM and JS (6 instead of 4)"
echo ""

# ── 3f: Module-specific symbols in correct module ──
echo "  -- Module-specific assignment --"
jvm_in_jvm=$("$KODEX" search JvmService --module jvm --idx "$IDX" 2>&1)
assert_contains "JvmService in jvm module" "JvmService" "$jvm_in_jvm"

js_in_js=$("$KODEX" search JsService --module js --idx "$IDX" 2>&1)
assert_contains "JsService in js module" "JsService" "$js_in_js"

# JvmService should NOT be in the js module (kodex shows Warning + fallback)
jvm_in_js=$("$KODEX" search JvmService --module js --idx "$IDX" 2>&1)
assert_contains "JvmService not in js module (shows warning)" "Warning.*matched no results" "$jvm_in_js"

# JsService should NOT be in the jvm module
js_in_jvm=$("$KODEX" search JsService --module jvm --idx "$IDX" 2>&1)
assert_contains "JsService not in jvm module (shows warning)" "Warning.*matched no results" "$js_in_jvm"
echo ""

# Summary
echo "========================================"
echo -e "  Results: ${GREEN}$PASS passed${NC}, ${RED}$FAIL failed${NC}, ${YELLOW}$KNOWN_FAIL known issues${NC}"
echo "========================================"
if [ $FAIL -gt 0 ]; then exit 1; fi
