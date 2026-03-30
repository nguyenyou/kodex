# queries.sh — snapshot test definitions for snapshot
#
# run_test <test_name> <command> [args...]   — appends --idx automatically
# run_help <test_name> <subcommand>          — runs <subcommand> --help (no index)

# ── help ───────────────────────────────────────────────────────────────────

run_help "help__top_level"
run_help "help__search"          search
run_help "help__info"            info
run_help "help__calls"           calls
run_help "help__index"           index
run_help "help__noise"           noise
