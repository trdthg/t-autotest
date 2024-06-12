default: test build install

test:
	cargo fmt
	cargo clippy --all -- -D warnings
	cargo test --all

build:
	cargo build --release
	maturin build --features pyo3/extension-module -m ./crates/t-binding/lang/py/Cargo.toml
