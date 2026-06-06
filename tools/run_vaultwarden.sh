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

docker run -d \
    --name $CONTAINER_NAME \
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
