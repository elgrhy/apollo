# APOLLO v1.0

**Production-grade distributed infrastructure runtime for autonomous AI agents.**

APOLLO is a self-hosted execution engine and fleet coordination layer that gives infrastructure providers, IT teams, and SaaS platforms a secure, observable, and fault-tolerant foundation for running agent workloads at scale. It is designed to be deployed once and operated indefinitely without developer involvement.

**Status:** Production Certified — v1.0 Frozen Release

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      Provider Control Plane                     │
│                  (your dashboard / billing system)              │
└────────────────────────────┬────────────────────────────────────┘
                             │ REST  (internal VPC only)
                ┌────────────▼────────────────┐
                │       Apollo Hub :9090       │
                │    Fleet Coordination Layer  │
                │  Node registry + health poll │
                └───┬────────────┬────────────┘
                    │            │
        ┌───────────▼──┐   ┌─────▼────────────┐
        │  Node :8080  │   │   Node :8080     │   ...
        │  Execution   │   │   Execution      │
        │  Engine      │   │   Engine         │
        └──────┬───────┘   └──────────────────┘
               │
   ┌───────────┼───────────┐
   │           │           │
┌──▼──┐    ┌───▼──┐    ┌───▼──┐
│ T-A │    │ T-B  │    │ T-C  │   Isolated tenant workspaces
│Agent│    │Agent │    │Agent │
└─────┘    └──────┘    └──────┘
```

| Component | Binary | Default Port | Role |
|:---|:---|:---|:---|
| Apollo Node | `apollo` | `0.0.0.0:8080` | Execution engine. Runs, isolates, and monitors agent processes. |
| Apollo Hub | `apollo-hub` | `0.0.0.0:9090` | Coordination layer. Tracks fleet health and routes new agents to available nodes. |
| Apollo CLI | `apollo` | — | Interactive shell and operator interface. |
| Apollo Doctor | `apollo doctor` | — | Acceptance validator. Certifies installation health. |

---

## Key Features

**Multi-Tenant Execution**
Each agent runs in a fully isolated workspace under `.apollo/tenants/{tenant_id}/{agent_name}/`. Path canonicalization prevents workspace escapes. No tenant can read or write another tenant's files.

**Process Group Containment**
Agent processes are placed in dedicated Unix process groups (`setpgid`). Termination signals (`killpg`) reach every subprocess in the group, eliminating orphan processes even after hard crashes.

**Deterministic Causal Traceability**
Every agent lifecycle event is written to an append-only event spine at `.apollo/events.jsonl` with correlation IDs linking each action to its orchestrator. The audit log survives restarts intact and is suitable for compliance review.

**Autonomous Restart Recovery**
On every startup, the node performs a pre-flight orphan sweep: identifies all process groups from the prior session and terminates them before the API accepts new requests. No manual cleanup is ever required after a crash.

**Fleet Coordination**
The hub polls every registered node's `/metrics` endpoint on a 30-second cycle. Nodes that miss two consecutive polls are marked `OFFLINE`. Nodes that are running but unreachable by the hub enter `DEGRADED` state and continue executing existing agents independently.

**Hardened Security**
- Environment scrubbing: agent processes inherit a fully sanitized environment with no host secrets
- Internal network blocking: agents cannot reach RFC 1918 private address ranges
- Rate limiting: per-node RPS limits on the management API
- `X-Apollo-Key` authentication on all REST endpoints
- Multiple concurrent keys supported for zero-downtime rotation

**SHA-256 Binary Integrity**
Every release ships with a `CHECKSUMS.sha256` manifest. The installer verifies both binaries before proceeding.

**Production Certification**
`apollo doctor` validates the process sandbox, event spine, and state persistence layers. A `PRODUCTION READY` result is the acceptance standard for every deployment.

---

## Installation

### One-Line Installer

```bash
curl -fsSL https://get.apollo.systems | bash
```

Requires: Rust/Cargo 1.75+, Git 2.30+.

### Manual Install (Air-Gapped / Secure Environments)

```bash
git clone --branch apollo-v1.0 --depth 1 git@github.com:elgrhy/apollo.git
cd apollo
cargo build --release
shasum -a 256 -c CHECKSUMS.sha256   # both lines must return OK
./install.sh
```

Binaries are installed to `/usr/local/bin/apollo-node/`. A global `apollo` symlink is created at `/usr/local/bin/apollo`.

### Verify

```bash
apollo doctor
```

All checks must return `OK`. `PRODUCTION READY` confirms a certified installation.

---

## Quick Start

```bash
# Start a node (default: 0.0.0.0:8080)
apollo node start --secret-keys "your-secret-key"

# Start the hub (default: 0.0.0.0:9090)
apollo-hub start

# Run an agent
apollo agent run openclaw --tenant demo-user

# List running agents
apollo agent list

# Check fleet status
curl -H "X-Apollo-Key: your-secret-key" http://localhost:8080/metrics
```

---

## REST API

All endpoints require the `X-Apollo-Key` header.

### Node Endpoints (`:8080`)

| Method | Endpoint | Description |
|:---|:---|:---|
| `GET` | `/metrics` | Node capacity and active agent count |
| `POST` | `/agents/add` | Register an agent package (copies to global store, generates SHA-256) |
| `POST` | `/agents/run` | Start an agent for a tenant |
| `DELETE` | `/agents/stop` | Stop an agent and release all resources |

**GET /metrics**
```json
{"active_agents": 12, "max_agents": 50}
```

**POST /agents/run**
```json
// Request
{"agent": "openclaw", "tenant": "user_123"}

// Response
{"id": "openclaw-1234", "pid": 5678, "port": 10234}
```

**POST /agents/add**
```json
{"source": "/absolute/path/to/agent"}
```

**DELETE /agents/stop**
```json
{"agent": "openclaw", "tenant": "user_123"}
```

### Hub Endpoints (`:9090`)

| Method | Endpoint | Description |
|:---|:---|:---|
| `POST` | `/nodes/add` | Register a node with the hub |
| `GET` | `/nodes/status` | Fleet-wide node status |
| `GET` | `/nodes/best` | Returns the node with most available capacity |

---

## Directory Structure

```
/usr/local/bin/apollo-node/
├── apollo          # Node daemon + CLI binary
└── apollo-hub      # Hub coordinator binary

/var/lib/apollo/
└── .apollo/
    ├── events.jsonl              # Append-only audit log
    ├── hub_nodes.json            # Hub node registry (persisted)
    └── tenants/
        └── {tenant_id}/
            └── {agent_name}/    # Isolated agent workspace
```

---

## Security Model

| Control | Enforcement |
|:---|:---|
| Tenant filesystem isolation | Path canonicalization; each tenant confined to its own workspace |
| Process group containment | `setpgid` on launch; `killpg` on stop/crash |
| Environment sanitization | Agent processes receive a fully scrubbed environment |
| Internal network blocking | Agents cannot initiate connections to RFC 1918 ranges |
| API authentication | `X-Apollo-Key` header required on all management endpoints |
| Key rotation | Multiple active keys supported simultaneously |
| Binary integrity | SHA-256 verified before installation |
| Audit trail | Append-only `events.jsonl`; every lifecycle transition logged |

The node API (port 8080) and hub API (port 9090) are designed for **internal network use only**. Neither service should be exposed to the public internet. Place a TLS-terminating reverse proxy in front of either service if cross-datacenter communication is required.

---

## SLA Summary

| Metric | Target |
|:---|:---|
| Node process availability | 99.5% per calendar month |
| Automatic restart after crash | < 10 seconds |
| Node API response time (p99) | < 200 ms |
| Agent startup latency | < 2 seconds |
| Agent stop latency | < 1 second |
| Orphan cleanup on restart | < 30 seconds, fully automatic |
| Hub failure detection | ≤ 60 seconds |
| Max agent density per node | 50 (configurable) |

---

## Production Certification

APOLLO v1.0 is validated under the following certification modules:

| Module | Name | Coverage |
|:---|:---|:---|
| HP-CERT | Hardware Pressure | CPU/memory saturation, I/O stress |
| DSI-CERT | Distributed State Integrity | Restart storms, state recovery, orphan cleanup |
| NET-CERT | Network Resilience | Partition tolerance, degraded mode, hub failover |

---

## Systemd Services

**apollo-node.service** — recommended configuration:

```ini
[Service]
ExecStart=/usr/local/bin/apollo-node/apollo node start \
    --listen 0.0.0.0:8080 \
    --base-dir /var/lib/apollo \
    --max-agents 50 \
    --secret-keys "YOUR_KEY"
Restart=on-failure
RestartSec=5s
User=apollo
```

**apollo-hub.service** — recommended configuration:

```ini
[Service]
ExecStart=/usr/local/bin/apollo-node/apollo-hub start \
    --listen 0.0.0.0:9090 \
    --storage /var/lib/apollo/.apollo/hub_nodes.json
Restart=on-failure
RestartSec=5s
User=apollo
```

Full unit files and service account setup are in the [Production Deployment Guide](docs/production_deployment.md).

---

## Documentation

| Document | Purpose |
|:---|:---|
| [Enterprise Handoff Pack](docs/HANDOFF.md) | Master index for all operator documentation |
| [Quick Start Guide](docs/quick_start.md) | Step-by-step installation and first run |
| [Production Deployment Guide](docs/production_deployment.md) | systemd, directory structure, log locations |
| [Network & Security Guide](docs/network_security.md) | Ports, firewall rules, key management |
| [SLA](docs/sla.md) | Full availability, recovery, and performance guarantees |
| [Pilot Feedback Framework](docs/pilot_feedback_framework.md) | Structured feedback and incident reporting |
| [Patch Strategy](docs/patch_strategy.md) | v1.0.x release rules and upgrade procedure |
| [Enterprise Approval Pack](enterprise_approval_pack.md) | SOC2 summary, FMEA, compliance checklist |

---

## Requirements

| | Minimum |
|:---|:---|
| OS | Linux x86_64 (Ubuntu 20.04+, RHEL 8+) |
| Rust | 1.75+ |
| Git | 2.30+ |
| RAM | 512 MB per node |
| Disk | 2 GB (build artifacts + logs) |
| Network | Internal VPC; no public internet required at runtime |

---

*APOLLO v1.0 is a frozen production release. Architecture, API contracts, and the audit log schema are stable. Bug fixes are issued as v1.0.x patches. See [Patch Strategy](docs/patch_strategy.md) for scope rules.*
