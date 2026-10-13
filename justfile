[default]
all: format lint test

format:
    cargo fmt --all --check

lint:
    cargo clippy --locked --workspace --all-targets -- -D warnings

test:
    cargo test --locked --workspace

build:
    cargo build --locked --release -p changenote
