# APOLLO v1.0 — Network & Security Guide

**Audience:** IT Engineers / Network Security  
**Classification:** Production Security  
**Version:** v1.0 (Frozen)

---

## Required Ports

| Port | Component | Protocol | Direction | Description |
|:---|:---|:---|:---|:---|
| 8080 | apollo-node | TCP (HTTP) | Inbound | Node REST API — management and agent lifecycle |
| 9090 | apollo-hub | TCP (HTTP) | Inbound | Hub REST API — fleet coordination |
| 10000–10999 | apollo-node | TCP | Inbound (internal) | Deterministic agent process ports (hash-assigned per tenant/agent) |

> All ports are configurable via `--listen` flags. The defaults above apply to standard deployments. Agent ports (10000–10999) are assigned deterministically and do not require dynamic allocation.

---

## Network Topology

APOLLO is designed for **internal-only communication**. No component requires internet access at runtime.

```
                    ┌─────────────────────────────┐
                    │  Provider Control Plane      │
                    │  (Dashboard / Billing System)│
                    └────────────┬────────────────┘
                                 │ HTTPS (your infra)
                    ┌────────────▼────────────────┐
                    │  APOLLO Hub  :9090           │
                    │  (Internal VPC only)         │
                    └──┬──────────┬──────────┬────┘
                       │          │          │
           ┌───────────▼─┐  ┌─────▼──────┐  ┌▼──────────────┐
           │ Node :8080  │  │ Node :8080 │  │  Node :8080   │
           │  (internal) │  │ (internal) │  │  (internal)   │
           └─────────────┘  └────────────┘  └───────────────┘
```

**The node API (port 8080) must never be exposed to the public internet.** Only your internal control plane and the hub should communicate with node APIs.

---

## Firewall Rules

### iptables

Apply on every node host:

```bash
# Allow SSH from management network
iptables -A INPUT -p tcp --dport 22 -s 10.0.0.0/8 -j ACCEPT

# Allow apollo-node API from internal VPC only
iptables -A INPUT -p tcp --dport 8080 -s 10.0.0.0/8 -j ACCEPT

# Allow agent ports from internal VPC only
iptables -A INPUT -p tcp --dport 10000:10999 -s 10.0.0.0/8 -j ACCEPT

# Allow apollo-hub API from internal VPC only (hub host only)
iptables -A INPUT -p tcp --dport 9090 -s 10.0.0.0/8 -j ACCEPT

# Drop all other inbound
iptables -A INPUT -j DROP

# Allow all established outbound
iptables -A OUTPUT -m state --state ESTABLISHED,RELATED -j ACCEPT
```

### ufw

```bash
# Node host
sudo ufw default deny incoming
sudo ufw allow from 10.0.0.0/8 to any port 22
sudo ufw allow from 10.0.0.0/8 to any port 8080
sudo ufw allow from 10.0.0.0/8 to any port 10000:10999 proto tcp

# Hub host (add to above)
sudo ufw allow from 10.0.0.0/8 to any port 9090

sudo ufw enable
```

Replace `10.0.0.0/8` with your actual VPC CIDR range.

---

## X-Apollo-Key Authentication

All requests to the node REST API must include the `X-Apollo-Key` header:

```
X-Apollo-Key: <secret>
```

**Key management rules:**

| Rule | Detail |
|:---|:---|
| Key storage | Store in environment variable or secrets manager. Do not hardcode in scripts. |
| Key rotation | Multiple keys can be active simultaneously. Add the new key, verify operation, then remove the old key. |
| Key length | Minimum 32 characters. Use a cryptographically random value. |
| Key per node | Each node may have its own distinct key. The hub stores the key per node entry. |

**Generating a secure key:**

```bash
openssl rand -hex 32
```

**Setting a key when starting the node:**

```bash
apollo node start --secret-keys "$(cat /etc/apollo/secret_key)"
```

---

## Internal-Only Communication Model

APOLLO is explicitly designed with the following security assumptions:

1. **Node API is not public.** The node REST API on port 8080 assumes a trusted caller (your control plane or hub). It does not implement TLS natively. Place it behind a TLS-terminating reverse proxy (e.g., Nginx, Caddy) if cross-datacenter communication is required.

2. **Hub is not public.** The hub REST API on port 9090 is a machine-to-machine coordination tool. It should be accessible only from your control plane and node hosts.

3. **Agent processes cannot reach internal services.** The runtime enforces a network kill-switch that blocks outbound connections to private address ranges (RFC 1918) and performs rate-limiting on all agent-initiated traffic.

4. **Tenant isolation is enforced at the filesystem level.** Each tenant's workspace is confined to `.apollo/tenants/{tenant_id}/`. Path canonicalization prevents workspace escapes. No tenant can read or write another tenant's files.

5. **Environment scrubbing.** Agent processes inherit a fully sanitized environment. No secrets, tokens, or host environment variables are passed to agent subprocesses.

---

## Security Assumptions Summary

| Assumption | Status |
|:---|:---|
| Node API behind private network | Required (operator responsibility) |
| TLS for cross-datacenter traffic | Operator responsibility (reverse proxy) |
| Secret key confidentiality | Operator responsibility |
| OS-level user isolation | Provided (dedicated `apollo` service account) |
| Tenant filesystem isolation | Enforced by runtime |
| Agent environment sanitization | Enforced by runtime |
| Process group containment | Enforced by runtime (`setpgid` / `killpg`) |
| Internal network blocking for agents | Enforced by runtime |

---

## Hardening Checklist

- [ ] Node port 8080 is not reachable from the public internet
- [ ] Hub port 9090 is not reachable from the public internet
- [ ] `X-Apollo-Key` values are stored in a secrets manager, not in scripts or git
- [ ] The `apollo` service account has no login shell (`/usr/sbin/nologin`)
- [ ] `apollo doctor` returns `PRODUCTION READY` after firewall changes
- [ ] Audit log (`events.jsonl`) is monitored or shipped to your SIEM

---

*See [Production Deployment Guide](production_deployment.md) for systemd service configuration.*
