ARG RUST_VERSION=1.92-bookworm
FROM rust:${RUST_VERSION} AS builder

WORKDIR /src
COPY . .

RUN cargo build --release
RUN strip target/release/http-dragonfly

# Runtime stage
FROM gcr.io/distroless/cc-debian12

USER nobody
WORKDIR /app
COPY --from=builder /src/target/release/http-dragonfly /app/

ENTRYPOINT ["/app/http-dragonfly"]
CMD ["--help"]
