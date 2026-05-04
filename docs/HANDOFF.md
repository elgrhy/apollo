# APOLLO v1.2 — Enterprise Handoff Pack

**Status:** Production Certified
**Version:** v1.2
**Distribution:** Private — IT Teams and Infrastructure Providers Only

---

## Purpose

This pack enables any qualified IT team to deploy, operate, monitor, and maintain APOLLO v1.2 without developer involvement. All documents are production-ready and self-contained.

---

## Contents

| Document | Audience | Purpose |
|----------|----------|---------|
| [Quick Start Guide](quick_start.md) | IT Engineers | Clone, build, install, verify, and run the first agent |
| [Production Deployment Guide](production_deployment.md) | Sysadmins | systemd services, env file, directory layout, log locations, restart behavior |
| [Network & Security Guide](network_security.md) | Network Security / IT | Ports, firewall rules, TLS, JWT, key management, hardening checklist |
| [SLA](sla.md) | Operations / Management | Availability targets, recovery guarantees, performance baselines, failure definitions |
| [Pilot Feedback Framework](pilot_feedback_framework.md) | Pilot Participants | Structured feedback format, incident reporting, log collection |
| [Patch Strategy](patch_strategy.md) | Engineers / Release Managers | v1.2.x scope rules, release discipline, upgrade procedure |

---

## System Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                        APOLLO v1.2 Stack                            │
├─────────────────┬──────────────────┬──────────────────┬────────────┤
│  apollo-node    │   apollo-hub     │   apollo (CLI)   │  webhooks  │
│  Execution      │  Fleet           │  Operator        │  Outbound  │
│  Engine         │  Coordinator     │  Shell           │  Events    │
│  :8080 / :8443  │  :9191           │  (interactive)   │  HMAC-SHA2 │
├─────────────────┴──────────────────┴──────────────────┴────────────┤
│  TLS via rustls · JWT + X-Apollo-Key auth · 100 RPS rate limiter   │
│  Per-tenant secrets (0600) · Usage metering · Persistent volumes   │
│  Multi-region routing · Auto-scale webhook · Agent catalog          │
├─────────────────────────────────────────────────────────────────────┤
│  apollo doctor — Acceptance Validator                               │
│  CHECKSUMS.sha256 — Binary Integrity                                │
│  .apollo/events.jsonl — Audit Log (Append-Only)                     │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Acceptance Standard

A deployment is production-certified when:

```bash
apollo doctor
```

returns `PRODUCTION READY` with all checks `OK`.

This command must be run after every installation, upgrade, or configuration change.

---

## Binary Integrity

The v1.2 release ships with a `CHECKSUMS.sha256` manifest. Verify before installation:

```bash
shasum -a 256 -c CHECKSUMS.sha256
```

Both `target/release/apollo` and `target/release/apollo-hub` must return `OK`.

---

## What's New in v1.2

| Feature | Benefit |
|---------|---------|
| TLS / HTTPS (native) | Encrypted node API without a reverse proxy |
| JWT authentication | Scoped tokens for sub-operator delegation |
| Per-tenant secret injection | OPENAI\_KEY, TELEGRAM\_TOKEN, etc. injected securely at spawn |
| Usage metering | CPU-seconds + memory-GB-seconds per tenant; billing reset API |
| Persistent volumes | Agent data survives restarts, rollbacks, and upgrades |
| Outbound webhook events | AGENT\_START, AGENT\_STOP, CAPACITY\_WARNING, SCALE\_NEEDED |
| Multi-region routing | `/nodes/best?region=X` + `/regions` capacity breakdown |
| Auto-scale webhook | Hub fires at configurable fleet utilization threshold |
| Rate limiting | 100 RPS per key; 429 on breach |
| Agent versioning + rollback | Backup on re-register; `POST /agents/rollback` |
| URL/git agent sourcing | Register from HTTPS archive or git repo |
| Runtime auto-provisioning | Missing runtime downloaded automatically from `agent.yaml` |

---

*APOLLO v1.2 is a production release. Architecture and API contracts are stable. Bug fixes are issued as v1.2.x patches under the rules defined in the [Patch Strategy](patch_strategy.md).*
