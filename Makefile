.PHONY: build test test-hardware coverage audit

build:
	cargo build --release

test:
	cargo test --lib --test unit

test-hardware:
	cargo test --test hardware -- --ignored --nocapture --test-threads=1

coverage:
	cargo llvm-cov --lib --test unit --lcov --output-path lcov.info

audit:
	cargo audit
