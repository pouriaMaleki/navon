FROM rust:1.88-bookworm AS builder

WORKDIR /app

COPY Cargo.toml ./
COPY src/ ./src/

RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates wget \
  && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/hsl-proxy /usr/local/bin/hsl-proxy

EXPOSE 3001

HEALTHCHECK --interval=30s --timeout=5s --retries=5 --start-period=10s \
  CMD wget -q -O /dev/null http://127.0.0.1:3001/health

CMD ["hsl-proxy"]
