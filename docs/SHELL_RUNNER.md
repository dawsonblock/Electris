# Shell runner

ELECTRO now prefers an isolated container runner for shell execution.

## Default behavior

- `ELECTRO_SHELL_BACKEND=auto` tries `docker` first, then `podman`.
- The runner mounts only the session workspace at `/workspace`.
- The root filesystem is read-only.
- `/tmp` is a tmpfs.
- Linux capabilities are dropped.
- Network is disabled unless `ELECTRO_SHELL_ALLOW_NETWORK=1`.
- Direct host fallback clears the inherited environment and points `HOME`, `XDG_*`, and cache directories into the workspace.

## Build a richer runner image

A starter image is included at `docker/shell-runner.Dockerfile`. It is the intended default image for isolated shell execution and includes `git`, `python3`, `node`, `jq`, `ripgrep`, `make`, and common archive tools.

```bash
./scripts/build_shell_runner.sh electro-shell-runner:local
export ELECTRO_SHELL_CONTAINER_IMAGE=electro-shell-runner:local
```

## Smoke test the runner

```bash
./scripts/smoke_shell_runner.sh electro-shell-runner:local
```

That smoke test runs with network disabled and verifies that the expected baseline tools exist inside the container.

## Trusted host fallback

Direct host execution is still available, but only when the operator sets both:

```bash
export ELECTRO_SHELL_BACKEND=host
export ELECTRO_ENABLE_HOST_SHELL=1
```

Additional notes:

- Host fallback no longer inherits the full parent environment.
- `HOME`, `XDG_CACHE_HOME`, `XDG_CONFIG_HOME`, `XDG_STATE_HOME`, and `TMPDIR` are redirected into workspace-local `.electro-host-*` directories.
- Shell launchers such as `sh`, `bash`, `dash`, `zsh`, and `sudo` are blocked in host fallback.
- If you really need to pass through selected secrets or tokens, set `ELECTRO_SHELL_PASSTHROUGH_ENV` to a comma-separated allowlist of exact environment variable names.

That fallback is intentionally awkward. It is there for emergencies and tightly controlled environments, not as the default operating mode.
