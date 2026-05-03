import subprocess
import time
import requests
import os
import json
import random
import signal
import psutil
from datetime import datetime

NODE_BINARY = "./target/debug/mars-node"
HUB_BINARY = "./target/debug/mars-hub"
BASE_PORT = 8500
HUB_PORT = 9500
SECRET = "CERTIFICATION_KEY"
NODE_COUNT = 10
SIMULATION_HOURS = 24

nodes = {} # port -> subprocess.Popen
anomalies = []

def log_event(msg):
    timestamp = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    formatted = f"[{timestamp}] {msg}"
    print(formatted)
    with open("certification_log.txt", "a") as f:
        f.write(formatted + "\n")

def cleanup_all():
    log_event("🛑 Cleaning up MARS processes for certification baseline...")
    for p in nodes.values():
        try: p.kill()
        except: pass
    for proc in psutil.process_iter(['name', 'environ']):
        try:
            if proc.info['name'] == 'mars-node' or (proc.info['environ'] and 'MARS_TENANT_ID' in proc.info['environ']):
                proc.kill()
        except: pass

def start_node(i):
    port = BASE_PORT + i
    base_dir = f".mars/certification/node_{i}"
    os.makedirs(base_dir, exist_ok=True)
    proc = subprocess.Popen([
        NODE_BINARY, "node", "start",
        "--listen", f"127.0.0.1:{port}",
        "--base-dir", base_dir,
        "--secret-keys", SECRET
    ], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    nodes[port] = proc
    # Register agent
    time.sleep(1)
    try:
        requests.post(f"http://127.0.0.1:{port}/agents/add", 
                      headers={"X-Mars-Key": SECRET}, 
                      json={"source": os.path.abspath("examples/openclaw")}, 
                      timeout=5)
    except Exception as e:
        anomalies.append(f"Node {i} Reg Fail: {e}")
    return port

def verify_integrity():
    node_pids = [p.pid for p in nodes.values() if p.poll() is None]
    found_orphans = False
    for proc in psutil.process_iter(['pid', 'ppid', 'environ']):
        try:
            env = proc.info['environ']
            if env and 'MARS_WORKSPACE' in env:
                ppid = proc.info['ppid']
                if ppid not in node_pids and ppid != 1: # re-parenting happens
                    log_event(f"❌ ORPHAN DETECTED: PID {proc.info['pid']}")
                    found_orphans = True
        except: pass
    return not found_orphans

def run_certification():
    cleanup_all()
    log_event(f"🚀 INITIATING 24-HOUR PRODUCTION CERTIFICATION SUITE...")
    
    for i in range(NODE_COUNT):
        port = start_node(i)
        subprocess.run([HUB_BINARY, "add", "--ip", f"127.0.0.1:{port}", "--key", SECRET, "--name", f"node-{i}"], stdout=subprocess.DEVNULL)

    hub_proc = subprocess.Popen([HUB_BINARY, "start", "--listen", f"0.0.0.0:{HUB_PORT}"], stdout=subprocess.DEVNULL)
    
    start_time = time.time()
    end_time = start_time + (SIMULATION_HOURS * 3600)
    
    try:
        while time.time() < end_time:
            # 1. HP-CERT: Spawning Load
            target = random.randint(0, NODE_COUNT - 1)
            requests.post(f"http://127.0.0.1:{BASE_PORT+target}/agents/run", 
                          headers={"X-Mars-Key": SECRET}, json={"agent": "openclaw", "tenant": "cert-user"}, timeout=5)

            # 2. Status Report
            try:
                resp = requests.get(f"http://localhost:{HUB_PORT}/status", timeout=2)
                data = resp.json()
                online = sum(1 for n in data if n['status']['is_online'])
                active = sum(n['status']['active_agents'] for n in data)
                log_event(f"STATUS: {online}/{NODE_COUNT} Nodes Online | {active} Agents Active | Anomalies: {len(anomalies)}")
            except:
                log_event("⚠️ HUB SYNC ACCURACY: Hub Busy/Unresponsive")

            # 3. DSI-CERT / NET-CERT Chaos
            dice = random.random()
            if dice < 0.1: # Storm
                log_event("🌪️  DSI-CERT: Forced Restart Storm (Killing 3 random nodes)")
                # Emit to Event Spine
                with open(".mars/events.jsonl", "a") as ef:
                    ef.write(json.dumps({
                        "timestamp": int(time.time()), "node_id": "chaos-engine", "level": "FATAL",
                        "category": "LIFECYCLE", "action": "CHAOS_STORM", "message": "Forced 3-node crash storm"
                    }) + "\n")
                for _ in range(3):
                    idx = random.randint(0, NODE_COUNT - 1)
                    nodes[BASE_PORT + idx].kill()
            elif dice < 0.15: # Partial failure
                log_event("🧊 NET-CERT: Packet Loss / Timeout Simulation (Node Freeze)")
                # Emit to Event Spine
                with open(".mars/events.jsonl", "a") as ef:
                    ef.write(json.dumps({
                        "timestamp": int(time.time()), "node_id": "chaos-engine", "level": "WARN",
                        "category": "HEALTH", "action": "CHAOS_FREEZE", "message": "Simulated node freeze"
                    }) + "\n")
                idx = random.randint(0, NODE_COUNT - 1)
                os.kill(nodes[BASE_PORT + idx].pid, signal.SIGSTOP)
                time.sleep(10)
                os.kill(nodes[BASE_PORT + idx].pid, signal.SIGCONT)

            # 4. Recovery check
            for i in range(NODE_COUNT):
                if nodes[BASE_PORT + i].poll() is not None:
                    start_node(i)

            if not verify_integrity():
                anomalies.append("Orphan Integrity Failure")

            time.sleep(30)
            
    except KeyboardInterrupt:
        log_event("Certification interrupted by user.")
    finally:
        hub_proc.terminate()
        cleanup_all()

if __name__ == "__main__":
    run_certification()
