#!/usr/bin/env python3
import os
import sys
import json
import shutil

HOST_NAME = "com.8bit.cosmic_bwarden"
DESCRIPTION = "COSMIC BWarden Native Messaging Host"
# Replace this with the actual extension ID if/when we have a permanent one
ALLOWED_EXTENSIONS = ["cosmic-bwarden@8bit.com"] 

def register_firefox():
    home = os.path.expanduser("~")
    manifest_dir = os.path.join(home, ".mozilla", "native-messaging-hosts")
    os.makedirs(manifest_dir, exist_ok=True)
    
    # We need the absolute path to the agent binary
    # For development, we can point to target/debug/cosmic-bwarden-agent
    project_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    agent_path = os.path.join(project_root, "target", "debug", "cosmic-bwarden-agent")
    
    if not os.path.exists(agent_path):
        print(f"Error: Agent binary not found at {agent_path}")
        print("Please build the project first: cargo build -p cosmic-bwarden-agent")
        return

    # Create a wrapper script because Native Messaging doesn't support arguments in 'path'
    wrapper_path = os.path.join(manifest_dir, "cosmic-bwarden-browser-host.sh")
    with open(wrapper_path, "w") as f:
        f.write("#!/bin/bash\n")
        f.write(f'"{agent_path}" browser-host "$@"\n')
    os.chmod(wrapper_path, 0o755)

    manifest = {
        "name": HOST_NAME,
        "description": DESCRIPTION,
        "path": wrapper_path,
        "type": "stdio",
        "allowed_extensions": ALLOWED_EXTENSIONS
    }

    manifest_path = os.path.join(manifest_dir, f"{HOST_NAME}.json")
    with open(manifest_path, "w") as f:
        json.dump(manifest, f, indent=2)
    
    print(f"Registered Firefox native messaging host at {manifest_path}")

if __name__ == "__main__":
    if sys.platform != "linux":
        print("This script currently only supports Linux.")
        sys.exit(1)
        
    register_firefox()
