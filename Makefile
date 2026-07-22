.PHONY: build test test-hardware

build:
	cargo build --release

test:
	cargo test --test unit

test-hardware:
	cargo test --test hardware -- --ignored
