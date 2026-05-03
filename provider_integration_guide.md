# MARS Headless Engine Integration (v1.1)

This document defines how hosting providers integrate with a standalone MARS Node. All lifecycle endpoints are now **fully functional**.

## 1. Core Architecture
MARS is a **Headless Execution Engine**. Providers manage the node via REST API.

## 2. Authentication
All requests must include `X-Mars-Key`.
- **Header**: `X-Mars-Key: <secret>`

## 3. Headless API Endpoints

### 🟢 Node Metrics
**GET** `/metrics`
- **Response**: `{"active_agents": 12, "max_agents": 50}`

### 📦 Register Agent
**POST** `/agents/add`
- **Body**: `{"source": "/absolute/path/to/agent"}`
- **Purpose**: Copies package to global store and generates SHA-256 hash.

### 🚀 Run Agent
**POST** `/agents/run`
- **Body**: `{"agent": "openclaw", "tenant": "user_123"}`
- **Response**: `{"id": "openclaw-1234", "pid": 5678, "port": 10234, ...}`

### 🛑 Stop Agent
**DELETE** `/agents/stop`
- **Body**: `{"agent": "openclaw", "tenant": "user_123"}`
- **Purpose**: Terminates entire process group and releases resources.

## 4. Operational Checklist
- **Systemd**: Run `mars node start` as the system daemon.
- **Firewall**: Only expose the node port to your internal control plane.
- **Monitoring**: Check `/metrics` every 10s for cluster load balancing.
