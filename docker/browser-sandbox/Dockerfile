FROM debian:bookworm-20240408-slim

RUN apt-get update && apt-get install -y --no-install-recommends         chromium         ca-certificates         fonts-liberation         fonts-noto-color-emoji         tini         && rm -rf /var/lib/apt/lists/*

RUN useradd -m -u 10001 -s /bin/bash chrome

COPY docker/browser-sandbox/entrypoint.sh /usr/local/bin/temm1e-browser-entrypoint
RUN chmod 0755 /usr/local/bin/temm1e-browser-entrypoint

USER chrome
WORKDIR /home/chrome

EXPOSE 9222
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/temm1e-browser-entrypoint"]
