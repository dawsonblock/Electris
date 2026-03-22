# ---- Builder stage ----
FROM rust:1.83 AS builder

ARG GIT_HASH=unknown
ARG BUILD_DATE=unknown

WORKDIR /app

# Copy manifests first for dependency caching
COPY Cargo.toml Cargo.lock build.rs ./
COPY crates/ crates/
COPY src/ src/

# Set env vars so build.rs fallback works even without git
ENV GIT_HASH=${GIT_HASH}
ENV BUILD_DATE=${BUILD_DATE}

RUN cargo build --release

# ---- Runtime stage ----
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        chromium \
    && rm -rf /var/lib/apt/lists/*

# Create non-root electro user
RUN groupadd -g 1000 electro && \
    useradd -u 1000 -g electro -m -d /home/electro electro

# chromiumoxide looks for "chromium" or "chromium-browser" on PATH
ENV CHROME_PATH=/usr/bin/chromium
ENV HOME=/home/electro

WORKDIR /app

COPY --from=builder /app/target/release/electro ./electro

RUN chown -R electro:electro /app /home/electro

EXPOSE 8080

USER electro

ENTRYPOINT ["./electro", "start"]
