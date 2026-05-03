# MARS Provider Edition

MARS is a **multi-tenant AI agent runtime layer** designed for hosting providers. It enables any hosting company (Hostinger, GoDaddy, Google Cloud, AWS, etc.) to offer AI agent capabilities to their customers instantly, safely, and at scale.

## 🚀 Core Vision
MARS provides the execution infrastructure that turns any hosting platform into an AI-native environment. It handles agent lifecycles, resource isolation, and model routing, allowing providers to monetize AI features without building complex infrastructure.

## 🏗️ Architecture

### MARS Node Agent (`mars-node`)
The core daemon running on provider infrastructure. It manages agent lifecycles and enforces resource limits.
- **REST API**: Provider-facing API for agent management.
- **Multi-Tenant**: Native isolation for multiple customers on the same node.

### MARS Runtime (`mars-runtime`)
An abstraction layer for executing agents in various environments:
- **Process Isolation**: Standard OS process sandboxing.
- **Containerization**: (Upcoming) Docker/Podman support.
- **WASM**: (Upcoming) High-density, high-security sandboxing.

### Agent Registry
Agents are versioned packages containing logic and configuration (`agent.yaml`).

## 🛠️ Getting Started

### Installation
Build the node agent using Cargo:
```bash
cargo build -p mars-node
```

### Running the Node
Start the daemon:
```bash
./target/debug/mars-node run --listen 0.0.0.0:8080 --base-dir .mars/agents
```

### Installing an Agent
Agents are installed via the REST API or CLI helper:
```bash
./target/debug/mars-node install --config examples/hello-agent.yaml
```

## 📜 License
MARS is licensed under the terms of the project's license.
