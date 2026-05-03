import os
import sys
import time

tenant_id = os.getenv("MARS_TENANT_ID", "unknown")
agent_name = os.getenv("MARS_AGENT_NAME", "unknown")
workspace = os.getenv("MARS_WORKSPACE", ".")

print(f"--- MARS Agent Startup ---")
print(f"Agent: {agent_name}")
print(f"Tenant: {tenant_id}")
print(f"Workspace: {workspace}")
print(f"--------------------------")

# Simulate some work
while True:
    print(f"[{time.ctime()}] Agent {agent_name} for tenant {tenant_id} is running...")
    time.sleep(10)
