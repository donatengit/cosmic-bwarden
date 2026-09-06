#!/bin/bash
# run_vaultwarden.sh - Helper script to run Vaultwarden in Docker with sudo

# Check if sudo is available
if ! command -v sudo &> /dev/null; then
    echo "Error: sudo is not installed. Please run this script with root privileges or install sudo."
    exit 1
fi

# Container name
CONTAINER_NAME="corbw-vaultwarden-test"

# Stop and remove existing container if it exists
echo "Cleaning up old container..."
docker stop $CONTAINER_NAME &> /dev/null
docker rm $CONTAINER_NAME &> /dev/null

# Start Vaultwarden
# Map to port 8080 by default
PORT=${1:-8080}
echo "Starting Vaultwarden on port $PORT..."

# --pids-limit 2048: some podman builds (this repo's preferred runtime, used
# rootless via the user socket) default to pids.max=1 per container, which
# lets the init process exist but nothing else — Vaultwarden's tokio runtime
# then dies with "OS can't spawn worker thread". 2048 is far beyond what a
# single Rust process needs and is harmless under a regular Docker daemon.
# Keep the container off the developer's cores for the length of the run. These
# match the caps the Rust harness applies (container_limits.rs); a container is
# not covered by the systemd scope `just` puts around the test command, because
# podman places it in a sibling cgroup. See docs/testing.md.
CONTAINER_CPUS=2
CONTAINER_MEM_MB=1024
LIMIT_ARGS=(--cpus "$CONTAINER_CPUS" --memory "${CONTAINER_MEM_MB}m")

docker run -d \
    --name $CONTAINER_NAME \
    --pids-limit 2048 \
    "${LIMIT_ARGS[@]}" \
    -e SIGNUPS_ALLOWED=true \
    -e I_REALLY_WANT_VOLATILE_STORAGE=true \
    -p $PORT:80 \
    vaultwarden/server:latest

# Wait for it to be ready
echo "Waiting for Vaultwarden to initialize..."
for i in {1..30}; do
    if docker logs $CONTAINER_NAME 2>&1 | grep -q "Running on http://0.0.0.0:80"; then
        echo "Vaultwarden is ready at http://localhost:$PORT"
        exit 0
    fi
    sleep 1
done

echo "Timeout waiting for Vaultwarden to start. Check 'docker logs $CONTAINER_NAME'"
exit 1
