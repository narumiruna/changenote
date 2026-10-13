FROM rust:1-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock README.md LICENSE ./
COPY crates ./crates
RUN cargo build --locked --release -p changenote

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends git ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /app/target/release/changenote /usr/local/bin/changenote
WORKDIR /repo
ENTRYPOINT ["changenote"]
