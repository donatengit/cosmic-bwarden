# Cosmic BWarden Client: Context & Architecture

A secure, native COSMIC Bitwarden client featuring a background agent, tray applet, and flexible CLI.

## Core Architecture

The project follows a modular Rust-based architecture split into specialized crates:

- **`cosmic-bwarden-core`**: The foundational library. Contains API clients, database models, IPC protocols, and cryptographic primitives.
- **`cosmic-bwarden-agent`**: A secure background service. Manages memory-locked sensitive data, handles vault synchronization, and serves as the IPC coordinator. Implements decryption caching for high performance.
- **`cosmic-bwarden-cli`**: A feature-rich command-line interface with flexible keyword support and advanced filtering.
- **`cosmic-bwarden-ui`**: The main COSMIC-native graphical interface and tray applet. Supports multi-window authentication and full CRUD operations.
- **`cosmic-bwarden-tests`**: End-to-end integration tests using Docker and Vaultwarden.

## Technical Stack

- **Language**: Rust
- **UI Framework**: `libcosmic` 1.0.0 (MVU Architecture)
- **Networking**: `reqwest`, `tokio`
- **Security**: Memory-locked regions for secrets, AES-256-CBC, PBKDF2/Argon2id.
- **Testing**: `testcontainers-rs` (Vaultwarden).

## "Game Changing" Improvements

### 🚀 Performance & UI
- **Decryption Caching**: The agent caches decrypted names/usernames, enabling instant search even in huge vaults.
- **Multi-Window Flow**: Distinct compact `Auth` window vs full `Main` window workspace.
- **Intelligent Focus**: Prevents window fragmentation by focusing existing windows using `window::gain_focus`.
- **Compact UX**: Uses `autosize` and `Length::Shrink` for a professional, focused feel on sensitive dialogs.

### 🛡️ Security
- **Thin Client Invariant**: UI and CLI MUST NOT store plaintext secrets in long-lived memory. Use `Secret` wrappers and transient `.expose()`.
- **Safe Persistence**: `access_token` and `refresh_token` are marked `#[serde(skip)]` in `Db` to prevent plaintext leakage in JSON cache. Keyring persistence via `oo7` is the authorized long-term storage method.
- **Granular Reprompts**: Full enforcement of Master Password reprompting for sensitive items.
- **Reactive State**: The UI uses long-lived `Action::Subscribe` streams for agent-pushed events (`Locked`, `Unlocked`, `VaultChanged`) instead of polling.

### 🔄 Data Integrity
- **Real-Time CRUD Sync**: Every Add, Update, and Delete operation is immediately synchronized with the server.
- **Manual Sync**: Dedicated Sync button for on-demand refreshes.

## Key Workflows

### Authentication
1. **Registration/Login**: Communicates with Bitwarden/Vaultwarden APIs.
2. **Unlock**: The agent holds the master key in memory-locked storage.
3. **Multi-Window Interaction**: UI automatically switches to the workspace window only after successful unlock.

### CLI Interactions
- **Modern Syntax**: Uses `KEY=VALUE` pairs for adding and editing entries.
- **Flexible Keywords**: Entry types can be placed anywhere in the command.
