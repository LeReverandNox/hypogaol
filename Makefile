.PHONY: build test test-hardware

build:
	cargo build --release

test:
	cargo test --lib --test unit

test-hardware:
	cargo test --test hardware -- --ignored --nocapture --test-threads=1
