import subprocess
import time
import requests
import os
import json
import random

NODE_BINARY = "./target/debug/apollo-node"
HUB_BINARY = "./target/debug/apollo-hub"
BASE_PORT = 8100
HUB_PORT = 9100
SECRET = "STRESS_TEST_KEY"
NODE_COUNT = 10
AGENTS_PER_NODE = 5

processes = []

def start_nodes():
    print(f"🚀 Spawning {NODE_COUNT} APOLLO Nodes...")
    for i in range(NODE_COUNT):
        port = BASE_PORT + i
        base_dir = f".apollo/stress/node_{i}"
        os.makedirs(base_dir, exist_ok=True)
        
        proc = subprocess.Popen([
            NODE_BINARY, "node", "start",
            "--listen", f"127.0.0.1:{port}",
            "--base-dir", base_dir,
            "--secret-keys", SECRET
        ], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        processes.append(proc)
        print(f"  - Node {i} started on port {port}")

def start_hub():
    print(f"🧠 Spawning APOLLO Hub...")
    # Register nodes first
    for i in range(NODE_COUNT):
        port = BASE_PORT + i
        subprocess.run([
            HUB_BINARY, "add",
            "--ip", f"127.0.0.1:{port}",
            "--key", SECRET,
            "--name", f"node-{i}"
        ], stdout=subprocess.DEVNULL)
    
    proc = subprocess.Popen([
        HUB_BINARY, "start",
        "--listen", f"0.0.0.0:{HUB_PORT}"
    ], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    processes.append(proc)
    print(f"  - Hub started on port {HUB_PORT}")

def run_simulation():
    print(f"🔥 Running Stress Simulation...")
    time.sleep(5) # Wait for startup
    
    for i in range(20):
        try:
            resp = requests.get(f"http://localhost:{HUB_PORT}/status")
            nodes = resp.json()
            online = sum(1 for n in nodes if n['status']['is_online'])
            print(f"Iteration {i}: Nodes Online: {online}/{NODE_COUNT}")
            
            if i == 5:
                print("💣 Injecting Chaos: Killing Node 3...")
                processes[3].terminate()
            
            if i == 10:
                print("♻️  Verifying recovery...")
                
        except Exception as e:
            print(f"Error: {e}")
        time.sleep(5)

def cleanup():
    print("🛑 Cleaning up...")
    for p in processes:
        p.terminate()
    print("Done.")

if __name__ == "__main__":
    try:
        start_nodes()
        start_hub()
        run_simulation()
    finally:
        cleanup()
