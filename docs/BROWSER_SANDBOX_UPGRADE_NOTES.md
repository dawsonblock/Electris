# Browser sandbox upgrade notes

## Summary

This pass upgrades the browser from process-level hardening to an OS-level boundary built around a remote Chrome container plus a dedicated proxy sidecar.

## New default

- `ELECTRO_BROWSER_ISOLATION_MODE=remote`
- `ELECTRO_BROWSER_REMOTE_URL=http://127.0.0.1:9223`

## Code changes

- Browser tool now connects to an already running remote browser through `Browser::connect` when remote isolation mode is enabled.
- Interactive browser session login uses the same remote-isolated runtime path.
- Browser pool uses the same remote-isolated runtime path.
- Browser runtime now fails closed when remote mode is selected but the isolated browser is unavailable.

## Ops changes

- Added `docker-compose.browser-sandbox.yml`
- Added `docker/browser-sandbox/*`
- Added `docker/browser-proxy/*`
- Added build/run/stop/smoke scripts
