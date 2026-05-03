# APOLLO v1.0 — Service Level Agreement

**Version:** v1.0  
**Classification:** Enterprise Operations  
**Effective Date:** Upon deployment of v1.0 certified release

---

## Scope

This SLA applies to self-hosted APOLLO v1.0 deployments operating on infrastructure that meets the prerequisites defined in the [Quick Start Guide](quick_start.md). Targets represent the designed behavior of the APOLLO runtime under normal operating conditions on hardware meeting minimum specifications.

---

## 2.1 Availability Targets

### Node Uptime

| Metric | Target | Notes |
|:---|:---|:---|
| Node process availability | 99.5% | Measured per calendar month |
| Time to automatic restart after crash | < 10 seconds | systemd `Restart=on-failure`, `RestartSec=5s` |
| Node API responsiveness (p99) | < 200 ms | Under normal agent density |
| Max consecutive restart failures before alert | 5 in 60 seconds | systemd limit; manual intervention required after threshold |

### Hub Monitoring

| Metric | Target | Notes |
|:---|:---|:---|
| Hub process availability | 99% | Measured per calendar month |
| Node health poll interval | ≤ 30 seconds | Passive polling of `/metrics` on each registered node |
| Node status propagation delay | ≤ 60 seconds | Time from node failure to hub marking node `OFFLINE` |

### Degraded State Handling

A node enters **DEGRADED** state when hub health polling fails but the node process is still running (e.g., network partition). In DEGRADED state:

- The node continues executing existing agents independently.
- No new agents are routed to the node by the hub.
- Existing agents are not interrupted.
- Upon network restoration, the node automatically returns to `ONLINE` status.

---

## 2.2 Recovery Guarantees

### Restart Recovery Time

| Event | Recovery Behavior | Time |
|:---|:---|:---|
| Node process crash | systemd restarts the node daemon | < 10 seconds |
| Node host reboot | Node daemon starts on boot (systemd enabled) | < 60 seconds |
| Orphan process cleanup on restart | Pre-flight sweep terminates all prior agent process groups | < 30 seconds |
| Node API ready after restart | API begins accepting requests after orphan cleanup | < 30 seconds total |

### Orphan Cleanup Guarantees

On every startup, the node daemon performs a deterministic orphan sweep:

1. All process groups associated with the prior session are identified.
2. `killpg` is issued to each group.
3. The node confirms no prior agent PIDs are active.
4. The API begins accepting requests only after cleanup completes.

**Result:** No manual process cleanup is required after a crash or hard reboot. Zombie processes do not accumulate across restarts.

### State Consistency Guarantees

| State Type | Guarantee |
|:---|:---|
| Audit log (`events.jsonl`) | Append-only; survives restarts intact |
| Hub node registry (`hub_nodes.json`) | Persisted to disk atomically; survives hub restarts |
| In-flight agent runs | Not persisted across node crashes; callers must re-issue run requests |
| Tenant workspace files | Preserved across restarts; not deleted on node crash |

> In-flight agents that were executing at the time of a crash must be restarted by the calling system. The runtime does not attempt to re-launch agents from pre-crash state.

---

## 2.3 Performance Baselines

All baselines measured on: 4-core x86_64, 8 GB RAM, SSD storage, Ubuntu 22.04.

| Metric | Baseline | Notes |
|:---|:---|:---|
| API response time (p50) | < 50 ms | `POST /agents/run`, `DELETE /agents/stop` |
| API response time (p99) | < 200 ms | Under load with 40+ active agents |
| Max agent density per node | 50 agents | Default `--max-agents 50`; configurable at startup |
| Agent process startup latency | < 2 seconds | Time from API call to process running |
| Agent process stop latency | < 1 second | Full process group termination via `killpg` |
| Hub status poll cycle | ≤ 30 seconds | Per registered node |
| Acceptable node-to-hub latency | < 500 ms | Round-trip for `/metrics` poll |

---

## 2.4 Failure Definitions

### Node Failure

A **node failure** is defined as any condition in which the node daemon process is no longer running and the node API is unreachable. This includes:

- Process crash (unhandled panic, OOM kill)
- Host shutdown or reboot
- Forced process termination by the OS

**Detection:** The hub marks a node `OFFLINE` after two consecutive failed health polls.

**Recovery:** Automatic via systemd. No operator action required unless the node exceeds the restart burst limit (5 restarts in 60 seconds), which indicates a persistent fault requiring investigation.

---

### Hub Failure

A **hub failure** is defined as any condition in which the hub daemon process is no longer running and the hub API is unreachable.

**Impact:** Node discovery and fleet-wide routing are unavailable. Individual nodes continue operating independently. Existing agents continue running without interruption.

**Recovery:** Automatic via systemd. Node state is not lost during hub downtime. On hub restart, nodes re-register or are re-polled according to the stored registry.

---

### Degraded Node

A **degraded node** is a node process that is running and serving agents but is unreachable by the hub (e.g., due to a network partition).

**Impact:** The hub cannot route new agents to the degraded node. The node itself continues normal operation.

**Recovery:** Automatic when network connectivity is restored. No operator action required.

---

### Recovery Success

A recovery is considered **successful** when:

1. `apollo doctor` returns `PRODUCTION READY` on the affected node.
2. The node API responds to `GET /metrics` with correct data.
3. The hub marks the node `ONLINE` within one poll cycle (≤ 30 seconds).
4. No orphan processes remain from the prior session (verified by absence of prior agent PIDs).

---

*See [Patch Strategy](patch_strategy.md) for how failures in v1.0 are addressed via point releases.*
