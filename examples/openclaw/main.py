import os
import sys
import time

tenant_id = os.getenv("APOLLO_TENANT_ID", "unknown")
agent_name = os.getenv("APOLLO_AGENT_NAME", "unknown")
workspace = os.getenv("APOLLO_WORKSPACE", ".")

print(f"--- Apollo Agent Startup ---")
print(f"Agent: {agent_name}")
print(f"Tenant: {tenant_id}")
print(f"Workspace: {workspace}")
print(f"--------------------------")

# Simulate some work
while True:
    print(f"[{time.ctime()}] Agent {agent_name} for tenant {tenant_id} is running...")
    time.sleep(10)
