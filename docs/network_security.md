# APOLLO v1.2 — Network & Security Guide

**Audience:** IT Engineers / Network Security
**Classification:** Production Security
**Version:** v1.2

---

## Required Ports

| Port | Component | Protocol | Direction | Description |
|------|-----------|----------|-----------|-------------|
| 8443 | apollo-node | TCP (HTTPS/TLS) | Inbound (internal) | Node REST API — management and agent lifecycle |
| 8080 | apollo-node | TCP (HTTP) | Inbound (dev only) | Plain HTTP fallback — do not expose in production |
| 9191 | apollo-hub | TCP (HTTP) | Inbound (internal) | Hub REST API — fleet coordination (no auth required; internal only) |
| 10000–65535 | apollo-node | TCP | Inbound (internal) | Deterministic agent process ports (hash-assigned per tenant/agent) |
| 443 / custom | Outbound | TCP (HTTPS) | Outbound | Webhook delivery to provider control plane |

> All ports are configurable via `--listen` flags. Agent ports (10000–65535) are assigned deterministically: `10000 + (hash(tenant_id + agent_name) % 55535)`. No dynamic allocation; no port conflicts between tenants.

---

## Network Topology

Apollo is designed for **internal-only communication**. No component requires internet access at runtime.

```
                ┌─────────────────────────────────────────┐
                │        Provider Control Plane            │
                │      (Dashboard / Billing System)        │
                └───────────────┬─────────────────────────┘
                                │ HTTPS (your infra + webhooks)
                ┌───────────────▼─────────────────────────┐
                │        APOLLO Hub  :9191                 │
                │        (Internal VPC only)               │
                └───────┬──────────────┬───────────────────┘
                        │              │
            ┌───────────▼──┐  ┌────────▼─────────────┐
            │ Node :8443   │  │  Node :8443           │  ...
            │ (TLS, VPC)   │  │  (TLS, VPC)           │
            └──────────────┘  └──────────────────────┘
                                        ↓
                                 Outbound only:
                                 webhook delivery
```

**The node API must never be exposed to the public internet.** Only your internal control plane and the hub should communicate with node APIs.

---

## Firewall Rules

### iptables

Apply on every node host:

```bash
# Allow SSH from management network
iptables -A INPUT -p tcp --dport 22 -s 10.0.0.0/8 -j ACCEPT

# Allow apollo-node HTTPS API from internal VPC only
iptables -A INPUT -p tcp --dport 8443 -s 10.0.0.0/8 -j ACCEPT

# Allow agent ports from internal VPC only
iptables -A INPUT -p tcp --dport 10000:65535 -s 10.0.0.0/8 -j ACCEPT

# Allow apollo-hub API from internal VPC only (hub host only)
iptables -A INPUT -p tcp --dport 9191 -s 10.0.0.0/8 -j ACCEPT

# Drop all other inbound
iptables -A INPUT -j DROP

# Allow established outbound (for webhook delivery)
iptables -A OUTPUT -m state --state ESTABLISHED,RELATED -j ACCEPT
```

### ufw

```bash
# Node host
sudo ufw default deny incoming
sudo ufw allow from 10.0.0.0/8 to any port 22
sudo ufw allow from 10.0.0.0/8 to any port 8443
sudo ufw allow from 10.0.0.0/8 to any port 10000:65535 proto tcp

# Hub host (add to above)
sudo ufw allow from 10.0.0.0/8 to any port 9191

sudo ufw enable
```

Replace `10.0.0.0/8` with your actual VPC CIDR range.

---

## TLS Configuration

Apollo v1.2 includes **native TLS** via `rustls`. No reverse proxy is required for encrypted transport.

### Providing Certificates

```bash
# Start node with TLS
apollo node start \
  --listen 0.0.0.0:8443 \
  --tls-cert /etc/apollo/node.crt \
  --tls-key  /etc/apollo/node.key \
  --secret-keys "${APOLLO_SECRET_KEYS}"
```

When both `--tls-cert` and `--tls-key` are provided, the node binds HTTPS. When omitted, it binds plain HTTP (development only).

### Certificate Sources

| Source | Notes |
|--------|-------|
| Internal CA / corporate PKI | Preferred for enterprise — trust can be scoped to internal CA |
| Self-signed | Acceptable for VPC-internal deployments where all callers are controlled |
| Let's Encrypt | Only if node is reachable from a publicly routable hostname |

### Key Protection

```bash
chmod 600 /etc/apollo/node.key
chown apollo:apollo /etc/apollo/node.key
```

The private key must be readable only by the `apollo` service account.

---

## Authentication

Apollo v1.2 supports two authentication methods, checked in order on every request:

### Method 1 — X-Apollo-Key (API Key)

```
X-Apollo-Key: <secret>
```

**Key management rules:**

| Rule | Detail |
|------|--------|
| Storage | `/etc/apollo/env` file, mode 640. Never hardcode in scripts or git. |
| Rotation | Multiple keys can be active simultaneously. Add new key, verify, remove old key. |
| Length | Minimum 32 characters. Use `openssl rand -hex 32`. |
| Per-node | Each node has its own key. The hub stores the key per node entry. |
| Rate limit | 100 RPS per key. Exceeding returns `429 Too Many Requests`. |

### Method 2 — JWT (Bearer Token)

```
Authorization: Bearer <HS256-JWT>
```

JWT claims:
```json
{
  "sub": "sub-operator-name",
  "exp": 1746360000,
  "keys": ["restricted-key-1"]
}
```

If `keys` is non-empty, the bearer token acts as a restricted credential — the caller can only use keys listed in the claim. Use this to issue scoped tokens to sub-operators without exposing the master key.

**Generating a JWT (example with Python):**

```python
import jwt, time
token = jwt.encode(
    {"sub": "hostinger-control-plane", "exp": int(time.time()) + 86400, "keys": []},
    "your-jwt-secret",
    algorithm="HS256"
)
```

---

## Rate Limiting

Apollo enforces a per-key token bucket at **100 RPS**. This applies to all authenticated endpoints.

- Requests that exceed the limit receive `HTTP 429 Too Many Requests`
- The hub adds a 50 ms delay between its `/metrics` and `/agents/list` polls to stay within this limit
- Adjust the bucket configuration if your control plane legitimately exceeds 100 RPS per key (use multiple keys to distribute load)

---

## Webhook Outbound Security

When `--webhook-url` is configured, Apollo POSTs lifecycle events to your control plane. All payloads are signed:

```
X-Apollo-Signature: sha256=<hex(HMAC-SHA256(body, secret))>
X-Apollo-Event: AGENT_START
```

**Verify the signature on your webhook receiver:**

```python
import hmac, hashlib

def verify(body: bytes, signature: str, secret: str) -> bool:
    expected = "sha256=" + hmac.new(
        secret.encode(), body, hashlib.sha256
    ).hexdigest()
    return hmac.compare_digest(expected, signature)
```

Configure `--webhook-secret` on the node and `--webhook-secret` on the hub to enable signed payloads. Without a secret, payloads are unsigned.

---

## Internal-Only Communication Model

1. **Node API is not public.** The HTTPS node API assumes a trusted caller. Deploy inside a private VPC.

2. **Hub is not public.** The hub REST API on port 9191 has no authentication. Restrict it to VPC-internal access only via firewall rules.

3. **Agent processes cannot reach internal services.** The runtime enforces a network kill-switch blocking outbound connections to RFC 1918 private address ranges.

4. **Tenant isolation is filesystem-enforced.** Each tenant's workspace is confined to `.apollo/tenants/{tenant_id}/`. Path canonicalization prevents workspace escapes.

5. **Secrets are never logged.** Per-tenant secrets (stored at `secrets/{tenant_id}.json`, mode 0600) are loaded only at agent spawn and never appear in logs, API responses, or audit events.

6. **Environment scrubbing.** `env_clear()` is called before spawn. Agents inherit only: Apollo-provided vars, tenant secrets, and a sanitized `PATH`. No host credentials or environment variables are passed through.

---

## Security Controls Summary

| Control | Status in v1.2 |
|---------|---------------|
| TLS for node API | Native — `rustls` via `axum-server` |
| API key authentication | Enforced; multiple keys; rotation-safe |
| JWT authentication | HS256; scoped `keys` claim |
| Rate limiting | 100 RPS per key; 429 on breach |
| Secrets storage | Mode 0600; never logged |
| Environment scrubbing | `env_clear()` before every spawn |
| Process group containment | `setpgid` / `killpg` (Unix); `CREATE_NEW_PROCESS_GROUP` / `taskkill /F /T` (Windows) |
| FS workspace isolation | `harden_path()` canonicalization + root check |
| Webhook integrity | HMAC-SHA256 signed payloads |
| Audit trail | Append-only `events.jsonl`; all lifecycle events |
| Binary integrity | SHA-256 manifest; `apollo doctor` validates on every deployment |

---

## Hardening Checklist

- [ ] Node port 8443 is not reachable from the public internet
- [ ] Hub port 9191 is not reachable from the public internet
- [ ] `/etc/apollo/env` is mode 640, owned by `root:apollo`
- [ ] `/etc/apollo/node.key` is mode 600, owned by `apollo:apollo`
- [ ] `APOLLO_SECRET_KEYS` values are stored in `/etc/apollo/env`, not in scripts or git
- [ ] `APOLLO_JWT_SECRET` is at least 64 characters, cryptographically random
- [ ] `--webhook-secret` is set and signature verification is implemented on your receiver
- [ ] The `apollo` service account has no login shell (`/usr/sbin/nologin`)
- [ ] `apollo doctor` returns `PRODUCTION READY` after firewall changes
- [ ] Audit log (`events.jsonl`) is monitored or shipped to your SIEM
- [ ] Agent logs (`logs/`) are included in your log retention policy

---

*See [Production Deployment Guide](production_deployment.md) for systemd service configuration.*
