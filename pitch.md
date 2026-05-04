# APOLLO v1.2
## The Agent Execution Layer for Enterprise Infrastructure

**Confidential — Providers · Investors · IT Leadership**

---

# THE PROBLEM

## AI Agents Have No Home

Every organization deploying AI agents hits the same three walls — and nobody has solved them at the infrastructure level.

---

**Wall 1: No isolation between tenants**

Agents are long-running, stateful processes. When a thousand users run the same agent on the same server, one bad actor can crash every other user's session, read another tenant's files, or exhaust shared memory. Generic cloud VMs were not built for this. The result is security incidents, support escalations, and agents that enterprise IT refuses to approve.

**Wall 2: Always-on is impossible on serverless**

Modern AI agents — the kind connected to WhatsApp, Telegram, email, and Slack — cannot tolerate cold starts. Google Cloud Run and AWS Lambda spin down after inactivity. When a message arrives for a user's agent, a 5-second spin-up is not acceptable. Providers are forced to choose between paying for always-on VMs per user (prohibitively expensive) or delivering a slow, unreliable experience.

**Wall 3: No billing, no observability, no control**

Providers cannot answer basic questions: How much CPU did tenant X consume this month? When did their agent crash? Which node in Frankfurt has capacity for a new user? The data exists scattered across logs, metrics, and process tables — with no standard to query it.

---

**The result:** Engineering teams are rebuilding the same agent management infrastructure independently, project by project, company by company. The execution layer is missing.

---

# THE SOLUTION

## Apollo — Infrastructure Purpose-Built for AI Agents

Apollo is a self-hosted, multi-tenant agent execution engine. It runs on your servers, manages the full lifecycle of every agent, isolates every tenant, meters every resource, and exposes a single REST API that your control plane uses to manage the entire fleet.

You bring the servers. Apollo brings everything else.

```
┌──────────────────────────────────────────────────────────────────┐
│                    Your Provider Platform                        │
│      (billing, user management, dashboard, control plane)        │
└───────────────────────┬──────────────────────────────────────────┘
                        │  REST API
         ┌──────────────▼──────────────────────────────────┐
         │              Apollo Hub (fleet brain)            │
         │  Region routing · Auto-scale signals · Catalog  │
         └──────┬───────────────────┬──────────────────────┘
                │ 10s poll          │ 10s poll
    ┌───────────▼────────┐  ┌───────▼──────────────┐
    │   Apollo Node      │  │   Apollo Node         │
    │   us-east-1        │  │   eu-west-1           │
    │   TLS :8443        │  │   TLS :8443           │
    │   user_1 → agent   │  │   user_9004 → agent   │
    │   user_2 → agent   │  │   user_9005 → agent   │
    │   ...              │  │   ...                 │
    └────────────────────┘  └───────────────────────┘
```

One Apollo node handles hundreds of concurrent agent processes. The hub coordinates fleets of nodes across any number of regions. Providers integrate once and scale forever.

---

# HOW IT WORKS

## For a User Signing Up on Hostinger

*Hostinger offers "AI agent hosting" — users can activate an openclaw agent from the dashboard.*

**What the user sees:** Click "Activate openclaw" → "Your agent is ready." → Start chatting.

**What Apollo does in under 2 seconds:**

1. Hostinger's control plane calls `POST /agents/run {"agent":"openclaw","tenant":"user_94812"}` on the least-loaded Apollo node in the nearest region
2. Apollo creates an isolated workspace at `tenants/user_94812/openclaw/`
3. Apollo injects the user's OPENAI\_KEY from secure storage (mode 0600, never logged)
4. Apollo spawns the agent process in a dedicated process group — it cannot touch any other user's files or processes
5. Apollo fires a signed `AGENT_START` event to Hostinger's webhook for billing and observability
6. Hostinger's control plane receives `{"port": 34201, "pid": 78890}` and maps the user's session to that port

When the user disconnects, `DELETE /agents/stop` tears down the process group instantly. CPU-seconds and memory usage have been accumulating in `usage/user_94812.json` since spawn — Hostinger calls `GET /usage/user_94812` at billing time and `POST /usage/user_94812/reset` to start the next cycle.

The user's agent data — conversation history, downloads, cached state — lives in `volumes/user_94812/openclaw/data/` and persists across every session, restart, and upgrade.

---

## For an IT Team Setting Up Apollo at Scale

**Day 1 — Installation (under 15 minutes):**

```bash
curl -fsSL https://get.apollo.systems | bash
apollo doctor   # validates sandbox, auth, event log, binary integrity
```

```bash
# Start the node (TLS, JWT, webhook, region)
apollo node start \
  --listen 0.0.0.0:8443 \
  --tls-cert /etc/apollo/node.crt \
  --tls-key  /etc/apollo/node.key \
  --secret-keys "key-1,key-2" \
  --jwt-secret "your-jwt-signing-secret" \
  --webhook-url https://control.company.com/apollo-events \
  --region us-east-1

# Register the hub
apollo-hub start \
  --webhook-url https://control.company.com/apollo-scale \
  --scale-threshold 0.80
```

**Day 7 — Fleet grows to 10 nodes across 3 regions.**
Hub routes every new agent request to the least-loaded node in the right region. No manual load balancing. No configuration changes.

**Day 30 — 50,000 active tenants.**
Each tenant has isolated storage, individual resource metering, and secrets that only their agents can see. IT receives a monthly CSV from `GET /usage` — billable hours computed to the second. Audit log has recorded every agent start, stop, crash, and recovery since Day 1.

**Standard operating tasks require no developer escalation:**

| Task | How |
|------|-----|
| Rotate an API key | Add new key to `--secret-keys`, remove old key after cutover |
| Deploy a new agent version | `POST /agents/add {"source": "https://github.com/org/agent.git"}` — Apollo fetches, validates, backs up old version |
| Roll back an agent | `POST /agents/rollback {"agent":"openclaw"}` — restores previous version in < 1 second |
| Check fleet capacity | `GET http://hub:9191/summary` |
| Trigger auto-scale | Automatic — hub fires webhook when fleet utilization exceeds 80% |
| Reset billing period | `POST /usage/{tenant_id}/reset` |

---

## The Cloud Run Problem — Solved

Google Cloud Run, AWS Lambda, and similar serverless platforms spin down containers after inactivity. A user's openclaw agent connected to their WhatsApp account needs to respond within seconds — not after a 5-second cold start.

**Without Apollo:** Provider runs a Cloud Run service per user. Costs $0.10/user/month at rest. But every cold start takes 3–8 seconds. Users complain. Engineers add always-warm keepalive pings. The workaround costs more than the saving.

**With Apollo:** One Apollo node runs 200 agent processes simultaneously, always warm. At $50/month for the VM, the cost per user at 200 tenants is $0.25/month — with zero cold starts, full isolation, and instant response.

When 200 users become 2,000, Apollo's scale webhook fires automatically:

```json
{
  "event": "SCALE_NEEDED",
  "message": "Fleet at 1800/2000 capacity",
  "status": "alert"
}
```

Your control plane receives this, provisions a new VM, runs the one-line installer, registers the node with the hub — and capacity doubles without touching the running fleet.

```python
# openclaw inside Apollo — zero cold start, always warm
import os, asyncio

OPENAI_KEY = os.environ["OPENAI_KEY"]      # injected by Apollo at spawn
DATA_DIR   = os.environ["APOLLO_VOLUME_DATA"]  # persistent per-tenant storage

async def handle_message(user_message: str) -> str:
    # respond immediately — no cold start, process has been warm since activation
    ...
```

---

# WHAT APOLLO DELIVERS

## Complete Enterprise Feature Set (v1.2)

| Capability | Mechanism |
|-----------|-----------|
| **Multi-tenant isolation** | Dedicated process group + workspace per tenant; `killpg` clears entire subtree |
| **TLS / HTTPS** | `rustls` via axum-server; cert + key at startup; no separate terminator needed |
| **JWT + key auth** | `X-Apollo-Key` OR `Authorization: Bearer` HS256-JWT; scoped tokens per sub-operator |
| **Per-tenant secrets** | Stored at mode 0600; injected at spawn; never logged |
| **Usage metering** | CPU-seconds + memory-GB-seconds per tenant; 60s sampling; reset API for billing cycles |
| **Persistent volumes** | Per-tenant, per-agent storage; survives restarts, rollbacks, upgrades |
| **Webhook events** | HMAC-SHA256 signed; AGENT\_START, AGENT\_STOP, CAPACITY\_WARNING, SCALE\_NEEDED |
| **Auto-scale alerting** | Hub fires on configurable threshold (default 80%); re-arms after scale-down |
| **Multi-region routing** | `/nodes/best?region=X`; `/regions` per-region capacity; region tag per node |
| **Rate limiting** | Per-key token bucket, 100 RPS; 429 on breach |
| **Agent versioning** | Backup on re-register; `POST /agents/rollback` restores previous version |
| **Runtime auto-install** | `runtime.install` in agent.yaml; Apollo downloads + installs runtime on first use |
| **Any language** | Python, Node, Go, Deno, Bun, Ruby, PHP, Java, .NET, Rust, Shell, or any custom command |
| **Audit trail** | Append-only `events.jsonl`; every lifecycle event recorded |
| **Cross-platform** | Linux, macOS, Windows; `aarch64` and `x86_64` |

---

# SECURITY MODEL

## Defense in Depth — No Cloud Required

Apollo is designed for private network deployment. No component requires internet access at runtime. No telemetry leaves the server.

| Layer | Control |
|-------|---------|
| **Tenant isolation** | Filesystem workspace per tenant; `harden_path()` canonicalization prevents escapes |
| **Process containment** | `setpgid` on launch; `killpg` on termination — entire process tree killed |
| **Environment sanitization** | `env_clear()` before spawn; only Apollo vars + tenant secrets + sanitized PATH |
| **Secrets protection** | Mode 0600 storage; loaded only at spawn; excluded from all API responses |
| **Auth** | X-Apollo-Key (multi-key, rotation-safe) + JWT (scoped tokens for sub-operators) |
| **Transport** | TLS via rustls; no plaintext required in production |
| **Webhook integrity** | HMAC-SHA256 signature on every outbound event |
| **Audit log** | Append-only; tamper-evident; SOC2 Type II compatible |
| **Binary integrity** | SHA-256 manifest verified at install; `apollo doctor` validates on every deployment |

This model satisfies the core requirements of enterprise security reviews: isolation, traceability, access control, and encrypted transport — built in, not bolted on.

---

# FOR INVESTORS

## The Missing Infrastructure Layer

The market for AI agent tooling has split in two: tools for *building* agents (LangChain, AutoGen, CrewAI) and tools for *deploying* them (nothing purpose-built). Apollo fills the gap that every infrastructure provider and enterprise IT team is currently solving by hand.

### The Moat

**Trust moat:** Enterprises and regulated industries cannot send agent execution to a third-party cloud. Data residency laws, compliance requirements, and security policies demand on-premises or private-cloud execution. Apollo is self-hosted by design — it runs on the customer's own infrastructure under their own security controls.

**Compliance moat:** Apollo's audit trail, tenant isolation, and access controls are architectural decisions, not features. Competitors retrofitting compliance onto generic infrastructure will be years behind on the institutional approval process that Apollo already satisfies.

**Switching cost:** Once a provider's platform is integrated against Apollo's REST API, their agent lifecycle depends on Apollo. Audit logs accumulate. Tenant workspaces are managed by Apollo. Volumes contain years of per-user data. The integration cost of switching is real and compounds over time.

**Distribution lock-in:** SHA-256 verified releases, one-line installer, `apollo doctor` acceptance test. Every deployment is a versioned, cryptographically verified artifact. Upgrades are controlled. Rollbacks are safe. This creates a clean licensing surface.

### Market Opportunity

Every infrastructure provider adding AI capabilities faces this problem:

- **Web hosting (Hostinger, GoDaddy):** Millions of users, agent-per-user economics require $0.25/user/month infrastructure, not $5/user/month
- **Cloud providers (AWS, GCP, Azure):** Managed agent execution is a tier above managed containers — higher margin, longer lock-in
- **Enterprise IT (Fortune 500):** AI agents touching internal data require on-premises execution with audit trails that survive a SOC2 review
- **AI product companies (openclaw, others):** Need a runtime to distribute their agents — Apollo becomes the distribution layer

### Current Status

- v1.2 is production-certified and feature-complete for enterprise requirements
- Binary integrity verification is live
- One-line installer is live (`get.apollo.systems`)
- Complete documentation: CLAUDE.md, Scenarios.md, provider integration guide, enterprise approval pack
- System is ready for pilot deployments

### The Ask

Apollo needs two things to reach commercial scale:

1. **Pilot customers** — 2–3 infrastructure providers to run production workloads and provide structured feedback under NDA
2. **Distribution** — visibility in the infrastructure provider and enterprise IT communities where deployment decisions are made

Engineering is complete. Product is complete. The gap is go-to-market.

---

# FOR PROVIDERS

## Integrate in an Afternoon

Apollo exposes a single REST API. Your control plane talks to Apollo nodes the same way it talks to any internal service.

### Integration Checklist

```bash
# 1. Install on each node (< 5 minutes)
curl -fsSL https://get.apollo.systems | bash
apollo doctor

# 2. Start the node
apollo node start --listen 0.0.0.0:8443 \
  --tls-cert cert.pem --tls-key key.pem \
  --secret-keys "your-key" \
  --webhook-url https://your-control-plane/apollo-events \
  --region us-east-1

# 3. Register your agent once
curl -X POST https://node:8443/agents/add \
  -H "X-Apollo-Key: your-key" \
  -d '{"source": "https://github.com/your-org/your-agent.git"}'

# 4. Store user secrets
curl -X PUT https://node:8443/tenants/{user_id}/secrets \
  -H "X-Apollo-Key: your-key" \
  -d '{"secrets": {"OPENAI_KEY": "sk-...", "TELEGRAM_TOKEN": "bot:..."}}'

# 5. Start an agent for each user
curl -X POST https://node:8443/agents/run \
  -H "X-Apollo-Key: your-key" \
  -d '{"agent":"your-agent","tenant":"{user_id}"}'

# 6. Bill by usage
curl https://node:8443/usage/{user_id} -H "X-Apollo-Key: your-key"
curl -X POST https://node:8443/usage/{user_id}/reset -H "X-Apollo-Key: your-key"
```

### What You Keep

- Your billing system (Apollo provides the usage data, you run the billing logic)
- Your user management (Apollo takes a tenant ID, you manage what it maps to)
- Your dashboard (Apollo provides the API, you build the UI)
- Your existing infrastructure (Apollo runs on whatever servers you already have)

Apollo is infrastructure, not a platform. It plugs into your system; it does not replace it.

---

# FOR IT MANAGEMENT

## Deploy Once. Operate Indefinitely.

Apollo reaches a state where it requires zero developer involvement to maintain. Standard failure modes are self-healing. Observability is built in. Compliance readiness ships with the binary.

### Compliance Readiness

| Requirement | Apollo Status |
|-------------|---------------|
| Tenant data isolation | Enforced at filesystem and process level — architectural, not configurable |
| Audit trail | Append-only event log; every lifecycle event recorded with timestamps |
| Access control | API key + JWT; multiple keys for rotation; scoped tokens for sub-operators |
| Encrypted transport | TLS via rustls; cert management is operator responsibility |
| Process containment | Unix process groups; no cross-tenant process visibility |
| Secret protection | Mode 0600 storage; secrets never appear in logs or API responses |
| Reproducible deployments | SHA-256 verified binaries; `apollo doctor` acceptance test on every deployment |

### Operational Guarantees

- **No manual process cleanup** — orphan sweep runs on every restart
- **No manual state reconstruction** — all state files survive crashes and reboots
- **No silent failures** — every failure mode is observable via systemd journal or audit log
- **No developer escalation for standard failure modes** — recovery is automatic
- **No cross-tenant incidents** — process group isolation means one tenant crash cannot affect others

---

# SUMMARY

| Audience | What Apollo Delivers |
|----------|----------------------|
| **Investors** | The missing execution layer for enterprise AI — trust moat, compliance moat, switching cost, and a clean licensing surface against a market that is re-inventing this infrastructure independently at every company |
| **Infrastructure Providers** | A REST API that turns any server into a multi-tenant, compliance-ready, billing-aware AI agent host — integrate in hours, scale to millions of tenants |
| **IT Management** | Self-healing, auditable, TLS-secured infrastructure that runs without developer escalation and satisfies SOC2 Type II requirements out of the box |
| **AI Product Companies** | A runtime that distributes your agent to any provider's infrastructure — one agent package, any server, any region, any scale |

---

## Contact & Distribution

**Distribution repository:** `git@github.com:elgrhy/apollo.git`
**Installer:** `curl -fsSL https://get.apollo.systems | bash`
**Acceptance test:** `apollo doctor`

---

*Apollo v1.2 — Built for elite infrastructure. TLS. JWT. Billing. Secrets. Webhooks. Multi-region.*
