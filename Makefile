test:
	cargo fmt
	cargo clippy -- -D warnings
	cargo test

build:
	cargo build
	maturin