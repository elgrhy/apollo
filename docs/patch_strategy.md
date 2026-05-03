# APOLLO v1.0 — Patch Strategy

**Audience:** Engineers / Release Managers  
**Classification:** Release Governance  
**Version:** v1.0 (Frozen)

---

## 4.1 Versioning Policy

APOLLO v1.0 is a **frozen architecture release**.

| Version Series | Meaning |
|:---|:---|
| `v1.0` | Production baseline. Architecture, API contracts, and behavior are frozen. |
| `v1.0.x` | Bug-fix patches only. No new features. No behavioral changes. |
| `v1.1` | Future minor release. Not planned. Not in scope for v1.0 operators. |

The v1.0 series guarantees that:

1. The external API contracts (`/metrics`, `/agents/run`, `/agents/stop`, `/agents/add`, hub endpoints) do not change.
2. The `apollo doctor` acceptance test produces identical output for the same system state.
3. The audit log schema (`events.jsonl`) does not change.
4. The binary interface for `apollo` and `apollo-hub` CLI flags does not change.

Any change to the above constitutes a minor version increment, not a patch, and is out of scope for the v1.0 series.

---

## 4.2 Patch Rules

### Allowed in v1.0.x

| Category | Examples |
|:---|:---|
| **Security fixes** | Authentication bypass, key comparison vulnerabilities, path traversal in workspace isolation |
| **Crash fixes** | Panics, unhandled `Result` errors that cause process termination |
| **Memory leaks** | Allocations that grow without bound during normal operation |
| **Race conditions** | Data races in concurrent agent startup/stop paths |
| **Installer fixes** | Checksum verification failures, binary installation path errors, symlink creation issues |

### Not Allowed in v1.0.x

| Category | Examples |
|:---|:---|
| **New features** | New API endpoints, new CLI commands, new agent capabilities |
| **Architectural changes** | Changes to the process model, runtime isolation strategy, or persistence layer |
| **Protocol changes** | Modifications to the `X-Apollo-Key` scheme, request/response schemas, event log format |
| **Performance improvements** | Optimizations that change observable behavior or resource usage profiles |
| **Dependency upgrades** | Unless the upgrade is required to fix a confirmed security vulnerability |

If a proposed fix requires crossing any of these boundaries, it is deferred to a future minor release and the current v1.0.x behavior is documented as a known limitation.

---

## 4.3 Release Discipline

Every v1.0.x patch must satisfy all of the following before release:

### Checksum Update Required

The `CHECKSUMS.sha256` file must be regenerated after building the patched binaries:

```bash
cargo build --release
shasum -a 256 target/release/apollo target/release/apollo-hub > CHECKSUMS.sha256
```

The updated checksums must be committed to the release tag before distribution.

### Doctor Test Must Pass Unchanged

The patch must not alter the output or pass/fail behavior of `apollo doctor` for any system that was previously certified. Run the acceptance test on a clean installation of the patched binaries:

```bash
apollo doctor
```

All checks must return `OK`. If any check that previously passed now fails, the patch is invalid and must be revised.

### API Contract Test

Verify that all existing API endpoints respond identically to pre-patch behavior:

```bash
# Node metrics
curl -s -H "X-Apollo-Key: test-key" http://localhost:8080/metrics

# Agent run (response schema must be unchanged)
curl -s -X POST -H "X-Apollo-Key: test-key" \
  -H "Content-Type: application/json" \
  -d '{"agent": "openclaw", "tenant": "test-user"}' \
  http://localhost:8080/agents/run

# Agent stop
curl -s -X DELETE -H "X-Apollo-Key: test-key" \
  -H "Content-Type: application/json" \
  -d '{"agent": "openclaw", "tenant": "test-user"}' \
  http://localhost:8080/agents/stop
```

Response field names and types must be identical to v1.0. Additional fields may not be added.

### Release Tag Format

```
apollo-v1.0.1
apollo-v1.0.2
...
```

Tags are applied to the `apollo` GitHub repository and are immutable after publication.

### Distribution

1. Build and verify binaries on the release host.
2. Generate updated `CHECKSUMS.sha256`.
3. Commit checksums and tag the release.
4. Notify pilot operators of the patch, including:
   - What was fixed (CVE reference or issue ID)
   - Whether the patch is security-critical
   - The new checksum values
   - The upgrade procedure

### Upgrade Procedure for Operators

```bash
# Pull the patch tag
git fetch origin
git checkout apollo-v1.0.x

# Rebuild
cargo build --release

# Verify checksums
shasum -a 256 -c CHECKSUMS.sha256

# Stop services, install, restart
sudo systemctl stop apollo-node apollo-hub
sudo cp target/release/apollo /usr/local/bin/apollo-node/apollo
sudo cp target/release/apollo-hub /usr/local/bin/apollo-node/apollo-hub
sudo systemctl start apollo-node apollo-hub

# Validate
apollo doctor
```

Downtime during upgrade is expected to be less than 60 seconds per node.

---

## Summary

| Rule | Requirement |
|:---|:---|
| Patch scope | Bug fixes only — no features, no architectural changes |
| Checksum update | Mandatory on every patch |
| Doctor test | Must pass unchanged on patched binaries |
| API contract | Must not change any existing endpoint schema |
| Release tag | Immutable, sequential (v1.0.1, v1.0.2, ...) |
| Operator notification | Required for all patches; immediate for security fixes |
