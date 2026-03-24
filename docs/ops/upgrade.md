# Operations Guide: Upgrading Electro

This guide covers upgrading Electro in production and local environments.

## Upgrading Electro

### Using electro update

The recommended upgrade method:

```bash
electro update
```

This command:
- Checks for available updates from the configured release channel
- Downloads the new binary to a staging location
- Verifies the binary checksum
- Atomic swap to the new version

### Manual git pull + build

For environments without network access to release artifacts:

```bash
cd /opt/electro
git fetch --all
git checkout <version-tag>
cargo build --release --bin electro
sudo cp target/release/electro /usr/local/bin/electro
```

### Version check after upgrade

Verify the upgrade succeeded:

```bash
electro --version
```

Compare with expected version from release notes.

## Pre-Upgrade Checklist

### Backup configuration

```bash
# Backup config files
cp /etc/electro/config.toml /etc/electro/config.toml.$(date +%Y%m%d)
cp -r ~/.electro ~/.electro.backup.$(date +%Y%m%d)

# Backup vault (if using)
cp ~/.electro/vault.enc ~/.electro/vault.enc.backup
```

### Check release notes

Before upgrading, review the release notes for:
- Breaking changes
- New required configuration fields
- Deprecated features
- Security patches

Release notes are available at: `https://github.com/<org>/electro/releases`

### Review breaking changes

Run the configuration validator with the new binary against your config:

```bash
electro config validate --config /etc/electro/config.toml
```

Common breaking changes to watch for:
- Removed configuration keys
- Changed default values
- New required fields
- Protocol version bumps

## Post-Upgrade Verification

### Health check

```bash
# Start Electro and check health endpoint
curl http://localhost:8080/health

# Or with TLS
curl -k https://localhost:443/health
```

Expected response: `{"status":"healthy"}`

### Configuration validation

```bash
electro config validate
```

Address any warnings or errors before proceeding.

### Test basic functionality

1. Send a test message through each configured channel
2. Verify AI provider responds correctly
3. Confirm memory persistence works (check database/files)
4. Test any custom skills or tools

## Rollback Procedure

### Reverting to previous binary

```bash
# If using electro update with automatic rollback
electro update --rollback

# Manual rollback
sudo cp /usr/local/bin/electro /usr/local/bin/electro.new
sudo cp /usr/local/bin/electro.old /usr/local/bin/electro
```

### Restoring configuration

```bash
# Restore config from backup
sudo cp /etc/electro/config.toml.YYYYMMDD /etc/electro/config.toml

# Restore vault from backup
cp ~/.electro.backup/vault.enc ~/.electro/vault.enc
```

### When to rollback

Roll back immediately if:
- Health endpoint returns errors
- Configuration validation fails repeatedly
- Channels fail to connect
- Critical functionality is broken
- Security vulnerability in new version

## Configuration Migration

### What to do if config schema changes

1. Run `electro config validate` to identify invalid keys
2. Review the release notes for migration guidance
3. Update your config file with new schema

Example migration for a removed key:

```toml
# Before (deprecated)
[gateway]
timeout = 30

# After (current)
[gateway]
timeout_seconds = 30
```

### Using electro config validate

```bash
# Validate specific config file
electro config validate --config /path/to/config.toml

# Show all config with resolved values (secrets masked)
electro config show
```

The validator checks:
- TOML/YAML syntax
- Required fields for enabled features
- Environment variable resolution
- Vault secret existence
- Non-empty channel allowlists
