# Runbook: Vault Failure

> **Alert:** `VaultDecryptionFailure` / `VaultHighErrorRate`
> **Severity:** Critical
> **Service:** vault
> **Response Time:** < 5 minutes
> **Last Updated:** 2026-03-08

---

## Symptoms and Detection

### Triggering Alerts

| Alert | Condition | Duration |
|-------|-----------|----------|
| `VaultDecryptionFailure` | `increase(electro_vault_decryption_failures_total[5m]) > 0` | Immediate (any failure) |
| `VaultHighErrorRate` | Vault error rate > 0.1% | 2 minutes |

### Related Warning Alerts

| Alert | Condition | Duration |
|-------|-----------|----------|
| `VaultLatencyHigh` | Vault operation p99 > 10ms | 5 minutes |
| `VaultKeyPermissionDrift` | `vault.key` permissions != 0600 | 1 minute |
| `VaultKeyCountHigh` | Vault contains > 500 keys | 1 hour |

### Observable Symptoms

- PagerDuty incident fires with `severity=critical, service=vault`.
- `electro_vault_decryption_failures_total` counter has incremented.
- `electro_vault_operation_total{status="error"}` is increasing.
- Provider connections fail because API keys cannot be decrypted from the vault.
- Log messages containing `ElectroError::Vault("decryption failed: ...")`.
- Log messages containing `ElectroError::Vault("corrupt vault file: ...")`.

---

## Impact Assessment

| Dimension | Impact |
|-----------|--------|
| **User-facing** | If provider API keys are stored in the vault, all AI completions fail. Secrets-dependent operations halt. |
| **SLO burn** | Vault Operations SLO (99.99%) has only 4.3 min/month budget. Decryption failures have zero-tolerance policy. |
| **Blast radius** | All components that resolve secrets via `vault://electro/` URIs are affected. |
| **Data loss risk** | HIGH. If `vault.key` is lost or corrupted, all encrypted secrets in `vault.enc` become permanently unrecoverable. |
| **Security risk** | Decryption failure may indicate key tampering or unauthorized access to vault files. |

---

## Step-by-Step Diagnosis

### Step 1: Classify the failure type

Check logs to determine whether this is a decryption failure, a file I/O failure, or a data corruption issue:

```bash
journalctl -u electro --since "10 minutes ago" | grep -i "vault\|decrypt\|encrypt"
```

Failure patterns:

| Log Pattern | Cause |
|-------------|-------|
| `decryption failed: aead::Error` | Key mismatch or ciphertext corruption |
| `vault key must be exactly 32 bytes` | `vault.key` file is truncated or wrong size |
| `failed to read vault key` | `vault.key` file missing or unreadable |
| `corrupt vault file` | `vault.enc` JSON is malformed |
| `failed to read vault file` | `vault.enc` file missing or unreadable |
| `failed to write vault file` | Disk full or permission denied |
| `bad nonce base64` / `bad ciphertext base64` | Individual entry corruption in `vault.enc` |

### Step 2: Verify vault.key file integrity

```bash
# Check existence and size (must be exactly 32 bytes)
ls -la ~/.electro/vault.key
wc -c ~/.electro/vault.key
# Expected output: 32 ~/.electro/vault.key

# Check permissions (must be 0600 = 384 decimal)
stat -c "%a %s" ~/.electro/vault.key   # Linux
stat -f "%Lp %z" ~/.electro/vault.key  # macOS
# Expected: 600 32

# Verify it contains binary data (not text/base64)
file ~/.electro/vault.key
# Expected: "data" (raw binary)
xxd ~/.electro/vault.key | head -3
```

### Step 3: Verify vault.enc file integrity

```bash
# Check existence and readability
ls -la ~/.electro/vault.enc

# Validate JSON structure
python3 -m json.tool ~/.electro/vault.enc > /dev/null 2>&1 && echo "Valid JSON" || echo "INVALID JSON"

# Check structure: should be a JSON object with string keys
python3 -c "
import json
with open('$HOME/.electro/vault.enc') as f:
    data = json.load(f)
print(f'Keys: {len(data)}')
for key in list(data.keys())[:5]:
    entry = data[key]
    print(f'  {key}: nonce={len(entry.get(\"nonce\",\"\"))} chars, ct={len(entry.get(\"ciphertext\",\"\"))} chars')
"
```

### Step 4: Check disk health

```bash
# Check available disk space
df -h ~/.electro/

# Check for filesystem errors
dmesg | grep -i "error\|fault\|corrupt" | tail -20

# Check I/O latency
iostat -x 1 3
```

### Step 5: Check file permissions and ownership

```bash
# Full directory check
ls -la ~/.electro/
# vault.key should be: -rw------- (0600)
# vault.enc should be: -rw-r--r-- or -rw-------

# Check process user matches file owner
ps aux | grep electro | grep -v grep
stat ~/.electro/vault.key
```

### Step 6: Test vault operations in isolation

```bash
# Use the ELECTRO CLI to test vault read
electro vault list

# Try to read a specific key
electro vault get <key-name>
```

---

## Remediation

### Remediation A: vault.key Permission Drift

If `VaultKeyPermissionDrift` fired (permissions are not 0600):

```bash
chmod 600 ~/.electro/vault.key

# Verify
stat -c "%a" ~/.electro/vault.key  # Linux
stat -f "%Lp" ~/.electro/vault.key  # macOS
# Must show: 600
```

No restart required. The next vault operation will succeed.

### Remediation B: vault.key File Missing or Corrupted

**WARNING: If the original vault.key is lost, all secrets in vault.enc are PERMANENTLY UNRECOVERABLE.**

1. Check for backups:
   ```bash
   # Check common backup locations
   ls -la ~/.electro/vault.key.bak
   ls -la /backup/electro/vault.key
   ```

2. If a backup exists, restore it:
   ```bash
   cp /backup/electro/vault.key ~/.electro/vault.key
   chmod 600 ~/.electro/vault.key
   ```

3. If no backup exists and vault.key is corrupted:
   - All existing secrets are lost.
   - Remove the corrupted vault files:
     ```bash
     mv ~/.electro/vault.key ~/.electro/vault.key.corrupted
     mv ~/.electro/vault.enc ~/.electro/vault.enc.corrupted
     ```
   - ELECTRO will generate a new vault.key on next startup via `LocalVault::ensure_key()`.
   - Re-provision all secrets (API keys, etc.) manually.

4. Restart ELECTRO:
   ```bash
   systemctl restart electro
   ```

### Remediation C: vault.enc Corruption

1. If `vault.enc` has invalid JSON:
   ```bash
   # Attempt to salvage
   python3 -c "
   import json
   with open('$HOME/.electro/vault.enc') as f:
       raw = f.read()
   # Try to find valid JSON prefix
   for i in range(len(raw), 0, -1):
       try:
           data = json.loads(raw[:i])
           with open('$HOME/.electro/vault.enc.salvaged', 'w') as out:
               json.dump(data, out, indent=2)
           print(f'Salvaged {len(data)} keys')
           break
       except:
           continue
   "
   ```

2. If salvage succeeds:
   ```bash
   cp ~/.electro/vault.enc ~/.electro/vault.enc.corrupted
   mv ~/.electro/vault.enc.salvaged ~/.electro/vault.enc
   systemctl restart electro
   ```

3. If individual entries are corrupted (bad base64 nonce/ciphertext), remove only the affected entries from the JSON and restart.

### Remediation D: Disk Space Exhaustion

The `LocalVault::flush()` method rewrites the entire `vault.enc` on every mutation. If disk is full:

```bash
# Free disk space
df -h ~/.electro/
# Remove unnecessary files or expand volume

# Verify write capability
touch ~/.electro/test_write && rm ~/.electro/test_write
```

### Remediation E: Security Incident Response

If decryption failure is suspected to be caused by tampering:

1. **Preserve evidence:**
   ```bash
   cp -a ~/.electro/ /tmp/electro-incident-$(date +%s)/
   sha256sum ~/.electro/vault.key ~/.electro/vault.enc
   ```

2. **Check file modification times:**
   ```bash
   stat ~/.electro/vault.key ~/.electro/vault.enc
   ```

3. **Check access logs:**
   ```bash
   # auditd logs if enabled
   ausearch -f ~/.electro/vault.key
   ```

4. Escalate as a security incident per the [Incident Response](./incident-response.md) playbook (SEV1).

5. After investigation, rotate all secrets:
   - Generate new vault.key
   - Re-provision all API keys from source
   - Rotate any credentials that were stored in the vault

---

## Prevention Measures

1. **Automated backups:** Back up `vault.key` to a separate secure location on every startup. The 32-byte key file is trivial to back up.
   ```bash
   # Example backup script run by systemd ExecStartPre
   cp ~/.electro/vault.key /secure-backup/vault.key.$(date +%Y%m%d)
   ```

2. **File integrity monitoring:** Use AIDE, Tripwire, or similar to detect unauthorized changes to `vault.key` and `vault.enc`.

3. **Permission enforcement:** The heartbeat loop checks `vault.key` permissions. Ensure `VaultKeyPermissionDrift` warning alert is routed to the team.

4. **Disk space monitoring:** Alert on disk usage > 80% for the volume containing `~/.electro/`.

5. **Key rotation plan:** Document and periodically test vault key rotation:
   - Decrypt all secrets with old key
   - Generate new key
   - Re-encrypt all secrets with new key
   - Verify round-trip

6. **Vault key count management:** Keep vault under 500 keys to avoid flush latency issues (B4 bottleneck in capacity baseline). Clean up unused secrets quarterly.

7. **Migration plan:** For cloud deployments with > 500 keys or high write rates, plan migration to AWS KMS or HashiCorp Vault backend.

---

## Related Runbooks

- [Provider Unreachable](./provider-unreachable.md) -- vault failure causes API key resolution failure
- [Gateway Down](./gateway-down.md) -- vault failure at startup can prevent gateway initialization
- [Incident Response](./incident-response.md) -- security incident escalation
