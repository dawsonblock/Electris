# Docker / Docker Compose — OAuth Setup

ELECTRO's OAuth login requires a browser and a localhost callback. Inside a container, neither is available. The solution: authenticate on your local machine, then mount the token file into the container.

## Quick Start

### 1. Authenticate on your local machine

```bash
# Install and build ELECTRO locally (or use a pre-built binary)
git clone https://github.com/dawsonblock/Electro.git
cd electro
cargo build --release

# Authenticate — opens browser, log into ChatGPT
./target/release/electro auth login --output ./oauth.json
```

This creates `oauth.json` in your current directory (and also saves to `~/.electro/oauth.json` as usual).

### 2. Copy the token to your server (if remote)

```bash
scp oauth.json yourserver:/path/to/electro/oauth.json
```

### 3. Mount into your container

```yaml
# docker-compose.yml
version: "3.8"
services:
  electro:
    build: .
    volumes:
      - ./oauth.json:/root/.electro/oauth.json
      - ./electro.toml:/root/.electro/electro.toml
    environment:
      - TELEGRAM_BOT_TOKEN=${TELEGRAM_BOT_TOKEN}
    restart: unless-stopped
```

```bash
docker-compose up -d
```

ELECTRO auto-detects `~/.electro/oauth.json` at startup — no config changes needed.

## How Token Refresh Works

OAuth tokens expire after ~1 hour. ELECTRO auto-refreshes them using the refresh token (valid ~10 days). The refreshed token is written back to `oauth.json`. Because the file is volume-mounted, the refreshed token persists across container restarts.

**Important:** The volume mount must be a file bind mount (not a directory), so writes inside the container propagate to the host.

## Re-authentication

If the refresh token expires (~10 days without use), re-run on your local machine:

```bash
electro auth login --output ./oauth.json
```

Then restart the container to pick up the new token.

## Headless Servers (no browser)

If your local machine also has no browser (e.g., another server):

```bash
electro auth login --headless --output ./oauth.json
```

This prints a URL. Open it on any device with a browser, complete the login, then paste the redirect URL back into the terminal.

## Kubernetes

Same approach — authenticate locally, create a Secret from the token file:

```bash
electro auth login --output ./oauth.json
kubectl create secret generic electro-oauth --from-file=oauth.json=./oauth.json
```

Mount the secret in your pod spec:

```yaml
volumeMounts:
  - name: oauth-token
    mountPath: /root/.electro/oauth.json
    subPath: oauth.json
volumes:
  - name: oauth-token
    secret:
      secretName: electro-oauth
```

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| "No OAuth tokens found" at startup | oauth.json not mounted at `/root/.electro/oauth.json` | Check volume mount path |
| "Token refresh failed" | Refresh token expired (>10 days) | Re-run `electro auth login --output` locally |
| Container starts in onboarding mode | oauth.json is empty or malformed | Re-authenticate locally |
| Token refreshes but lost on restart | Using a directory mount instead of file mount | Use `./oauth.json:/root/.electro/oauth.json` (file:file) |
