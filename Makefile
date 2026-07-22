.PHONY: build test test-hardware

build:
	cargo build --release

test:
	cargo test

test-hardware:
	cargo test -- --ignored
