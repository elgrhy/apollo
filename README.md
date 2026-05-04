# APOLLO v1.2

**Production-grade distributed infrastructure runtime for autonomous AI agents.**

Apollo is a self-hosted execution engine and fleet coordination layer that gives infrastructure providers, IT teams, and SaaS platforms a secure, observable, billing-aware foundation for running agent workloads at scale. Deploy once, operate indefinitely — no developer involvement required.

**Status:** Production Certified — v1.2

---

## Architecture

```
┌────────────────────────────────────────────────────────────────────┐
│                       Provider Control Plane                       │
│                   (your dashboard / billing system)                │
└──────────────────────────────┬─────────────────────────────────────┘
                               │ REST + webhooks (internal VPC)
              ┌────────────────▼─────────────────────┐
              │          Apollo Hub  :9191            │
              │  Region routing · Auto-scale alerts  │
              │  Agent catalog · Fleet health        │
              └──────┬───────────────┬───────────────┘
                     │ 10s poll      │ 10s poll
         ┌───────────▼──────┐  ┌─────▼────────────────┐
         │  Node :8443 (TLS)│  │  Node :8443 (TLS)    │  ...
         │  us-east-1       │  │  eu-west-1           │
         │  tenant_1 → agt  │  │  tenant_9001 → agt   │
         │  tenant_2 → agt  │  │  tenant_9002 → agt   │
         └──────────────────┘  └──────────────────────┘
```

| Component | Binary | Default Port | Role |
|-----------|--------|-------------|------|
| Apollo Node | `apollo` | `0.0.0.0:8080` (HTTP) / `8443` (TLS) | Execution engine. Runs, isolates, meters, and monitors agent processes. |
| Apollo Hub | `apollo-hub` | `0.0.0.0:9191` | Fleet coordinator. Region routing, catalog, auto-scale alerts. |
| Apollo CLI | `apollo` | — | Operator interface: agent registration, rollback, remove. |
| Apollo Doctor | `apollo doctor` | — | Acceptance validator. Certifies every deployment. |

---

## Key Features

**Multi-Tenant Execution**
Each agent runs in an isolated workspace under `.apollo/tenants/{tenant_id}/{agent_name}/`. Path canonicalization prevents workspace escapes. Per-tenant process groups ensure one tenant crash cannot affect any other.

**TLS / HTTPS (Native)**
Provide `--tls-cert` and `--tls-key` at startup — the node binds HTTPS via `rustls`. No reverse proxy required.

**JWT + Key Authentication**
All node endpoints require `X-Apollo-Key: <key>` OR `Authorization: Bearer <HS256-JWT>`. Multiple keys supported for zero-downtime rotation. JWT claims can carry scoped key bundles for sub-operator delegation.

**Per-Tenant Secret Injection**
Store tenant secrets via `PUT /tenants/{id}/secrets`. Secrets are written at mode `0600`, loaded only at agent spawn, injected into the process environment, and never appear in logs or API responses.

**Usage Metering**
A background loop samples CPU and memory every 60 s per running agent. Accumulated data (`cpu_seconds`, `memory_gb_seconds`, `total_starts`, `total_stops`) is queryable per tenant and reset-able at billing cycle boundaries.

**Persistent Volumes**
Declare `volumes:` in `agent.yaml`. Apollo creates `volumes/{tenant}/{agent}/{name}/` and injects `APOLLO_VOLUME_{NAME}` into the agent environment. Data persists across restarts, rollbacks, and upgrades.

**Outbound Webhook Events**
Every agent lifecycle event fires an HMAC-SHA256 signed HTTP POST: `AGENT_START`, `AGENT_STOP`, `CAPACITY_WARNING`. The hub fires `SCALE_NEEDED` when fleet utilization exceeds the configurable threshold. All events are retried 3× with exponential backoff.

**Multi-Region Fleet Routing**
Tag nodes with `--region`. The hub's `/nodes/best?region=X` returns the least-loaded node in a region. `/regions` provides per-region capacity breakdown.

**Agent Versioning + Rollback**
Re-registering an agent backs up the previous version to `agents/{name}.v{version}/`. `POST /agents/rollback` restores it in under a second.

**Runtime Auto-Provisioning**
If the required runtime isn't installed, `agent.yaml` can supply download URLs. Apollo downloads and installs it to `runtimes/{kind}/` on first registration — no manual setup on the node.

**Any Language**
Python, Node.js, Go, Deno, Bun, Ruby, PHP, Java, .NET, Rust, Shell, or any custom command via `runtime.command` template.

**Cross-Platform**
Linux, macOS, Windows. `aarch64` and `x86_64`.

---

## Installation

### One-Line Installer

```bash
curl -fsSL https://get.apollo.systems | bash
apollo doctor
```

Requires: Rust/Cargo 1.75+, Git 2.30+.

### Manual Install (Air-Gapped / Secure Environments)

```bash
git clone --branch apollo-v1.2 --depth 1 git@github.com:elgrhy/apollo.git
cd apollo
cargo build --release
shasum -a 256 -c CHECKSUMS.sha256   # both lines must return OK
./install.sh
apollo doctor
```

---

## Quick Start

```bash
# Start a node (plain HTTP for dev)
apollo node start --secret-keys "your-secret-key"

# Start a node (TLS for production)
apollo node start \
  --listen 0.0.0.0:8443 \
  --tls-cert /etc/apollo/node.crt \
  --tls-key  /etc/apollo/node.key \
  --secret-keys "key-1,key-2" \
  --jwt-secret "your-jwt-signing-secret" \
  --webhook-url https://control.example.com/apollo-events \
  --region us-east-1

# Start the hub
apollo-hub start \
  --webhook-url https://control.example.com/apollo-scale \
  --scale-threshold 0.80

# Register an agent (local, URL, or git)
apollo agent --base-dir .apollo add ./examples/openclaw
apollo agent --base-dir .apollo add https://github.com/org/agent.git

# Start an agent for a tenant
apollo agent --base-dir .apollo run openclaw --tenant user_123

# Check capacity
curl -H "X-Apollo-Key: your-secret-key" http://localhost:8080/metrics
```

---

## REST API

All node endpoints require `X-Apollo-Key: <key>` OR `Authorization: Bearer <JWT>`.

### Node Endpoints (`:8080` / `:8443`)

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/health` | Liveness check |
| `GET` | `/metrics` | Node capacity, region, active agent count |
| `GET` | `/agents/list` | All registered agents with specs and checksums |
| `POST` | `/agents/add` | Register agent from local path, URL, or git |
| `POST` | `/agents/run` | Start an agent for a tenant |
| `DELETE` | `/agents/stop` | Stop an agent and release resources |
| `POST` | `/agents/rollback` | Restore previous agent version |
| `POST` | `/agents/remove` | Permanently remove a registered agent |
| `PUT` | `/tenants/:id/secrets` | Store per-tenant secrets (written 0600) |
| `DELETE` | `/tenants/:id/secrets` | Delete all secrets for a tenant |
| `GET` | `/usage` | Accumulated usage for all tenants |
| `GET` | `/usage/:id` | Accumulated usage for one tenant |
| `POST` | `/usage/:id/reset` | Reset usage counters at billing cycle |

**Key examples:**

```bash
# Node capacity
curl -H "X-Apollo-Key: KEY" http://localhost:8080/metrics
# {"active_agents":14,"max_agents":200,"node_id":"us-east-1-node-01","region":"us-east-1"}

# Run an agent
curl -X POST http://localhost:8080/agents/run \
  -H "X-Apollo-Key: KEY" \
  -d '{"agent":"openclaw","tenant":"user_123"}'
# {"status":"started","port":34201,"pid":78432}

# Store tenant secrets
curl -X PUT http://localhost:8080/tenants/user_123/secrets \
  -H "X-Apollo-Key: KEY" -H "Content-Type: application/json" \
  -d '{"secrets":{"OPENAI_KEY":"sk-...","TELEGRAM_TOKEN":"bot:..."}}'

# Get billing usage
curl -H "X-Apollo-Key: KEY" http://localhost:8080/usage/user_123
# {"tenant_id":"user_123","cpu_seconds":14.3,"memory_gb_seconds":0.87,"total_starts":3,...}

# Reset at billing cycle
curl -X POST http://localhost:8080/usage/user_123/reset -H "X-Apollo-Key: KEY"
```

### Hub Endpoints (`:9191`) — No Auth Required (internal network only)

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/summary` | Fleet-wide counts: nodes, agents, capacity, catalog size |
| `GET` | `/nodes/status` | Per-node health, active agents, region |
| `GET` | `/nodes/best` | Least-loaded node (optional `?region=X` filter) |
| `GET` | `/catalog` | Aggregated agent catalog across all nodes |
| `GET` | `/regions` | Per-region capacity breakdown |

```bash
curl http://localhost:9191/summary
curl "http://localhost:9191/nodes/best?region=eu-west-1"
curl http://localhost:9191/regions
```

---

## Directory Structure

```
/var/lib/apollo/.apollo/
├── events.jsonl                    # Audit log (append-only)
├── hub_nodes.json                  # Hub node registry
├── agents.json                     # Registered agent catalog
├── agents/{name}/                  # Agent package store
├── agents/{name}.v{ver}/           # Version backup for rollback
├── instances/{tenant_id}.json      # Running instances (sharded by tenant)
├── secrets/{tenant_id}.json        # Per-tenant secrets (mode 0600)
├── usage/{tenant_id}.json          # Billing metering data
├── volumes/{id}/{agent}/{vol}/     # Persistent volume mounts
├── runtimes/{kind}/                # Auto-installed runtimes
├── logs/{id}/{name}.log            # Agent stdout/stderr (rotated 10 MB)
└── tenants/{id}/{name}/            # Per-tenant isolated workspaces
```

---

## Security Model

| Control | Enforcement |
|---------|-------------|
| Tenant filesystem isolation | Path canonicalization; each tenant confined to its own workspace |
| Process group containment | `setpgid` on launch; `killpg` on stop/crash — entire subtree killed |
| Environment sanitization | `env_clear()` before spawn; only Apollo vars + tenant secrets + sanitized PATH |
| Secrets protection | Mode `0600` storage; loaded only at spawn; excluded from all API responses |
| API authentication | `X-Apollo-Key` (multi-key, rotation-safe) OR `Authorization: Bearer` HS256-JWT |
| JWT scoping | `keys` claim in JWT allows scoped tokens per sub-operator |
| Rate limiting | Per-key token bucket, 100 RPS; `429` on breach |
| Transport | TLS via `rustls`; native — no reverse proxy required |
| Webhook integrity | HMAC-SHA256 `X-Apollo-Signature` on every outbound event |
| Audit trail | Append-only `events.jsonl`; every lifecycle transition logged |
| Binary integrity | SHA-256 manifest verified at install; `apollo doctor` validates on every deployment |

---

## Systemd Services

**apollo-node.service** (production configuration):

```ini
[Unit]
Description=APOLLO Node — Agent Execution Engine v1.2
After=network.target

[Service]
Type=simple
User=apollo
Group=apollo
WorkingDirectory=/var/lib/apollo
EnvironmentFile=/etc/apollo/env
ExecStart=/usr/local/bin/apollo-node/apollo node start \
    --listen 0.0.0.0:8443 \
    --tls-cert /etc/apollo/node.crt \
    --tls-key  /etc/apollo/node.key \
    --base-dir /var/lib/apollo \
    --max-agents 200 \
    --secret-keys "${APOLLO_SECRET_KEYS}" \
    --jwt-secret  "${APOLLO_JWT_SECRET}" \
    --webhook-url "${APOLLO_WEBHOOK_URL}" \
    --region      "${APOLLO_REGION}"
Restart=on-failure
RestartSec=5s
NoNewPrivileges=true
ProtectSystem=strict
ReadWritePaths=/var/lib/apollo /etc/apollo

[Install]
WantedBy=multi-user.target
```

**apollo-hub.service** (production configuration):

```ini
[Unit]
Description=APOLLO Hub — Fleet Coordination Layer v1.2
After=network.target

[Service]
Type=simple
User=apollo
Group=apollo
WorkingDirectory=/var/lib/apollo
EnvironmentFile=/etc/apollo/env
ExecStart=/usr/local/bin/apollo-node/apollo-hub start \
    --listen 0.0.0.0:9191 \
    --storage /var/lib/apollo/.apollo/hub_nodes.json \
    --webhook-url "${APOLLO_SCALE_WEBHOOK_URL}" \
    --scale-threshold 0.80
Restart=on-failure
RestartSec=5s
NoNewPrivileges=true
ProtectSystem=strict
ReadWritePaths=/var/lib/apollo

[Install]
WantedBy=multi-user.target
```

Full unit files and service account setup: [Production Deployment Guide](docs/production_deployment.md).

---

## SLA Summary

| Metric | Target |
|--------|--------|
| Node process availability | 99.5% per calendar month |
| Automatic restart after crash | < 10 seconds |
| Node API response time (p99) | < 200 ms |
| Agent startup latency | < 2 seconds |
| Agent stop latency | < 1 second |
| Orphan cleanup on restart | < 30 seconds, fully automatic |
| Hub health poll interval | 10 seconds |
| Hub failure detection | ≤ 20 seconds (2 missed polls) |
| Catalog refresh interval | ~50 seconds (every 5th poll tick) |
| Max agent density per node | 200 (configurable via `--max-agents`) |

---

## Documentation

| Document | Purpose |
|----------|---------|
| [Enterprise Handoff Pack](docs/HANDOFF.md) | Master index for all operator documentation |
| [Quick Start Guide](docs/quick_start.md) | Step-by-step installation and first run |
| [Production Deployment Guide](docs/production_deployment.md) | systemd, directory layout, env file, log locations |
| [Network & Security Guide](docs/network_security.md) | Ports, firewall rules, TLS, JWT, key management |
| [SLA](docs/sla.md) | Availability, recovery, and performance guarantees |
| [Pilot Feedback Framework](docs/pilot_feedback_framework.md) | Structured feedback and incident reporting |
| [Patch Strategy](docs/patch_strategy.md) | v1.2.x release rules and upgrade procedure |
| [Enterprise Approval Pack](enterprise_approval_pack.md) | SOC2 summary, FMEA, compliance checklist |
| [Scenarios](Scenarios.md) | Hostinger user flow, IT team setup, Cloud Run problem |

---

## Requirements

| | Minimum |
|-|---------|
| OS | Linux x86_64/aarch64 (Ubuntu 20.04+, RHEL 8+), macOS, Windows |
| Rust | 1.75+ |
| Git | 2.30+ |
| RAM | 512 MB per node |
| Disk | 2 GB (build artifacts + logs) |
| Network | Internal VPC; no public internet required at runtime |

---

*Apollo v1.2 — TLS · JWT · Billing · Secrets · Webhooks · Multi-region · Any language · Any platform.*
