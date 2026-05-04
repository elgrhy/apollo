# APOLLO v1.2 — Patch Strategy

**Audience:** Engineers / Release Managers
**Classification:** Release Governance
**Version:** v1.2

---

## 1. Versioning Policy

| Version Series | Meaning |
|---------------|---------|
| `v1.0` | Original frozen baseline. Superseded by v1.2. Deployments at v1.0 may upgrade directly to v1.2. |
| `v1.1` | Intermediate release (URL/git sourcing, rollback, catalog). Superseded by v1.2. |
| `v1.2` | Current stable release. Full enterprise feature set: TLS, JWT, secrets, metering, volumes, webhooks, multi-region. |
| `v1.2.x` | Bug-fix patches only. No new features. No behavioral changes. |
| `v1.3+` | Future. Dashboard UI, Python/Node/Go SDKs. Will not break v1.2 API contracts. |
| `v2.0+` | Future. Kubernetes operator / Helm chart. |

The v1.2 series guarantees that:

1. The external API contracts (`/health`, `/metrics`, `/agents/*`, `/tenants/*`, `/usage/*`, hub endpoints) do not change.
2. The `apollo doctor` acceptance test produces identical output for the same system state.
3. The audit log schema (`events.jsonl`) does not change.
4. The binary interface for `apollo` and `apollo-hub` CLI flags does not change.
5. The webhook payload schema (`WebhookPayload` fields) does not change.

Any change to the above constitutes a minor version increment, not a patch, and is out of scope for the v1.2 series.

---

## 2. Patch Rules

### Allowed in v1.2.x

| Category | Examples |
|----------|---------|
| **Security fixes** | Authentication bypass, key comparison vulnerabilities, path traversal in workspace isolation, secrets leaking into logs |
| **Crash fixes** | Panics, unhandled `Result` errors that cause process termination |
| **Memory leaks** | Allocations that grow without bound during normal operation |
| **Race conditions** | Data races in concurrent agent startup/stop paths, usage accumulation races |
| **Webhook delivery bugs** | Payload serialization errors, signature computation errors, retry logic failures |
| **Metering accuracy bugs** | CPU/memory accounting errors that produce incorrect usage totals |
| **Installer fixes** | Checksum verification failures, binary installation path errors, symlink creation issues |

### Not Allowed in v1.2.x

| Category | Examples |
|----------|---------|
| **New features** | New API endpoints, new CLI commands, new agent capabilities, dashboard UI |
| **Architectural changes** | Changes to the process model, runtime isolation strategy, or persistence layer |
| **Protocol changes** | Modifications to auth schemes, request/response schemas, event log format, webhook payload fields |
| **Performance improvements** | Optimizations that change observable behavior or resource usage profiles |
| **Dependency upgrades** | Unless required to fix a confirmed security vulnerability |

---

## 3. Release Discipline

Every v1.2.x patch must satisfy all of the following before release:

### Checksum Update Required

```bash
cargo build --release
shasum -a 256 target/release/apollo target/release/apollo-hub > CHECKSUMS.sha256
```

Updated checksums must be committed to the release tag before distribution.

### Doctor Test Must Pass Unchanged

```bash
apollo doctor
```

All checks must return `OK` on a clean installation of the patched binaries. If any check that previously passed now fails, the patch is invalid.

### API Contract Tests

```bash
# Auth: both methods must work
curl -s -H "X-Apollo-Key: test-key" http://localhost:8080/health
curl -s -H "Authorization: Bearer <JWT>" http://localhost:8080/health

# Core agent endpoints
curl -s -H "X-Apollo-Key: test-key" http://localhost:8080/metrics
curl -s -H "X-Apollo-Key: test-key" http://localhost:8080/agents/list

curl -s -X POST http://localhost:8080/agents/run \
  -H "X-Apollo-Key: test-key" -H "Content-Type: application/json" \
  -d '{"agent":"openclaw","tenant":"test-user"}'

curl -s -X DELETE http://localhost:8080/agents/stop \
  -H "X-Apollo-Key: test-key" -H "Content-Type: application/json" \
  -d '{"agent":"openclaw","tenant":"test-user"}'

# Secrets API
curl -s -X PUT http://localhost:8080/tenants/test-user/secrets \
  -H "X-Apollo-Key: test-key" -H "Content-Type: application/json" \
  -d '{"secrets":{"TEST_KEY":"test-value"}}'

# Usage API
curl -s -H "X-Apollo-Key: test-key" http://localhost:8080/usage/test-user

# Hub endpoints
curl -s http://localhost:9191/summary
curl -s http://localhost:9191/nodes/status
curl -s "http://localhost:9191/nodes/best?region=default"
curl -s http://localhost:9191/catalog
curl -s http://localhost:9191/regions
```

Response field names and types must be identical to v1.2. Additional fields may not be added.

### Release Tag Format

```
apollo-v1.2.1
apollo-v1.2.2
...
```

Tags are applied to the `apollo` GitHub repository and are immutable after publication.

### Distribution

1. Build and verify binaries on the release host
2. Generate updated `CHECKSUMS.sha256`
3. Commit checksums and tag the release
4. Notify pilot operators, including:
   - What was fixed (CVE reference or issue ID)
   - Whether the patch is security-critical (immediate notification required for SEC category)
   - The new checksum values
   - The upgrade procedure

---

## 4. Upgrade Procedure for Operators

```bash
# Pull the patch tag
git fetch origin
git checkout apollo-v1.2.x

# Rebuild
cargo build --release

# Verify checksums
shasum -a 256 -c CHECKSUMS.sha256

# Stop services, install, restart
sudo systemctl stop apollo-node apollo-hub
sudo cp target/release/apollo     /usr/local/bin/apollo-node/apollo
sudo cp target/release/apollo-hub /usr/local/bin/apollo-node/apollo-hub
sudo systemctl start apollo-node apollo-hub

# Validate
apollo doctor
```

Expected downtime per node: less than 60 seconds.

In-flight agents at the time of the restart must be re-launched by the control plane after the node comes back online. Usage data, secrets, volumes, and the audit log are preserved across the upgrade.

---

## 5. Summary

| Rule | Requirement |
|------|-------------|
| Patch scope | Bug fixes only — no features, no architectural changes |
| Checksum update | Mandatory on every patch |
| Doctor test | Must pass unchanged on patched binaries |
| API contract | Must not change any existing endpoint schema |
| Webhook schema | Must not change `WebhookPayload` field names or types |
| Usage schema | Must not change `TenantUsage` field names or types |
| Release tag | Immutable, sequential (v1.2.1, v1.2.2, ...) |
| Operator notification | Required for all patches; immediate for SEC category |
