# Configurable Paths and Isolation

COSMIC BWarden allows you to customize the paths for its IPC socket, SSH agent socket, and configuration file. This is useful for running multiple instances, testing, or advanced system configurations.

## Command Line Arguments

All binaries (`cosmic-bwarden-agent`, `cosmic-bwarden-cli`, and `cosmic-bwarden-ui`) support the following flags:

- `--config <PATH>`: Use a specific configuration file.
- `--socket <PATH>`: Use a specific Unix socket for IPC.

The agent also supports:
- `--ssh-socket <PATH>`: Use a specific Unix socket for the SSH agent protocol.

## Environment Variables

You can also set these paths via environment variables:

- `COSMIC_BWARDEN_CONFIG`: Path to the configuration file.
- `COSMIC_BWARDEN_SOCKET`: Path to the main IPC socket.
- `COSMIC_BWARDEN_SSH_SOCKET`: Path to the SSH agent socket.

## Configuration File Settings

The `config.json` file supports these keys:

```json
{
  "socket_path": "/path/to/socket",
  "ssh_agent_socket_path": "/path/to/ssh-socket"
}
```

## Priority Order

The application resolves paths in the following order (highest to lowest priority):

1. **Command Line Argument** (`--socket`)
2. **Environment Variable** (`COSMIC_BWARDEN_SOCKET`)
3. **Configuration File** (`socket_path` in `config.json`)
4. **Default System Path** (usually in `XDG_RUNTIME_DIR`)

## Testing Isolation

These features are used by the E2E test suite to ensure that every test run is completely isolated from the user's daily client. Each test starts an agent on a unique socket in a temporary directory, preventing data corruption or interference.
