#!/usr/bin/env python3
"""Register the COSMIC BWarden native messaging host for Firefox.

Called by `just install` with --agent-path and --home to point at the installed
binary. Called without arguments during development to point at target/debug/.
"""
import argparse
import os
import sys
import json

HOST_NAME = "com.8bit.cosmic_bwarden"
DESCRIPTION = "COSMIC BWarden Native Messaging Host"
ALLOWED_EXTENSIONS = ["cosmic-bwarden@8bit.com"]


def register_firefox(agent_path: str, home: str) -> None:
    manifest_dir = os.path.join(home, ".mozilla", "native-messaging-hosts")
    os.makedirs(manifest_dir, exist_ok=True)

    # Native Messaging manifests don't support arguments in 'path', so we use
    # a minimal wrapper script to pass the browser-host subcommand.
    wrapper_path = os.path.join(manifest_dir, "cosmic-bwarden-browser-host.sh")
    with open(wrapper_path, "w") as f:
        f.write("#!/bin/sh\n")
        f.write(f'exec "{agent_path}" browser-host\n')
    os.chmod(wrapper_path, 0o755)

    manifest = {
        "name": HOST_NAME,
        "description": DESCRIPTION,
        "path": wrapper_path,
        "type": "stdio",
        "allowed_extensions": ALLOWED_EXTENSIONS,
    }
    manifest_path = os.path.join(manifest_dir, f"{HOST_NAME}.json")
    with open(manifest_path, "w") as f:
        json.dump(manifest, f, indent=2)

    print(f"Registered Firefox native messaging host at {manifest_path}")
    print(f"  agent: {agent_path}")


if __name__ == "__main__":
    if sys.platform != "linux":
        print("This script currently only supports Linux.")
        sys.exit(1)

    project_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    default_agent = os.path.join(project_root, "target", "debug", "cosmic-bwarden-agent")

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--agent-path",
        default=default_agent,
        help="Path to cosmic-bwarden-agent binary (default: target/debug/)",
    )
    parser.add_argument(
        "--home",
        default=os.path.expanduser("~"),
        help="User home directory for manifest installation (default: ~)",
    )
    args = parser.parse_args()

    if not os.path.exists(args.agent_path):
        print(f"Error: Agent binary not found at {args.agent_path}")
        if args.agent_path == default_agent:
            print("Build it first: cargo build -p cosmic-bwarden-agent")
        sys.exit(1)

    register_firefox(args.agent_path, args.home)
