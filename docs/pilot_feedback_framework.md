# APOLLO v1.2 — Pilot Feedback Framework

**Audience:** IT Engineers / Pilot Participants
**Classification:** Pilot Operations
**Version:** v1.2

---

## Purpose

This framework defines how pilot participants report operational experience with APOLLO v1.2. Structured feedback accelerates issue resolution and ensures that v1.2.x patches address real-world deployment conditions.

---

## 1. Feedback Categories

All reports must identify which category applies.

| Category | Code | Scope |
|----------|------|-------|
| **Stability** | `STAB` | Crashes, unexpected restarts, agent failures, data loss |
| **Performance** | `PERF` | Latency regressions, throughput degradation, resource exhaustion, metering inaccuracy |
| **Installation Friction** | `INST` | Install script failures, build errors, binary verification failures, TLS configuration issues |
| **Security Concerns** | `SEC` | Authentication bypasses, isolation failures, secrets leaking into logs or API responses, key handling issues, webhook signature failures |
| **Operational Complexity** | `OPS` | Unclear procedures, missing documentation, systemd integration issues, observability gaps, webhook delivery problems |

---

## 2. Required Feedback Format

Submit via your designated feedback channel (see Section 3).

```json
{
  "provider": "Company or team name",
  "environment": "OS, kernel version, hardware specs (e.g. Ubuntu 22.04, 6.5 kernel, 8-core/16GB/NVMe)",
  "apollo_version": "v1.2",
  "tls_enabled": true,
  "jwt_enabled": true,
  "webhook_configured": true,
  "node_count": 3,
  "region_count": 2,
  "peak_tenant_count": 500,
  "category": "STAB | PERF | INST | SEC | OPS",
  "issues": [
    {
      "id": "001",
      "category": "STAB | PERF | INST | SEC | OPS",
      "severity": "critical | high | medium | low",
      "title": "One-line description",
      "description": "Full description of what happened, what was expected, and what occurred instead",
      "steps_to_reproduce": [
        "Step 1",
        "Step 2",
        "Step 3"
      ],
      "frequency": "always | intermittent | once",
      "logs_attached": true
    }
  ],
  "performance_metrics": {
    "avg_api_response_ms": 0,
    "p99_api_response_ms": 0,
    "active_agents_at_peak": 0,
    "node_restarts_per_day": 0,
    "webhook_delivery_failures_per_day": 0,
    "metering_accuracy_delta_percent": 0.0,
    "uptime_percent_30d": 0.0
  },
  "stability_rating": 8,
  "installation_rating": 9,
  "operational_complexity_rating": 7,
  "tls_setup_rating": 8,
  "webhook_reliability_rating": 9,
  "comments": "Free-form observations, suggestions, or context that does not fit the fields above"
}
```

**Rating scale:** 1 = unacceptable, 10 = production-grade with no friction.

---

## 3. Incident Reporting Flow

### Step 1 — Classify Severity

| Severity | Definition | Response Target |
|----------|------------|----------------|
| **Critical** | Data loss, security breach, secrets exposed in logs/API, isolation failure, unrecoverable node/hub failure, `apollo doctor` fails after restart | Same business day |
| **High** | Recurring crashes, performance below SLA baselines, installation failure on supported OS, webhook delivery consistently failing | 2 business days |
| **Medium** | Intermittent issues, partial degradation, metering inaccuracy, missing documentation | 5 business days |
| **Low** | Minor friction, cosmetic issues, suggestions | Next patch cycle |

### Step 2 — Collect Required Logs

**For node issues:**

```bash
# System journal for the past hour
journalctl -u apollo-node --since "1 hour ago" > apollo-node.log

# Audit event log (last 500 events)
tail -500 /var/lib/apollo/.apollo/events.jsonl > events_recent.jsonl

# Doctor output
apollo doctor > doctor_output.txt

# Usage state for affected tenant
cat /var/lib/apollo/.apollo/usage/<tenant_id>.json > usage_snapshot.json

# System metrics at time of failure
top -bn1 > system_top.txt
free -h >> system_top.txt
df -h >> system_top.txt
```

**For hub issues:**

```bash
journalctl -u apollo-hub --since "1 hour ago" > apollo-hub.log
cat /var/lib/apollo/.apollo/hub_nodes.json > hub_registry.json
curl http://localhost:9191/summary > hub_summary.json
curl http://localhost:9191/regions > hub_regions.json
```

**For TLS / auth issues:**

```bash
# Test both auth methods
curl -v -H "X-Apollo-Key: test-key" https://localhost:8443/health 2>&1 > tls_test.txt
openssl x509 -in /etc/apollo/node.crt -noout -text >> tls_test.txt
```

**For webhook issues:**

```bash
# Capture webhook delivery attempts in events log
grep "webhook" /var/lib/apollo/.apollo/events.jsonl > webhook_events.jsonl

# Check node journal for delivery errors
journalctl -u apollo-node --since "1 hour ago" | grep -i webhook > webhook_log.txt
```

**For installation issues:**

```bash
cargo build --release 2>&1 > build_output.txt
uname -a > env_info.txt
rustc --version >> env_info.txt
cargo --version >> env_info.txt
openssl version >> env_info.txt
```

### Step 3 — Submit the Report

Provide the completed JSON template (Section 2) along with all log files to your designated contact. For critical severity issues, include `[CRITICAL]` in the subject line.

### What Constitutes a Critical Incident

A critical incident requires immediate escalation if any of the following are true:

- `apollo doctor` returns a non-`OK` result after a restart on a previously certified installation
- A tenant's secrets (`OPENAI_KEY`, etc.) appear in logs, API responses, or another tenant's environment
- A tenant can access another tenant's workspace or processes (isolation failure)
- An agent process persists after `DELETE /agents/stop` (orphan escape)
- `events.jsonl` contains data inconsistent with the actual system state
- A secret key (`X-Apollo-Key`) is accepted without being in the configured key list
- A JWT with an expired `exp` claim is accepted
- The node crashes more than 5 times within 60 seconds on hardware meeting minimum specs
- Webhook signature verification produces a false positive (unsigned payload accepted as signed)

---

## 4. Feedback Cycle

| Phase | Activity |
|-------|---------|
| Week 1–2 | Installation, TLS setup, and initial validation; `INST` and `OPS` feedback prioritized |
| Week 3–4 | Production load testing with real tenant traffic; `PERF`, `STAB`, and `SEC` feedback collected |
| Week 5–6 | Webhook integration, billing metering validation, multi-region routing testing |
| End of pilot | Final JSON report + all logs submitted |

Patch releases (v1.2.x) will be issued in response to confirmed `critical` and `high` severity issues. See [Patch Strategy](patch_strategy.md) for what qualifies for a patch.
