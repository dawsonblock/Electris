FROM debian:bookworm-20240408-slim

RUN apt-get update && apt-get install -y --no-install-recommends         squid         netcat-openbsd         && rm -rf /var/lib/apt/lists/*

COPY docker/browser-proxy/squid.conf /etc/squid/squid.conf

EXPOSE 3128
CMD ["/usr/sbin/squid", "-N", "-f", "/etc/squid/squid.conf"]
