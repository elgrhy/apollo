# APOLLO v1.0 — Enterprise Handoff Pack

**Status:** Production Certified  
**Version:** v1.0 (Frozen)  
**Distribution:** Private — IT Teams and Infrastructure Providers Only

---

## Purpose

This pack enables any qualified IT team to deploy, operate, monitor, and maintain APOLLO v1.0 without developer involvement. All documents are production-ready and self-contained.

---

## Contents

| Document | Audience | Purpose |
|:---|:---|:---|
| [Quick Start Guide](quick_start.md) | IT Engineers | Clone, build, install, verify, and run the first agent |
| [Production Deployment Guide](production_deployment.md) | Sysadmins | systemd services, directory structure, log locations, restart behavior |
| [Network & Security Guide](network_security.md) | Network Security / IT | Ports, firewall rules, X-Apollo-Key usage, security model |
| [SLA](sla.md) | Operations / Management | Uptime targets, recovery guarantees, performance baselines, failure definitions |
| [Pilot Feedback Framework](pilot_feedback_framework.md) | Pilot Participants | Structured feedback format, incident reporting, log collection |
| [Patch Strategy](patch_strategy.md) | Engineers / Release Managers | v1.0.x scope rules, release discipline, upgrade procedure |

---

## System Overview

```
┌──────────────────────────────────────────────────────────┐
│                    APOLLO v1.0 Stack                     │
├──────────────────┬───────────────────┬───────────────────┤
│   apollo-node    │   apollo-hub      │   apollo (CLI)    │
│  Execution Engine│  Fleet Coordinator│  Operator Shell   │
│  Port: 8080      │  Port: 9090       │  (interactive)    │
├──────────────────┴───────────────────┴───────────────────┤
│  apollo doctor — Acceptance Validator                    │
│  CHECKSUMS.sha256 — Binary Integrity                     │
│  .apollo/events.jsonl — Audit Log (Append-Only)          │
└──────────────────────────────────────────────────────────┘
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

The v1.0 release ships with a `CHECKSUMS.sha256` manifest. Verify before installation:

```bash
shasum -a 256 -c CHECKSUMS.sha256
```

Both `target/release/apollo` and `target/release/apollo-hub` must return `OK`.

---

*APOLLO v1.0 is a frozen production release. Architecture, API contracts, and behavior are stable. Bug fixes are issued as v1.0.x patches under the rules defined in the [Patch Strategy](patch_strategy.md).*
