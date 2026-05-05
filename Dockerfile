# syntax=docker/dockerfile:1

# ---- build ----
FROM rust:1.88-bookworm AS builder
WORKDIR /app

# Cache deps: build a stub crate against Cargo.toml/Cargo.lock first
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src && echo 'fn main(){}' > src/main.rs && \
    cargo build --release && \
    rm -rf src target/release/deps/rate_my_roomate* target/release/rate-my-roomate*

# Real build
COPY src ./src
COPY templates ./templates
COPY static ./static
RUN cargo build --release

# ---- runtime ----
FROM debian:bookworm-slim
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/rate-my-roomate /usr/local/bin/rate-my-roomate
COPY static ./static

RUN mkdir -p /data
ENV DATABASE_URL=sqlite:///data/app.db
EXPOSE 3000

CMD ["/usr/local/bin/rate-my-roomate"]
