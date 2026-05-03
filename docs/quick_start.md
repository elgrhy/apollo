# APOLLO v1.0 — Quick Start Guide

**Audience:** IT Engineers  
**Classification:** Production Onboarding  
**Version:** v1.0 (Frozen)

---

## Prerequisites

| Requirement | Minimum Version | Notes |
|:---|:---|:---|
| Linux (x86_64) | Ubuntu 20.04 / RHEL 8 | Systemd required |
| Rust / Cargo | 1.75+ | `curl https://sh.rustup.rs -sSf \| sh` |
| Git | 2.30+ | For repository access |
| Disk | 2 GB free | Build artifacts + logs |
| Memory | 512 MB minimum | Per node instance |

---

## Step 1 — Clone the Repository

APOLLO is distributed via a private GitHub repository. SSH access must be provisioned before proceeding.

```bash
git clone --branch apollo-v1.0 --depth 1 git@github.com:elgrhy/apollo.git
cd apollo
```

> **Air-gapped environments:** Contact your Apollo distribution contact for a pre-built tarball and skip to Step 3.

---

## Step 2 — Build Production Binaries

```bash
cargo build --release
```

Build artifacts are written to `target/release/`. Expected output:

```
target/release/apollo        # CLI + Node daemon
target/release/apollo-hub    # Hub coordinator daemon
```

Build time: approximately 3–5 minutes on a standard server.

---

## Step 3 — Verify Binary Integrity

Before installation, verify the SHA-256 checksums against the release manifest:

```bash
shasum -a 256 -c CHECKSUMS.sha256
```

Expected output (both lines must pass):

```
target/release/apollo: OK
target/release/apollo-hub: OK
```

**Do not proceed if either checksum fails.** The release artifact may be corrupt or tampered.

---

## Step 4 — Install

```bash
./install.sh
```

The installer performs:
- Copies binaries to `/usr/local/bin/apollo-node/`
- Creates the `/usr/local/bin/apollo` global symlink
- Runs `apollo doctor` to confirm a clean installation

Alternatively, use the one-line network installer (requires outbound internet access):

```bash
curl -fsSL https://get.apollo.systems | bash
```

---

## Step 5 — First Run

Start a node on the default interface and port (`0.0.0.0:8080`):

```bash
apollo node start
```

To specify a custom listen address, port, and secret key:

```bash
apollo node start --listen 0.0.0.0:8080 --secret-keys "your-secret-key"
```

Start the hub coordinator (default port `0.0.0.0:9090`):

```bash
/usr/local/bin/apollo-node/apollo-hub start
```

---

## Step 6 — Verify Installation

Run the acceptance validator:

```bash
apollo doctor
```

All checks must return `OK`. A `PRODUCTION READY` result confirms that the process sandbox, event spine, and state persistence modules are fully operational.

---

## Step 7 — Run Your First Agent

```bash
apollo agent run openclaw --tenant demo-user
```

Verify the agent is running:

```bash
apollo agent list
```

Query the node API directly:

```bash
curl -H "X-Apollo-Key: your-secret-key" http://localhost:8080/metrics
```

Expected response:

```json
{"active_agents": 1, "max_agents": 50}
```

---

## Quick Reference

| Command | Purpose |
|:---|:---|
| `apollo node start` | Start the node daemon |
| `apollo doctor` | Validate installation |
| `apollo agent run <name> --tenant <id>` | Launch an agent |
| `apollo agent list` | List active agents |
| `GET /metrics` | Node health and capacity |

---

*For production deployment with systemd, see [Production Deployment Guide](production_deployment.md).*  
*For network configuration and firewall rules, see [Network & Security Guide](network_security.md).*
