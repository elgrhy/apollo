# APOLLO v1.0 — Production Deployment Guide

**Audience:** IT Engineers / System Administrators  
**Classification:** Production Operations  
**Version:** v1.0 (Frozen)

---

## Recommended Directory Structure

```
/usr/local/bin/apollo-node/
├── apollo          # Node CLI and daemon binary
└── apollo-hub      # Hub coordinator daemon binary

/usr/local/bin/
└── apollo          # Global symlink → apollo-node/apollo

/var/lib/apollo/
└── .apollo/
    ├── events.jsonl              # Audit log (append-only)
    ├── hub_nodes.json            # Hub node registry
    └── tenants/
        └── {tenant_id}/
            └── {agent_name}/    # Isolated agent workspace

/etc/systemd/system/
├── apollo-node.service
└── apollo-hub.service

/var/log/apollo/                 # Optional symlink to .apollo/ logs
```

---

## Systemd Service: apollo-node

Create `/etc/systemd/system/apollo-node.service`:

```ini
[Unit]
Description=APOLLO Node — Agent Execution Engine
After=network.target
StartLimitIntervalSec=60
StartLimitBurst=5

[Service]
Type=simple
User=apollo
Group=apollo
WorkingDirectory=/var/lib/apollo
ExecStart=/usr/local/bin/apollo-node/apollo node start \
    --listen 0.0.0.0:8080 \
    --base-dir /var/lib/apollo \
    --max-agents 50 \
    --secret-keys "REPLACE_WITH_SECRET_KEY"
Restart=on-failure
RestartSec=5s
StandardOutput=journal
StandardError=journal
SyslogIdentifier=apollo-node

# Hardening
NoNewPrivileges=true
ProtectSystem=strict
ReadWritePaths=/var/lib/apollo
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

---

## Systemd Service: apollo-hub

Create `/etc/systemd/system/apollo-hub.service`:

```ini
[Unit]
Description=APOLLO Hub — Fleet Coordination Layer
After=network.target
StartLimitIntervalSec=60
StartLimitBurst=5

[Service]
Type=simple
User=apollo
Group=apollo
WorkingDirectory=/var/lib/apollo
ExecStart=/usr/local/bin/apollo-node/apollo-hub start \
    --listen 0.0.0.0:9090 \
    --storage /var/lib/apollo/.apollo/hub_nodes.json
Restart=on-failure
RestartSec=5s
StandardOutput=journal
StandardError=journal
SyslogIdentifier=apollo-hub

# Hardening
NoNewPrivileges=true
ProtectSystem=strict
ReadWritePaths=/var/lib/apollo
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

---

## Service Account Setup

Create a dedicated, unprivileged system account:

```bash
sudo useradd --system --no-create-home --shell /usr/sbin/nologin apollo
sudo mkdir -p /var/lib/apollo
sudo chown -R apollo:apollo /var/lib/apollo
```

---

## Enabling and Starting Services

```bash
# Reload systemd after creating unit files
sudo systemctl daemon-reload

# Enable services to start on boot
sudo systemctl enable apollo-node
sudo systemctl enable apollo-hub

# Start services
sudo systemctl start apollo-node
sudo systemctl start apollo-hub

# Verify status
sudo systemctl status apollo-node
sudo systemctl status apollo-hub
```

---

## Log Locations

| Component | Log Destination | Command |
|:---|:---|:---|
| apollo-node (stdout/stderr) | systemd journal | `journalctl -u apollo-node -f` |
| apollo-hub (stdout/stderr) | systemd journal | `journalctl -u apollo-hub -f` |
| Audit event spine | `/var/lib/apollo/.apollo/events.jsonl` | `tail -f /var/lib/apollo/.apollo/events.jsonl` |
| Node registry state | `/var/lib/apollo/.apollo/hub_nodes.json` | `cat /var/lib/apollo/.apollo/hub_nodes.json` |

The audit log (`events.jsonl`) is append-only and records every agent lifecycle event with a causal trace. Do not rotate or truncate this file without first archiving it.

---

## Restart Behavior

| Condition | Behavior | Recovery Time |
|:---|:---|:---|
| Normal exit (code 0) | Service restarts after 5 seconds | < 10 seconds |
| Crash (non-zero exit) | Service restarts after 5 seconds | < 10 seconds |
| Consecutive failures (5 in 60s) | systemd stops restart attempts | Manual intervention required |
| Node restart with active agents | Pre-flight orphan cleanup runs on boot | Automatic, < 30 seconds |

On startup, the node daemon performs an orphan sweep: any agent processes from a prior session that are still running are cleanly terminated before the new session begins. No manual cleanup is required after a crash or hard reboot.

---

## Multi-Node Deployment

For fleets with more than one node, register each node with the hub after starting both services:

```bash
# From the hub host, register a node
curl -s -X POST http://localhost:9090/nodes/add \
  -H "Content-Type: application/json" \
  -d '{"id": "edge-01", "url": "http://10.0.1.10:8080", "key": "NODE_SECRET_KEY"}'
```

Query fleet status:

```bash
curl -s http://localhost:9090/nodes/status
```

Request the best available node for a new agent:

```bash
curl -s http://localhost:9090/nodes/best
```

---

## Acceptance Validation

After deployment, confirm production readiness on every node:

```bash
apollo doctor
```

All checks must return `OK`. Run this command after every deployment, upgrade, or configuration change.

---

*See [Network & Security Guide](network_security.md) for firewall configuration.*  
*See [SLA](sla.md) for uptime expectations and failure recovery guarantees.*
