import subprocess
import time
import requests
import os
import json
import random
import signal
import psutil
import uuid
from datetime import datetime

NODE_BINARY = "./target/debug/apollo-node"
HUB_BINARY = "./target/debug/apollo-hub"
BASE_PORT = 8500
HUB_PORT = 9500
SECRET = "CERTIFICATION_KEY"
NODE_COUNT = 10
SIMULATION_HOURS = 24

nodes = {} # port -> subprocess.Popen
anomalies = []

def log_event(msg, correlation_id=None):
    timestamp = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    cid_str = f" | CID: {correlation_id}" if correlation_id else ""
    formatted = f"[{timestamp}] {msg}{cid_str}"
    print(formatted)
    with open("certification_log.txt", "a") as f:
        f.write(formatted + "\n")

def emit_spine_event(action, message, level="INFO", correlation_id=None):
    event = {
        "timestamp": int(time.time()),
        "node_id": "chaos-engine",
        "level": level,
        "category": "LIFECYCLE",
        "action": action,
        "message": message,
        "correlation_id": correlation_id
    }
    with open(".apollo/events.jsonl", "a") as ef:
        ef.write(json.dumps(event) + "\n")

def cleanup_all():
    log_event("🛑 Cleaning up APOLLO processes...")
    for p in nodes.values():
        try: p.kill()
        except: pass
    for proc in psutil.process_iter(['name', 'environ']):
        try:
            if proc.info['name'] == 'apollo-node' or (proc.info['environ'] and 'APOLLO_TENANT_ID' in proc.info['environ']):
                proc.kill()
        except: pass

def start_node(i):
    port = BASE_PORT + i
    base_dir = f".apollo/certification/node_{i}"
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
    except: pass
    return port

def run_causal_chaos():
    cleanup_all()
    log_event(f"🚀 INITIATING CAUSAL CHAOS VALIDATION ({SIMULATION_HOURS}h)...")
    
    for i in range(NODE_COUNT):
        start_node(i)
        subprocess.run([HUB_BINARY, "add", "--ip", f"127.0.0.1:{BASE_PORT+i}", "--key", SECRET, "--name", f"node-{i}"], stdout=subprocess.DEVNULL)

    hub_proc = subprocess.Popen([HUB_BINARY, "start", "--listen", f"0.0.0.0:{HUB_PORT}"], stdout=subprocess.DEVNULL)
    
    start_time = time.time()
    end_time = start_time + (SIMULATION_HOURS * 3600)
    
    try:
        while time.time() < end_time:
            correlation_id = str(uuid.uuid4())[:8]
            
            # 1. Spawn load with CID
            target = random.randint(0, NODE_COUNT - 1)
            requests.post(f"http://127.0.0.1:{BASE_PORT+target}/agents/run", 
                          headers={"X-Mars-Key": SECRET, "X-Mars-Correlation-Id": correlation_id}, 
                          json={"agent": "openclaw", "tenant": "cert-user"}, timeout=5)

            # 2. Chaos with CID
            dice = random.random()
            if dice < 0.1:
                log_event(f"🌪️  STORM: Killing 3 nodes", correlation_id)
                emit_spine_event("CHAOS_STORM", "Forced 3-node crash", "FATAL", correlation_id)
                for _ in range(3):
                    idx = random.randint(0, NODE_COUNT - 1)
                    nodes[BASE_PORT + idx].kill()
            
            # Auto-restart
            for i in range(NODE_COUNT):
                if nodes[BASE_PORT + i].poll() is not None:
                    start_node(i)

            time.sleep(20)
            
    except KeyboardInterrupt: pass
    finally:
        hub_proc.terminate()
        cleanup_all()

if __name__ == "__main__":
    run_causal_chaos()
