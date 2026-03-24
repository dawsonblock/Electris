This archive now includes a containerized remote browser runtime and proxy sidecar. The recommended mode is `ELECTRO_BROWSER_ISOLATION_MODE=remote`, which connects over CDP to `http://127.0.0.1:9223` instead of spawning Chrome on the ELECTRO host.

# Browser hardening

This archive tightens the browser surface but does not pretend to fully sandbox it.

## What changed

- Browser navigation only allows `http` and `https` URLs.
- Direct navigation to private, loopback, local, or internal hosts is blocked.
- Browser navigation now resolves the destination hostname before navigation and blocks names that resolve to private IPs.
- After navigation, the final URL is validated again. This catches many redirect cases that land on blocked destinations.
- Browser and `web_fetch` share an optional domain allowlist through `ELECTRO_PUBLIC_WEB_ALLOWLIST`.
- Browser JavaScript evaluation is disabled by default. Enable it only with `ELECTRO_BROWSER_ALLOW_EVAL=1`.
- Browser can be pointed at an operator-managed outbound proxy with `ELECTRO_BROWSER_PROXY_SERVER`.
- Additional Chrome flags reduce background networking and LAN-adjacent chatter.

## Example strict mode

```bash
export ELECTRO_PUBLIC_WEB_ALLOWLIST=github.com,docs.rs
export ELECTRO_BROWSER_PROXY_SERVER=http://127.0.0.1:8888
export ELECTRO_BROWSER_PROXY_BYPASS=
export ELECTRO_BROWSER_ALLOW_EVAL=0
```

## What this still does not do

- It does not put Chrome in its own network namespace.
- The reference sandbox now routes browser egress through a dedicated proxy container on an internal Docker network.
- It still does not attest subresource-by-subresource policy inside arbitrary public web applications; the main enforcement boundary is now the container network plus proxy.
- It does not replace a real egress proxy, firewall rule set, or sandbox.

Use these controls as a reduction in risk, not as a claim of full browser isolation.
