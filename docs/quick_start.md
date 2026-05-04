# APOLLO v1.2 — Quick Start Guide

**Audience:** IT Engineers
**Classification:** Production Onboarding
**Version:** v1.2

---

## Prerequisites

| Requirement | Minimum Version | Notes |
|-------------|----------------|-------|
| Linux (x86_64 / aarch64) | Ubuntu 20.04 / RHEL 8 | systemd required for production; macOS and Windows supported |
| Rust / Cargo | 1.75+ | `curl https://sh.rustup.rs -sSf \| sh` |
| Git | 2.30+ | For repository access and git-sourced agent packages |
| Disk | 2 GB free | Build artifacts + logs + agent packages |
| RAM | 512 MB minimum | Per node instance |
| TLS certificate | Any valid cert | Self-signed acceptable for internal VPC deployments |

---

## Step 1 — Clone the Repository

Apollo is distributed via a private GitHub repository. SSH access must be provisioned before proceeding.

```bash
git clone --branch apollo-v1.2 --depth 1 git@github.com:elgrhy/apollo.git
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

The installer copies binaries to `/usr/local/bin/apollo-node/`, creates the `/usr/local/bin/apollo` global symlink, and runs `apollo doctor` to confirm a clean installation.

Alternatively, use the one-line network installer:

```bash
curl -fsSL https://get.apollo.systems | bash
```

---

## Step 5 — Generate TLS Certificate (Production)

For production deployments, generate or provision a TLS certificate. A self-signed cert is acceptable for internal VPC use:

```bash
openssl req -x509 -newkey rsa:4096 -keyout /etc/apollo/node.key \
  -out /etc/apollo/node.crt -days 365 -nodes \
  -subj "/CN=$(hostname -f)"
chmod 600 /etc/apollo/node.key
```

For local development, omit `--tls-cert` and `--tls-key` to run plain HTTP on port 8080.

---

## Step 6 — Start the Node

**Production (TLS on port 8443):**

```bash
apollo node start \
  --listen 0.0.0.0:8443 \
  --tls-cert /etc/apollo/node.crt \
  --tls-key  /etc/apollo/node.key \
  --secret-keys "$(openssl rand -hex 32)" \
  --jwt-secret  "$(openssl rand -hex 32)" \
  --webhook-url https://control.example.com/apollo-events \
  --region us-east-1
```

**Development (plain HTTP on port 8080):**

```bash
apollo node start --secret-keys "dev-key-1"
```

Store `--secret-keys` and `--jwt-secret` values in `/etc/apollo/env`. Never hardcode them in shell scripts.

---

## Step 7 — Start the Hub

```bash
apollo-hub start \
  --listen 0.0.0.0:9191 \
  --storage /var/lib/apollo/.apollo/hub_nodes.json \
  --webhook-url https://control.example.com/apollo-scale \
  --scale-threshold 0.80
```

Register this node with the hub:

```bash
apollo-hub add \
  --ip "$(hostname -f):8443" \
  --key "your-node-secret-key" \
  --name "$(hostname -s)" \
  --region us-east-1 \
  --storage /var/lib/apollo/.apollo/hub_nodes.json
```

---

## Step 8 — Verify Installation

```bash
apollo doctor
```

All checks must return `OK`. A `PRODUCTION READY` result confirms that the process sandbox, event spine, and state persistence modules are fully operational. Run this after every deployment, upgrade, or configuration change.

---

## Step 9 — Register and Run Your First Agent

```bash
# Register from a local directory
apollo agent --base-dir /var/lib/apollo add ./examples/openclaw

# Or register from a git repository
apollo agent --base-dir /var/lib/apollo add https://github.com/org/agent.git

# Store secrets for a tenant
curl -X PUT https://localhost:8443/tenants/demo-user/secrets \
  -H "X-Apollo-Key: your-key" -H "Content-Type: application/json" \
  -d '{"secrets": {"OPENAI_KEY": "sk-..."}}'

# Start the agent for a tenant
curl -X POST https://localhost:8443/agents/run \
  -H "X-Apollo-Key: your-key" \
  -d '{"agent":"openclaw","tenant":"demo-user"}'

# Check capacity
curl -H "X-Apollo-Key: your-key" https://localhost:8443/metrics
```

---

## Quick Reference

| Command | Purpose |
|---------|---------|
| `apollo node start` | Start the node daemon |
| `apollo doctor` | Validate installation |
| `apollo agent --base-dir DIR add <source>` | Register an agent (local, URL, or git) |
| `apollo agent --base-dir DIR run <name> --tenant <id>` | Launch an agent for a tenant |
| `apollo agent --base-dir DIR rollback <name>` | Restore previous agent version |
| `GET /health` | Liveness check |
| `GET /metrics` | Node capacity and region |
| `GET /agents/list` | All registered agents |
| `GET /usage/:id` | Billing usage for a tenant |
| `POST /usage/:id/reset` | Reset usage at billing cycle |
| `GET http://hub:9191/summary` | Fleet-wide overview |
| `GET http://hub:9191/nodes/best?region=X` | Least-loaded node in region |
| `GET http://hub:9191/regions` | Per-region capacity |

---

*For systemd production setup, see [Production Deployment Guide](production_deployment.md).*
*For firewall and TLS configuration, see [Network & Security Guide](network_security.md).*
