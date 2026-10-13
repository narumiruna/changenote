[default]
all: format lint test

format:
    cargo fmt --check

lint:
    cargo clippy --locked --all-targets -- -D warnings

test:
    cargo test --locked

build:
    cargo build --locked --release
