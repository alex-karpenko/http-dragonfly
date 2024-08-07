ARG RUST_VERSION=1.80-bookworm
FROM rust:${RUST_VERSION} AS builder

WORKDIR /src
COPY . .

RUN cargo build --release
RUN strip target/release/http-dragonfly

# Runtime stage
FROM debian:12-slim

RUN apt update && apt install -y ca-certificates && apt clean

USER nobody
WORKDIR /app
COPY --from=builder /src/target/release/http-dragonfly /app/

ENTRYPOINT ["/app/http-dragonfly"]
CMD ["--help"]
