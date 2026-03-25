# Build stage
FROM rust:1.75 AS builder

WORKDIR /app
COPY . .

RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/spine-server /app/spine-server

EXPOSE 8080

ENV PORT=8080
ENV RUST_LOG=info

CMD ["/app/spine-server"]
