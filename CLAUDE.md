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

# Run the node daemon (API on :8080)
./target/release/apollo node start --secret-keys "your-key"

# Run the hub coordinator (API on :9191)
./target/release/apollo-hub start

# Check / lint
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

All requests require `X-Apollo-Key: <secret>` header.

```bash
# Node capacity + identity
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

# Rollback to previous version
curl -X POST http://localhost:8080/agents/rollback -H "X-Apollo-Key: KEY" -d '{"agent":"openclaw"}'

# Health
curl -H "X-Apollo-Key: KEY" http://localhost:8080/health
```

## Hub API (hub running)

No auth required on hub endpoints.

```bash
curl http://localhost:9191/summary          # fleet overview
curl http://localhost:9191/nodes/status     # per-node health + agent counts
curl http://localhost:9191/nodes/best       # least-loaded available node
curl http://localhost:9191/catalog          # aggregated agent catalog across all nodes
```

Hub polls each node's `/metrics` every 10 s. Catalog is refreshed every 5th tick (~50 s) via `/agents/list`. A 50 ms delay between the two requests avoids the node's per-key rate limiter.

## Architecture

Four Rust crates in a Cargo workspace:

- **`apollo-core`** — shared primitives: `types.rs` (all structs), `agents.rs` (registry CRUD, URL/git sourcing, versioning, rollback), `detect.rs` (node capability detection), `fetch.rs` (HTTP archive + git clone), `runtime_registry.rs` (launch dispatch, runtime auto-install, sharded instance paths). No binary.
- **`apollo-runtime`** — `AgentRuntime` trait + `ProcessRuntime`. Cross-platform process spawning: Unix (`setpgid`/`killpg` via `nix`), Windows (`CREATE_NEW_PROCESS_GROUP`/`taskkill /F /T`). Sharded instance storage (`instances/{tenant_id}.json`). Port = `10000 + (hash % 55535)`.
- **`apollo-node`** — binary `apollo`. Clap CLI with `node start | status`, `agent add | run | stop | list | rollback | remove`, and `doctor`. Runs `tiny_http` API server for REST management. Orphan recovery on startup.
- **`apollo-hub`** — binary `apollo-hub`. Clap CLI with `start | add | list`. Tiny_http server wrapped in `tokio::task::spawn_blocking`. Background poller fetches `/metrics` + `/agents/list` from each node.

## Key Data Flows

**Agent Registration** (local/URL/git):
1. `resolve_agent_source(source, staging_dir)` — local copy, HTTP archive download, or `git clone --depth 1`
2. Parse `agent.yaml` → `AgentSpec`
3. `detect_node_capabilities()` — OS/arch/runtime compatibility check
4. `ensure_runtime(runtime, runtimes_dir)` — check PATH → local store → auto-download from `runtime.install` URL
5. Backup existing version to `agents/{name}.v{old_version}/`; copy new files to `agents/{name}/`
6. SHA-256 the yaml content → `checksum`; upsert `AgentRecord` with `prev_version`

**Agent Start**:
1. Look up `AgentSpec` from `agents.json`
2. `resolve_launch(runtime, entry_path, runtimes_dir)` — dispatch table or `{entry}` command template
3. Create tenant workspace: `tenants/{tenant_id}/{agent_name}/`
4. Spawn with `PYTHONUNBUFFERED=1`, scrubbed env, process group isolation
5. Background resource monitor (CPU/memory kill via `sysinfo`)

## Agent Package Format

```yaml
name: openclaw
version: 1.0.0
runtime:
  type: python3          # any: python3, node, go, deno, bun, ruby, php, perl, java, dotnet, gx, shell, rust, or custom
  entry: main.py
  command: "gx run {entry}"   # optional: override launch command template
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
```

Runtime dispatch: built-in table covers python3/node/go/deno/bun/ruby/php/perl/java/dotnet/gx/rust/shell/powershell. Unknown types execute the `command` template or run the entry as a native binary. `{entry}` in `command` is replaced with the absolute entry file path.

## State Files

All state lives under `base_dir` (default `.apollo/`):

| File/Dir | Contents |
|----------|----------|
| `agents.json` | Registered agent records (specs + checksums + prev_version) |
| `instances/{tenant_id}.json` | Running instances sharded by tenant |
| `agents/{name}/` | Copied agent package (global store) |
| `agents/{name}.v{ver}/` | Version backup for rollback |
| `runtimes/{kind}/` | Auto-installed runtimes |
| `tenants/{id}/{name}/` | Per-tenant isolated workspace |
| `logs/{id}/{name}.log` | Agent stdout/stderr; rotated at 10 MB |
| `events.jsonl` | Append-only audit log |

## Security Model

- **Auth**: `X-Apollo-Key` checked on every request; supports multiple comma-separated keys
- **Rate limiting**: per-key token bucket (default 100 RPS); hub adds 50 ms gap between metrics and catalog polls
- **Env scrubbing**: `cmd.env_clear()` before spawn — only Apollo vars + sanitized PATH
- **Process containment**: Unix = `setpgid`/`killpg`; Windows = `CREATE_NEW_PROCESS_GROUP`/`taskkill /F /T`
- **FS isolation**: `harden_path()` canonicalizes and checks `starts_with(root)` before exec

## Provider Integration

To add an agent (e.g., `openclaw`) so users can run it on any node:
1. Create a directory with `agent.yaml` + entry file(s), or host as a tarball/zip/git repo
2. Call `POST /agents/add {"source": "<path or URL>"}` or `apollo agent add <source>`
3. Users call `POST /agents/run {"agent": "openclaw", "tenant": "<user_id>"}` — isolated workspace, deterministic port

The node API (`:8080`) and hub API (`:9191`) are internal-only — place behind a TLS reverse proxy for external access.
