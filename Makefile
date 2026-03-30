.PHONY: test coverage coverage-json bench bench-json profile

test:
	cargo test

coverage:
	cargo llvm-cov --summary-only

coverage-json:
	cargo llvm-cov --json

bench:
	cargo bench

bench-json:
	cargo bench --bench kodex_bench -- --output-format bencher 2>/dev/null

profile:
	cargo build --profile profiling
	@echo "Run: samply record ./target/profiling/kodex --idx .scalex/kodex.idx def <symbol>"
	@echo "Run: flamegraph -- ./target/profiling/kodex --idx .scalex/kodex.idx def <symbol>"
