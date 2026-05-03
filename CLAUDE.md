# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Run

```bash
# Build all crates (release)
cargo build --release

# Build debug (faster, used by stress_test.py)
cargo build

# Validate installation
./target/release/apollo doctor

# Run the node daemon (API on :8080)
./target/release/apollo node start --secret-keys "your-key"

# Run the hub coordinator (API on :9090)
./target/release/apollo-hub start

# Check Rust compilation without building artifacts
cargo check

# Run clippy lints
cargo clippy

# Run all tests
cargo test
```

## Agent Lifecycle (CLI)

```bash
# Register an agent package (reads agent.yaml from the directory)
./target/release/apollo agent --base-dir .apollo add ./examples/openclaw

# Start an agent for a tenant
./target/release/apollo agent --base-dir .apollo run openclaw --tenant alice

# Stop a running agent
./target/release/apollo agent --base-dir .apollo stop openclaw --tenant alice
```

Note: `--base-dir` must come before the subcommand (`add`/`run`/`stop`), not after.

## REST API (when node is running)

All requests require `X-Apollo-Key: <secret>` header.

```bash
# Node capacity
curl -H "X-Apollo-Key: KEY" http://localhost:8080/metrics

# Register agent package (absolute path required)
curl -X POST http://localhost:8080/agents/add \
  -H "X-Apollo-Key: KEY" -H "Content-Type: application/json" \
  -d '{"source": "/abs/path/to/agent"}'

# Run agent for tenant
curl -X POST http://localhost:8080/agents/run \
  -H "X-Apollo-Key: KEY" -H "Content-Type: application/json" \
  -d '{"agent": "openclaw", "tenant": "user_123"}'

# Stop agent
curl -X DELETE http://localhost:8080/agents/stop \
  -H "X-Apollo-Key: KEY" -H "Content-Type: application/json" \
  -d '{"agent": "openclaw", "tenant": "user_123"}'

# Hub fleet status
curl http://localhost:9090/status
```

## Architecture

Four Rust crates in a Cargo workspace:

- **`apollo-core`** — shared primitives: `types.rs` (all structs), `agents.rs` (registry CRUD + `register_agent_package`), `detect.rs` (node capability detection: OS, arch, RAM, runtimes, Ollama). No binary.
- **`apollo-runtime`** — `AgentRuntime` trait + `ProcessRuntime` implementation. Handles process spawning with `setpgid`, env scrubbing, port assignment (deterministic hash), log rotation, and resource enforcement (CPU/memory kill loop via `sysinfo`).
- **`apollo-node`** — binary `apollo`. Clap CLI with `node start | status`, `agent add | run | stop`, and `doctor`. Also runs `tiny_http` API server for REST management. Performs orphan recovery on startup.
- **`apollo-hub`** — binary `apollo-hub`. Clap CLI with `start | add | list`. Runs a `tiny_http` server on :9090, polls node `/metrics` every 10 seconds in background, implements circuit breaker (backs off nodes with ≥5 failures).

## Key Data Flows

**Agent Registration** (`/agents/add` or `apollo agent add`):
1. Parse `agent.yaml` from the source directory into `AgentSpec`
2. `detect_node_capabilities()` — checks OS/arch/runtimes match
3. Copy package files to `base_dir/agents/{name}/`
4. SHA-256 the yaml content → stored as `checksum`
5. Upsert `AgentRecord` in `base_dir/agents.json`

**Agent Start** (`/agents/run` or `apollo agent run`):
1. Look up `AgentSpec` from `agents.json`
2. Create tenant workspace: `base_dir/tenants/{tenant_id}/{agent_name}/`
3. Pre-flight orphan kill: scan processes for `APOLLO_WORKSPACE=<path>` env var
4. Spawn process via `python3 <entry>` or direct exec, with `setpgid(0,0)` for process group isolation
5. Inject sanitized env: `APOLLO_TENANT_ID`, `APOLLO_AGENT_NAME`, `APOLLO_PORT`, `APOLLO_WORKSPACE`, `NO_PROXY`
6. Port = `10000 + (hash(tenant_id + agent_name) % 50000)` — deterministic, no collision tracking
7. Spawn background monitor that kills process group on CPU/memory violation
8. Append `AGENT_START` event to `.apollo/events.jsonl`

**Event Spine**: `log_event()` in `types.rs` appends JSONL to `.apollo/events.jsonl` (relative to CWD when called from the API server, which runs from the working directory).

## Agent Package Format

Every agent is a directory containing `agent.yaml`:

```yaml
name: openclaw
version: 1.0.0
runtime:
  type: python3        # or "node", "rustc"
  entry: main.py
llm:
  required: true
  provider: any
  fallback: true       # allows running without LLM
resources:
  cpu: 0.5             # fraction of one core
  memory: 512mb
  timeout: 120
permissions:
  network: full
  filesystem: sandbox
  processes: restricted
compatibility:
  os: [linux, darwin]
  arch: [x86_64, aarch64]
restart_policy:
  max_restarts: 3
  window_secs: 60
```

Runtimes detected on the node: `python3`, `node`, `rustc` (via `which`). The agent's `runtime.type` must match one of these or registration fails.

## State Files

All state lives under `base_dir` (default `.apollo/`):

| File | Contents |
|------|----------|
| `agents.json` | Registered agent records (specs + checksums) |
| `instances.json` | Running/stopped agent instances with PIDs |
| `hub_nodes.json` | Hub's node registry (persisted across restarts) |
| `events.jsonl` | Append-only audit log (written relative to server CWD) |
| `agents/{name}/` | Copied agent package files (global store) |
| `tenants/{id}/{name}/` | Per-tenant isolated workspace |
| `logs/{id}/{name}.log` | Agent stdout/stderr; rotated at 10 MB |

## Security Model

- **Auth**: `X-Apollo-Key` checked on every request against `NodeConfig.secret_keys` (supports multiple keys for rotation)
- **Rate limiting**: per-key token bucket, configurable RPS (`NodeNetworkPolicy.rate_limit_rps`, default 50)
- **Env scrubbing**: `cmd.env_clear()` before spawn — only Apollo vars + sanitized PATH passed in
- **Network blocking**: `NO_PROXY` and `APOLLO_NETWORK_ALLOW_INTERNAL=false` injected (agent-side enforcement)
- **FS isolation**: `harden_path()` canonicalizes and checks `starts_with(root)` before executing
- **Process containment**: `setpgid(0,0)` on spawn; `killpg(-pid, SIGTERM/SIGKILL)` on stop

## Provider Integration

To add a new agent (e.g., `openclaw`) to Apollo so users can run it on any node:
1. Create a directory with `agent.yaml` + entry file(s)
2. POST to `/agents/add` with the absolute path, or run `apollo agent add <dir>` — this copies files to the global store and validates compatibility
3. Users then call `POST /agents/run {"agent": "openclaw", "tenant": "<user_id>"}` — each tenant gets an isolated workspace and deterministic port

The node API (`:8080`) and hub API (`:9090`) are internal-only — place behind a TLS reverse proxy for cross-datacenter use.
