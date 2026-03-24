FROM debian:bookworm-slim

ENV DEBIAN_FRONTEND=noninteractive \
    HOME=/tmp

RUN apt-get update && apt-get install -y --no-install-recommends \
    bash \
    build-essential \
    ca-certificates \
    coreutils \
    curl \
    fd-find \
    findutils \
    git \
    jq \
    make \
    nodejs \
    npm \
    patch \
    procps \
    python3 \
    python3-pip \
    python3-venv \
    ripgrep \
    sqlite3 \
    tar \
    tini \
    unzip \
    xz-utils \
    zip \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /workspace
ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["bash"]
