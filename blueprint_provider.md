# APOLLO System Specification: Provider-Grade Standalone Engine

## 1. Core Vision
APOLLO is a **headless execution engine** that replaces generic virtualization with **AI-native capability isolation**. It is built for infrastructure providers who need to host thousands of independent agent workloads with strict resource governance and security.

## 2. Architecture: "The Standalone Node"
APOLLO prioritizes a **standalone-first** architecture. Each node is self-contained and exposes a management API for direct integration with provider control panels.

### A. APOLLO Node (Rust Daemon)
- **Headless API**: REST-based lifecycle management (Add, Run, Stop, Metrics).
- **Multi-Tenant Isolation**: Cryptographic tenant separation and workspace-locked filesystems.
- **Resource Governance**: Active CPU/Memory policing via `sysinfo` and `nix` process groups.
- **Node Identity**: Deterministic capability detection and hardware profiling.

### B. APOLLO Runtime (Sandbox)
- **Hardened Processes**: Standard OS processes wrapped in process groups (`setpgid`) for absolute termination control.
- **Environment Scrubbing**: 100% sanitized environment variables.
- **Network Kill-switch**: Default blocking of internal network ranges and scanning.

### C. The Minimal Hub (Coordination)
- **Node Registry**: Fleet-wide inventory of active nodes and keys.
- **Health Polling**: Centralized visibility into cluster-wide capacity and load mapping.

## 3. The Provider-Node Contract
Providers integrate APOLLO nodes directly into their existing architecture:
1.  **Deployment**: `apollo node start` runs as a system service.
2.  **Authentication**: `X-Apollo-Key` secured REST requests.
3.  **Observability**: `/metrics` polling for real-time fleet density.

## 4. Multi-Tenant Execution Model
- **Isolated Workspace**: `.apollo/tenants/{tenant_id}/{agent_name}/`
- **Port Management**: Deterministic, hash-based port assignment to prevent collisions.
- **Log Isolation**: Individual log streams per tenant/agent with automated rotation.

## 5. Security Posture
- **FS Boundary**: Path-canonicalization to prevent workspace escapes.
- **Zombie Protection**: Process group signaling (`killpg`) ensures no orphaned processes.
- **Fleet Defense**: Rate-limiting and multiple secret key support for cluster integrity.
