# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Run

```bash
# Build all crates (release)
cargo build --release

# Build debug (faster)
cargo build

# Validate installation
./target/release/apollo doctor

# Run the node daemon (HTTP on :8080)
./target/release/apollo node start --secret-keys "your-key"

# Run the node daemon with TLS (HTTPS on :8443)
./target/release/apollo node start \
  --listen 0.0.0.0:8443 \
  --tls-cert /etc/apollo/node.crt \
  --tls-key  /etc/apollo/node.key \
  --secret-keys "your-key" \
  --jwt-secret "your-jwt-signing-secret" \
  --webhook-url https://control.example.com/apollo-events \
  --region us-east-1

# Run the hub coordinator (HTTP on :9191)
./target/release/apollo-hub start \
  --webhook-url https://control.example.com/apollo-scale \
  --scale-threshold 0.80

# Check / lint / test
cargo check
cargo clippy
cargo test
```

## Agent Lifecycle (CLI)

```bash
# Register from local path, HTTPS archive/zip, or git URL
./target/release/apollo agent --base-dir .apollo add ./examples/openclaw
./target/release/apollo agent --base-dir .apollo add https://example.com/agent-1.0.tar.gz
./target/release/apollo agent --base-dir .apollo add https://github.com/org/agent.git

# Start agent for a tenant
./target/release/apollo agent --base-dir .apollo run openclaw --tenant alice

# Stop, rollback, remove
./target/release/apollo agent --base-dir .apollo stop openclaw --tenant alice
./target/release/apollo agent --base-dir .apollo rollback openclaw
./target/release/apollo agent --base-dir .apollo remove openclaw
```

Note: `--base-dir` must come before the subcommand, not after.

## REST API (node running)

All requests require `X-Apollo-Key: <secret>` OR `Authorization: Bearer <HS256-JWT>`.

```bash
# Node capacity + identity + region
curl -H "X-Apollo-Key: KEY" http://localhost:8080/metrics

# List registered agents
curl -H "X-Apollo-Key: KEY" http://localhost:8080/agents/list

# Register agent package
curl -X POST http://localhost:8080/agents/add \
  -H "X-Apollo-Key: KEY" -H "Content-Type: application/json" \
  -d '{"source": "/abs/path/or/URL/or/git"}'

# Run/stop agent
curl -X POST   http://localhost:8080/agents/run  -H "X-Apollo-Key: KEY" -d '{"agent":"openclaw","tenant":"user_123"}'
curl -X DELETE http://localhost:8080/agents/stop -H "X-Apollo-Key: KEY" -d '{"agent":"openclaw","tenant":"user_123"}'

# Rollback / remove agent
curl -X POST http://localhost:8080/agents/rollback -H "X-Apollo-Key: KEY" -d '{"agent":"openclaw"}'
curl -X POST http://localhost:8080/agents/remove   -H "X-Apollo-Key: KEY" -d '{"agent":"openclaw"}'

# Per-tenant secrets (stored mode 0600, injected at spawn)
curl -X PUT    http://localhost:8080/tenants/user_123/secrets \
  -H "X-Apollo-Key: KEY" -H "Content-Type: application/json" \
  -d '{"secrets": {"OPENAI_KEY": "sk-...", "TELEGRAM_TOKEN": "bot:..."}}'
curl -X DELETE http://localhost:8080/tenants/user_123/secrets -H "X-Apollo-Key: KEY"

# Usage metering
curl -H "X-Apollo-Key: KEY" http://localhost:8080/usage                   # all tenants
curl -H "X-Apollo-Key: KEY" http://localhost:8080/usage/user_123          # one tenant
curl -X POST http://localhost:8080/usage/user_123/reset -H "X-Apollo-Key: KEY"  # billing reset

# Health
curl -H "X-Apollo-Key: KEY" http://localhost:8080/health
```

## Hub API (hub running, no auth required)

```bash
curl http://localhost:9191/summary              # fleet overview
curl http://localhost:9191/nodes/status         # per-node health + agent counts
curl "http://localhost:9191/nodes/best?region=us-east-1"  # least-loaded node in region
curl http://localhost:9191/catalog              # aggregated agent catalog across all nodes
curl http://localhost:9191/regions              # per-region capacity breakdown
```

Hub polls each node's `/metrics` every 10 s. Catalog refreshes every 5th tick (~50 s) via `/agents/list`. A 50 ms delay between requests avoids the node's per-key rate limiter. Auto-scale webhook fires when fleet utilization exceeds `--scale-threshold` (default 0.80), re-arms at 70%.

## Architecture

Four Rust crates in a Cargo workspace:

- **`apollo-core`** — shared primitives: `types.rs`, `agents.rs` (registry CRUD + URL/git sourcing + versioning), `detect.rs` (node capability detection), `fetch.rs` (HTTP archive + git clone), `runtime_registry.rs` (launch dispatch + runtime auto-install + sharded instance paths), `secrets.rs` (per-tenant secret storage), `usage.rs` (metering accumulation), `webhook.rs` (outbound lifecycle events). No binary.
- **`apollo-runtime`** — `AgentRuntime` trait + `ProcessRuntime`. Cross-platform spawning: Unix (`setpgid`/`killpg` via `nix`), Windows (`CREATE_NEW_PROCESS_GROUP`/`taskkill /F /T`). Injects secrets + volume env vars at spawn. Sharded instance storage (`instances/{tenant_id}.json`). Port = `10000 + (hash % 55535)`.
- **`apollo-node`** — binary `apollo`. Axum HTTP/HTTPS server (replaces tiny_http). Auth middleware: X-Apollo-Key OR Bearer JWT (HS256). Routes: agents, secrets, usage, health, metrics. Background metering loop samples CPU/memory every 60 s.
- **`apollo-hub`** — binary `apollo-hub`. Axum server. Background poller fetches `/metrics` + `/agents/list`. Region-aware `/nodes/best`. `/regions` endpoint. Auto-scale webhook at configurable threshold.

## Key Data Flows

**Agent Registration** (local/URL/git):
1. `resolve_agent_source` → local copy, HTTP archive download, or `git clone --depth 1`
2. Parse `agent.yaml` → `AgentSpec`
3. `detect_node_capabilities()` — OS/arch/runtime compatibility check
4. `ensure_runtime` — check PATH → local store → auto-download from `runtime.install` URL
5. Backup previous version to `agents/{name}.v{old_version}/`; copy new files to `agents/{name}/`
6. SHA-256 yaml → `checksum`; upsert `AgentRecord` with `prev_version`

**Agent Start**:
1. Look up `AgentSpec` from `agents.json`
2. `resolve_launch` — dispatch table or `{entry}` command template
3. Create `tenants/{tenant_id}/{agent_name}/`; create volume dirs
4. Load `secrets/{tenant_id}.json` → merge into process env
5. Inject `APOLLO_VOLUME_{NAME}` for each declared volume
6. Spawn with `PYTHONUNBUFFERED=1`, scrubbed env, process group isolation
7. Background resource monitor (CPU/memory kill via `sysinfo`)
8. `record_start(base_dir, tenant_id)` → usage metering
9. Fire `AGENT_START` webhook if configured

## Agent Package Format

```yaml
name: openclaw
version: 1.0.0
runtime:
  type: python3          # any: python3, node, go, deno, bun, ruby, php, perl, java, dotnet, gx, shell, rust, or custom
  entry: main.py
  command: "gx run {entry}"   # optional: override launch command
  install:
    linux:   https://example.com/runtime-linux
    macos:   https://example.com/runtime-macos
    windows: https://example.com/runtime-windows.exe
llm:
  required: true
  provider: any
  fallback: true
resources:
  cpu: 0.5
  memory: 512mb
  timeout: 120
permissions:
  network: full
  filesystem: sandbox
  processes: restricted
compatibility:
  os: [linux, darwin, windows]
  arch: [x86_64, aarch64]
restart_policy:
  max_restarts: 3
  window_secs: 60
volumes:
  - name: data       # creates volumes/{tenant}/{agent}/data/, injects APOLLO_VOLUME_DATA
    size: 1gb
```

## State Files

All state lives under `base_dir` (default `.apollo/`):

| Path | Contents |
|------|----------|
| `agents.json` | Registered agent records (specs + checksums + prev_version) |
| `instances/{tenant_id}.json` | Running instances sharded by tenant |
| `secrets/{tenant_id}.json` | Per-tenant env secrets (mode 0600 on Unix) |
| `usage/{tenant_id}.json` | Accumulated CPU-seconds, memory-GB-seconds, starts/stops |
| `agents/{name}/` | Copied agent package (global store) |
| `agents/{name}.v{ver}/` | Version backup for rollback |
| `runtimes/{kind}/` | Auto-installed runtimes |
| `tenants/{id}/{name}/` | Per-tenant isolated workspace |
| `volumes/{id}/{name}/{vol}/` | Persistent volume mounts per tenant+agent+volume |
| `logs/{id}/{name}.log` | Agent stdout/stderr; rotated at 10 MB |
| `events.jsonl` | Append-only audit log |

## Security Model

- **Auth**: `X-Apollo-Key` (multiple comma-separated keys for rotation) OR `Authorization: Bearer <HS256-JWT>` validated with `--jwt-secret`
- **Rate limiting**: per-key token bucket, 100 RPS default; hub adds 50 ms gap to avoid self-limiting
- **Secrets**: stored `0600`, loaded only at spawn, never logged
- **Env scrubbing**: `cmd.env_clear()` before spawn; only Apollo vars + tenant secrets + sanitized PATH
- **Process containment**: Unix = `setpgid`/`killpg`; Windows = `CREATE_NEW_PROCESS_GROUP`/`taskkill /F /T`
- **FS isolation**: `harden_path()` canonicalizes + `starts_with(root)` check before exec
- **Audit trail**: `events.jsonl` records all starts, stops, recoveries

## Provider Integration

1. Register agents once: `POST /agents/add {"source": "<URL or git>"}` — Apollo fetches, validates, auto-installs runtime if needed
2. Store per-tenant secrets: `PUT /tenants/{id}/secrets {"secrets": {"OPENAI_KEY": "..."}}`
3. Start agent per user: `POST /agents/run {"agent": "openclaw", "tenant": "<user_id>"}` — isolated workspace, deterministic port, secrets injected
4. Track usage for billing: `GET /usage/{id}` — CPU-seconds, memory-GB-seconds, starts/stops
5. Reset at billing cycle: `POST /usage/{id}/reset`

## Deferred Roadmap (build next)

- **K8s operator / Helm chart** — `deploy/helm/` for enterprise Kubernetes deployments
- **Dashboard UI** — web interface for fleet health, per-tenant usage, agent catalog, live logs
- **Python / Node / Go SDK** — thin client wrappers for `apollo.run_agent(tenant, agent)`
