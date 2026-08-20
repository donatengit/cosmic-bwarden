#!/usr/bin/env sh
# Ensure a container socket for the E2E harness. Order: explicit DOCKER_HOST,
# then the Docker socket, then the rootful podman socket, then the podman user
# socket (started on demand via systemd socket activation). Exits 1 with
# instructions when none is available.
set -e

if [ -n "$DOCKER_HOST" ]; then
    echo "Using container socket: $DOCKER_HOST"
    exit 0
fi

if [ -S /var/run/docker.sock ]; then
    echo "Using Docker socket /var/run/docker.sock"
    exit 0
fi

if [ -S /run/podman/podman.sock ]; then
    echo "Using podman socket /run/podman/podman.sock"
    exit 0
fi

runtime_dir="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
if [ -S "$runtime_dir/podman/podman.sock" ]; then
    echo "Using podman user socket $runtime_dir/podman/podman.sock"
    exit 0
fi

# No socket yet: try systemd socket activation (podman, the preferred runtime).
systemctl --user start podman.socket 2>/dev/null || true
if [ -S "$runtime_dir/podman/podman.sock" ]; then
    echo "Started podman user socket via systemd"
    exit 0
fi

echo "ERROR: no container socket. Start one with: systemctl --user start podman.socket (podman, preferred) or launch the Docker daemon." >&2
exit 1
