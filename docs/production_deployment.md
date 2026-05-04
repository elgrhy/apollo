# APOLLO v1.2 — Production Deployment Guide

**Audience:** IT Engineers / System Administrators
**Classification:** Production Operations
**Version:** v1.2

---

## Recommended Directory Structure

```
/usr/local/bin/apollo-node/
├── apollo          # Node CLI and daemon binary
└── apollo-hub      # Hub coordinator daemon binary

/usr/local/bin/
└── apollo          # Global symlink → apollo-node/apollo

/etc/apollo/
├── env             # Environment file (secret keys, JWT secret, webhook URL, region)
├── node.crt        # TLS certificate
└── node.key        # TLS private key (mode 0600)

/var/lib/apollo/
└── .apollo/
    ├── events.jsonl                    # Audit log (append-only, never truncate)
    ├── hub_nodes.json                  # Hub node registry
    ├── agents.json                     # Registered agent catalog
    ├── agents/{name}/                  # Agent package store
    ├── agents/{name}.v{ver}/           # Version backups for rollback
    ├── instances/{tenant_id}.json      # Running instances (sharded by tenant)
    ├── secrets/{tenant_id}.json        # Per-tenant secrets (mode 0600)
    ├── usage/{tenant_id}.json          # Billing metering data
    ├── volumes/{id}/{agent}/{vol}/     # Persistent volume mounts
    ├── runtimes/{kind}/                # Auto-installed runtimes
    ├── logs/{id}/{name}.log            # Agent stdout/stderr (rotated 10 MB)
    └── tenants/{id}/{name}/            # Per-tenant isolated workspaces

/etc/systemd/system/
├── apollo-node.service
└── apollo-hub.service
```

---

## Environment File

Create `/etc/apollo/env` before starting services. This file holds secrets and is loaded by systemd's `EnvironmentFile=` directive:

```bash
# /etc/apollo/env
# Permissions: chmod 640 /etc/apollo/env && chown root:apollo /etc/apollo/env

APOLLO_SECRET_KEYS=key-1,key-2
APOLLO_JWT_SECRET=replace-with-cryptographically-random-64-char-string
APOLLO_WEBHOOK_URL=https://control.example.com/apollo-events
APOLLO_SCALE_WEBHOOK_URL=https://control.example.com/apollo-scale
APOLLO_REGION=us-east-1
```

Generate secure values:
```bash
openssl rand -hex 32   # for each key
openssl rand -hex 64   # for JWT secret
```

---

## Systemd Service: apollo-node

Create `/etc/systemd/system/apollo-node.service`:

```ini
[Unit]
Description=APOLLO Node — Agent Execution Engine v1.2
After=network.target
StartLimitIntervalSec=60
StartLimitBurst=5

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
StandardOutput=journal
StandardError=journal
SyslogIdentifier=apollo-node

# Hardening
NoNewPrivileges=true
ProtectSystem=strict
ReadWritePaths=/var/lib/apollo /etc/apollo
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

> **Plain HTTP (dev/staging):** Replace `--listen 0.0.0.0:8443 --tls-cert ... --tls-key ...` with `--listen 0.0.0.0:8080`. All other flags remain the same.

---

## Systemd Service: apollo-hub

Create `/etc/systemd/system/apollo-hub.service`:

```ini
[Unit]
Description=APOLLO Hub — Fleet Coordination Layer v1.2
After=network.target
StartLimitIntervalSec=60
StartLimitBurst=5

[Service]
Type=simple
User=apollo
Group=apollo
WorkingDirectory=/var/lib/apollo
EnvironmentFile=/etc/apollo/env
ExecStart=/usr/local/bin/apollo-node/apollo-hub start \
    --listen 0.0.0.0:9191 \
    --storage /var/lib/apollo/.apollo/hub_nodes.json \
    --webhook-url   "${APOLLO_SCALE_WEBHOOK_URL}" \
    --scale-threshold 0.80
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
sudo mkdir -p /var/lib/apollo /etc/apollo
sudo chown -R apollo:apollo /var/lib/apollo
sudo chown root:apollo /etc/apollo
sudo chmod 750 /etc/apollo
sudo chmod 640 /etc/apollo/env
sudo chmod 600 /etc/apollo/node.key
```

---

## Enabling and Starting Services

```bash
# Reload systemd after creating unit files
sudo systemctl daemon-reload

# Enable services to start on boot
sudo systemctl enable apollo-node apollo-hub

# Start services
sudo systemctl start apollo-node apollo-hub

# Verify status
sudo systemctl status apollo-node apollo-hub
```

---

## Registering Nodes with the Hub

After the hub starts, register each node using the CLI (not a REST call — the hub has no `/nodes/add` endpoint):

```bash
apollo-hub add \
  --ip "node-hostname:8443" \
  --key "that-nodes-secret-key" \
  --name "node-us-1" \
  --region "us-east-1" \
  --storage /var/lib/apollo/.apollo/hub_nodes.json
```

Verify registration:

```bash
apollo-hub list --storage /var/lib/apollo/.apollo/hub_nodes.json
curl http://localhost:9191/nodes/status
```

Query fleet status and best available node:

```bash
curl http://localhost:9191/summary
curl "http://localhost:9191/nodes/best?region=us-east-1"
curl http://localhost:9191/regions
```

---

## Log Locations

| Component | Log Destination | Command |
|-----------|----------------|---------|
| apollo-node (stdout/stderr) | systemd journal | `journalctl -u apollo-node -f` |
| apollo-hub (stdout/stderr) | systemd journal | `journalctl -u apollo-hub -f` |
| Audit event log | `/var/lib/apollo/.apollo/events.jsonl` | `tail -f /var/lib/apollo/.apollo/events.jsonl` |
| Agent stdout/stderr | `/var/lib/apollo/.apollo/logs/{tenant}/{agent}.log` | `tail -f .apollo/logs/user_123/openclaw.log` |
| Hub node registry | `/var/lib/apollo/.apollo/hub_nodes.json` | `cat .apollo/hub_nodes.json` |
| Tenant usage | `/var/lib/apollo/.apollo/usage/{tenant_id}.json` | `cat .apollo/usage/user_123.json` |

The audit log (`events.jsonl`) is append-only. Do not rotate or truncate it without first archiving it to long-term storage. Agent logs rotate automatically at 10 MB.

---

## Restart Behavior

| Condition | Behavior | Recovery Time |
|-----------|----------|--------------|
| Normal exit (code 0) | Service restarts after 5 seconds | < 10 seconds |
| Crash (non-zero exit) | Service restarts after 5 seconds | < 10 seconds |
| Consecutive failures (5 in 60s) | systemd stops restart attempts | Manual investigation required |
| Node restart with active agents | Pre-flight orphan cleanup runs on boot | Automatic, < 30 seconds |

On startup, the node daemon performs an orphan sweep: any agent processes from a prior session are cleanly terminated before the new session begins. No manual cleanup is required after a crash or hard reboot.

---

## Upgrading to a Patch Release

```bash
# Pull the patch tag
git fetch origin
git checkout apollo-v1.2.x

# Rebuild
cargo build --release

# Verify checksums
shasum -a 256 -c CHECKSUMS.sha256

# Stop services, install, restart
sudo systemctl stop apollo-node apollo-hub
sudo cp target/release/apollo     /usr/local/bin/apollo-node/apollo
sudo cp target/release/apollo-hub /usr/local/bin/apollo-node/apollo-hub
sudo systemctl start apollo-node apollo-hub

# Validate
apollo doctor
```

Downtime per node: less than 60 seconds.

---

## Key Rotation (Zero-Downtime)

Apollo supports multiple simultaneous active keys:

1. Add the new key to the `APOLLO_SECRET_KEYS` env var (comma-separated): `key-old,key-new`
2. Reload the service: `sudo systemctl reload apollo-node` (or restart if reload is not supported)
3. Update your control plane to use the new key
4. Remove the old key from `APOLLO_SECRET_KEYS`
5. Restart the service

No agent downtime occurs during this process — only new API calls need the updated key.

---

*See [Network & Security Guide](network_security.md) for firewall and TLS configuration.*
*See [SLA](sla.md) for uptime expectations and failure recovery guarantees.*
