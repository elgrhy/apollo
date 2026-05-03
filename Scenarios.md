# Apollo — Enterprise Scenarios

## Scenario 1: Hostinger User Installs OpenClaw

### The user's experience

1. User logs into Hostinger control panel → goes to **AI Agents** → clicks **OpenClaw — AI Research Assistant** → clicks **Install**
2. A progress bar appears for 3 seconds → "OpenClaw is ready. Open it →"
3. User opens their OpenClaw dashboard at `https://user-12345.openclaw.hostinger.com`

### What Apollo does under the hood

```
Hostinger Control Plane
        │
        │ 1. GET http://apollo-hub:9191/nodes/best?region=eu
        │    Response: {"node":"node-eu-07","ip":"node-eu-07:8080","active_agents":47,"max_agents":200}
        │
        │ 2. PUT http://node-eu-07:8080/tenants/user-12345/secrets
        │    Body: {"OPENAI_KEY":"sk-...","TELEGRAM_TOKEN":"bot:..."}
        │    (stored encrypted, injected at spawn — agent code never touches key management)
        │
        │ 3. POST http://node-eu-07:8080/agents/run
        │    Body: {"agent":"openclaw","tenant":"user-12345"}
        │
        │ 4. Apollo node-eu-07:
        │    - Looks up openclaw spec from agents.json
        │    - Creates /var/lib/apollo/tenants/user-12345/openclaw/
        │    - Loads secrets for user-12345, merges into process env
        │    - Mounts volumes: /var/lib/apollo/volumes/user-12345/openclaw/data/ → APOLLO_VOLUME_DATA
        │    - Spawns: python3 /var/lib/apollo/agents/openclaw/main.py
        │    - Process group isolated (setpgid / CREATE_NEW_PROCESS_GROUP on Windows)
        │    - Port: 10000 + hash("user-12345"+"openclaw") % 55535 = 34891
        │    - Fires webhook: POST https://hostinger.com/apollo-events {"event":"AGENT_START",...}
        │
        │ 5. Response: {"pid":98234,"port":34891,"status":"running"}
        │
        │ 6. Hostinger registers:
        │    user-12345.openclaw.hostinger.com → node-eu-07:34891
        │    (nginx upstream rule, Cloudflare worker, or API gateway route)
        │
User browser → Hostinger CDN → nginx → node-eu-07:34891 → OpenClaw process for user-12345
```

### At 50,000 users

- Hub routes each installation to the least-loaded available node in the user's region
- When `node-eu-07` hits 200 agents, hub stops routing to it, moves to `node-eu-08`
- Each user's process is fully isolated — one crash never affects another tenant
- Hostinger IT adds capacity with: `apollo-hub add --ip new-node:8080 --key KEY --region eu`
- Usage events fire every 60s → Hostinger's billing system tracks CPU-seconds per tenant

---

## Scenario 2: IT Team (Google / Hostinger) Sets Up Apollo

### Day 1 — Single region, 3 nodes

**On each node server (Linux, macOS, or Windows Server):**

```bash
# Install
curl -fsSL https://apollo.sh/install.sh | bash
# or Windows:
irm https://apollo.sh/install.ps1 | iex

# Generate a TLS cert (or bring your own)
openssl req -x509 -newkey rsa:4096 -keyout node.key -out node.crt -days 365 -nodes

# Start the node as a system service
apollo node start \
  --listen 0.0.0.0:8443 \
  --tls-cert /etc/apollo/node.crt \
  --tls-key  /etc/apollo/node.key \
  --base-dir /var/lib/apollo \
  --max-agents 200 \
  --secret-keys "$APOLLO_KEY" \
  --webhook-url https://control.hostinger.com/apollo-events \
  --region eu-west-1
```

**Register all agents (one-time, done by the platform team):**

```bash
# From git (recommended — auto-updates via CI/CD)
apollo agent --base-dir /var/lib/apollo add https://github.com/openclaw/openclaw.git

# From HTTPS release tarball
apollo agent --base-dir /var/lib/apollo add https://releases.openclaw.ai/openclaw-2.1.tar.gz

# GX runtime agent — Apollo auto-downloads GX if not installed
apollo agent --base-dir /var/lib/apollo add https://github.com/your-org/gx-agent.git
```

**Start the hub:**

```bash
apollo-hub start \
  --listen 0.0.0.0:9191 \
  --webhook-url https://control.hostinger.com/apollo-scale \
  --scale-threshold 0.80

# Register each node
apollo-hub add --ip node-eu-01:8443 --key "$APOLLO_KEY" --name node-eu-01 --region eu-west-1
apollo-hub add --ip node-eu-02:8443 --key "$APOLLO_KEY" --name node-eu-02 --region eu-west-1
apollo-hub add --ip node-eu-03:8443 --key "$APOLLO_KEY" --name node-eu-03 --region eu-west-1

# Verify fleet
curl http://localhost:9191/summary
# {"nodes_total":3,"nodes_online":3,"fleet_capacity":600,"catalog_agents":3}
```

### Day 30 — Scaling to 50 nodes

```bash
# Provision servers via Terraform/Pulumi — same install script
# Then for each new server:
apollo-hub add --ip new-node-XX:8443 --key "$APOLLO_KEY" --region eu-west-1
# Hub immediately routes to new nodes. Zero downtime. Zero config changes elsewhere.
```

### What the IT manager sees

```
Fleet Dashboard (apollo-hub /summary)
──────────────────────────────────────
Nodes:     50 online / 50 total
Region:    eu-west-1
Capacity:  10,000 agents (200 per node × 50 nodes)
Active:    8,340 running agent instances
Top agents:
  openclaw    6,200 instances
  shell-agent 2,140 instances
Catalog:    3 registered agents
Scale alert: fired at 80% capacity (8,000 agents) → Terraform auto-provisioned 10 new nodes
```

---

## Scenario 3: The Always-On Problem — Apollo vs. Cloud Run

### Why Cloud Run fails for agents

```
User connects OpenClaw to WhatsApp
         │
         │ User sends WhatsApp message at 3am
         │
WhatsApp Business API fires webhook:
POST https://openclaw.hostinger.com/webhook/user-12345
         │
         ▼
Cloud Run container for user-12345
┌─────────────────────────────────────┐
│  COLD START: 2–4 seconds            │
│  Container image pull               │
│  Python interpreter init            │
│  App startup + auth to WhatsApp API │
└─────────────────────────────────────┘
         │
         ▼ (too late)
WhatsApp webhook already timed out (3s SLA)
→ message dropped
→ WhatsApp may disable the webhook entirely
```

Cloud Run also kills the container after ~15 minutes of inactivity. Any persistent WebSocket connection (Telegram bot, WhatsApp long-poll, trading feed, IoT stream) is terminated and cannot be re-established automatically.

### How Apollo solves it

```
User connects OpenClaw to WhatsApp
         │
Apollo has been running openclaw for user-12345
since installation — the process NEVER stops.
         │
         │ User sends WhatsApp message at 3am
         │
WhatsApp webhook → nginx → node-eu-07:34891
                                │
                       ┌────────▼────────────┐
                       │  OpenClaw process   │
                       │  running since Day 1│
                       │  already authed to  │
                       │  WhatsApp API       │
                       │  holding connection │
                       └────────┬────────────┘
                                │
                       Response in <100ms
```

### The specific Apollo capabilities that make this work

**1. Process never sleeps**
`apollo node start` runs as a systemd service. Agent processes run until `POST /agents/stop` is called. No inactivity timeout. No scale-to-zero.

**2. Persistent TCP/WebSocket connections**
OpenClaw can open a WebSocket to Telegram's bot API, use WhatsApp's persistent connection, or hold any long-lived socket on startup — and keep it open indefinitely. Cloud Run kills this after ~15 minutes idle.

**3. Orphan recovery on node restart**
If the server reboots, Apollo detects orphaned agents on startup (marked `running` in `instances/{tenant}.json`) and re-spawns them automatically. The WhatsApp connection is re-established within seconds, not minutes.

**4. Restart policy**
`agent.yaml` declares `max_restarts: 3 / window_secs: 60`. If OpenClaw crashes, Apollo respawns it. The WhatsApp webhook resumes before the user notices.

**5. Deterministic port**
`nginx proxy_pass http://node-eu-07:34891` is configured once and never changes. No service discovery updates, no DNS TTL, no redeployment when the agent restarts.

**6. Persistent volumes**
OpenClaw's conversation history, user preferences, and vector store live in `volumes/user-12345/openclaw/data/`. Survives restarts, upgrades, and node migrations.

### Code inside OpenClaw that works on Apollo but not on Cloud Run

```python
# main.py — runs on Apollo, impossible on Cloud Run
import os, asyncio
from telegram.ext import Application
from whatsapp import WhatsAppClient

PORT = int(os.environ["APOLLO_PORT"])
WORKSPACE = os.environ["APOLLO_WORKSPACE"]
VOLUME = os.environ["APOLLO_VOLUME_DATA"]  # persistent storage

async def main():
    # These connections stay open forever on Apollo.
    # Cloud Run kills them after 15 min idle.
    tg_app = Application.builder().token(os.environ["TELEGRAM_TOKEN"]).build()
    wa_client = WhatsAppClient(os.environ["WHATSAPP_TOKEN"])

    await tg_app.initialize()
    await tg_app.start()
    await tg_app.updater.start_polling()  # long-poll, never stops

    # HTTP server for webhooks
    app = create_http_app(wa_client, storage_dir=VOLUME)
    await app.run(host="0.0.0.0", port=PORT)

asyncio.run(main())
```

### Cost comparison at 10,000 active users

|  | Cloud Run | Apollo |
|--|-----------|--------|
| Idle cost | Near zero (scales to 0) | Full node cost |
| Message latency | 2–4 s cold start | <100 ms always |
| Persistent WebSocket | Not supported | Native |
| WhatsApp / Telegram bots | Unreliable, drops messages | Production-grade |
| Stateful memory across sessions | Lost on scale-down | Persistent in workspace |
| Multi-tenant isolation | Container per user (expensive) | Process per user (efficient) |
| 10,000 users cost (AWS) | ~$0 idle + $3,000/peak surge | ~$800/mo fixed (40 × $20 VPS) |

Apollo trades idle cost for reliability. For agents users expect to be "always there" — the same tradeoff that made dedicated hosting survive despite serverless.

---

## Summary: Why Providers Choose Apollo Over Alternatives

| Need | Lambda / Cloud Run | K8s + custom | Apollo |
|------|-------------------|-------------|--------|
| Always-on persistent processes | No | Yes, complex | Yes, simple |
| Tenant isolation | Container per tenant (costly) | Namespace per tenant (complex) | Process per tenant (built-in) |
| Any language / runtime | Limited | Yes | Yes + auto-provision |
| Agent versioning + rollback | Manual CI/CD | Manual helm | Built-in |
| Fleet routing | Custom service mesh | Ingress + operators | Built-in hub |
| Cold start for webhooks | 2–4 s | <1 s | <10 ms |
| Ops burden | Low (managed) | Very high | Low (single binary) |
| Vendor lock-in | High | None | None (open source) |
