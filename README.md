# MARS: Provider-Grade AI Agent Execution Engine

MARS is a **headless, multi-tenant agent execution runtime** designed specifically for hosting providers (Hostinger, AWS, GCP, etc.). It enables providers to offer secure, isolated, and governed AI agent environments to their customers at scale.

## 🚀 Core Vision: "The Agent Engine"
MARS is not an agent framework—it is the **infrastructure layer** that runs them. It replaces generic containers with **capability-aware, resource-enforced, and security-hardened** agent sandboxes.

## 🏗️ Architecture
- **MARS Node (`mars-node`)**: The primary execution daemon. It manages the full agent lifecycle (Add, Run, Stop) via a headless REST API.
- **MARS Runtime (`mars-runtime`)**: A hardened sandbox layer that enforces CPU/Memory limits, prevents zombie processes, and isolates tenant environments.
- **MARS Hub (Minimal)**: A lightweight coordination tool for tracking node health and fleet capacity.

## 🛡️ Hardened Security
- **Fleet Auth**: Key-based API authentication (`X-Mars-Key`) with multi-key support.
- **Resource Enforcement**: Active CPU/Memory monitoring and termination.
- **Process Sandbox**: Enforced process groups (`setpgid`) to prevent zombie leaks.
- **Network Firewall**: Default blocking of private ranges and internal network scanning.

## 🛠️ Getting Started

### 1. Build the Engine
```bash
cargo build -p mars-node
```

### 2. Start the Standalone Node
```bash
./target/debug/mars-node node start --listen 0.0.0.0:8080 --secret-keys YOUR_KEY
```

### 3. Check Health
```bash
curl -H "X-Mars-Key: YOUR_KEY" http://localhost:8080/metrics
```

## 📂 Integration Guide
See the [Provider Integration Guide](.mars/provider_integration_guide.md) for full REST API specifications and deployment strategies.
