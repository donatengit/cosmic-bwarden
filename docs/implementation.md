# Implementation Details

The **COSMIC BWarden Client** project follows a modular architecture designed for security, performance, and maintainability.

## 1. core (`cosmic-bwarden-core`)

The core library handles the heavy lifting of Bitwarden logic and security primitives.

### Cryptography
- **Symmetric Encryption**: Supports Bitwarden Type 2 cipherstrings using AES-256-CBC with HMAC-SHA256.
- **Asymmetric Encryption**: Supports RSA-2048-OAEP-SHA1 for organization keys and private key protection.
- **KDF Support**: Implements both PBKDF2 and Argon2id for master password hashing and key derivation, matching Bitwarden's security standards.

### Security & Memory Management
- **Memory Locking**: Critical secrets (Master Keys, Private Keys) are stored in buffers wrapped by the `region` crate to prevent them from being paged to disk (swapped).
- **Secure Clearing**: All secret-holding types implement the `Zeroize` trait to ensure memory is wiped immediately after use.

### API Client
- Uses `reqwest` with `rustls` for secure HTTPS communication.
- Implements the full Bitwarden login flow, including prelogin (KDF parameter retrieval) and vault synchronization.
- **Account Registration**: Supports creating new accounts directly, including KDF parameter negotiation.

## 2. Agent (`cosmic-bwarden-agent`)

The agent acts as the "source of truth" and secret manager for the local system.

### State Management
- Maintains the unlocked vault state and KDF parameters in a thread-safe `Arc<Mutex<State>>`.
- **Identity Verification**: Stores a salted hash of the master password to allow for "Master Password Reprompt" verification without having to re-derive the vault encryption keys.

### Security Features
- **Master Password Reprompt**: For items marked with `reprompt = 1` in the Bitwarden vault, the agent requires an explicit `VerifyMasterPassword` action from the client before releasing the secret.
- **Real-Time Synchronization**: All entry modifications (Add, Update, Delete) are immediately pushed to the configured Bitwarden/Vaultwarden server and locally cached upon success, ensuring data integrity across devices.
- **Automatic Locking**: Configurable idle timeout for automatic vault locking.

## 3. UI and Applet (`cosmic-bwarden-ui` & `cosmic-bwarden-ui`)

The front-end components are built on the latest **libcosmic 1.0.0** framework.

### Multi-Window and Window Management
- **State-Driven Windows**: The UI dynamically manages two distinct window types: `Auth` and `Main`. It uses the current vault state (Locked/Unlocked) to decide which window should be active.
- **Window Focusing**: Implements `window::gain_focus()` to intelligently handle user interaction. If a user clicks the applet "Unlock" button while the Auth window is already open but obscured, the application automatically brings the existing window to the front.
- **Compact Sizing**: Uses the COSMIC `autosize` feature combined with `Length::Shrink` containers for Authentication dialogs, providing a tight, focused user experience.

### Advanced Settings Integration
- **Native Widgets**: Leverages `cosmic::widget::settings::item` for a seamless integration with the COSMIC system theme.
- **Progressive Disclosure**: Keeps the primary login card simple while hiding advanced options (Remember email, Custom Server) behind an expandable "Advanced" toggle.

### Performance Optimizations
- **Decryption Caching**: The agent maintains a lazy-populated cache for entry names and usernames, significantly reducing AES decryption overhead for large vaults (thousands of items) when performing searches or listing sidebars.
- **Lightweight IPC**: Uses specialized minimal structs (`SidebarEntry`) for frequent sidebar updates to minimize JSON serialization latency over the Unix socket.

### Clipboard Integration
- Integrates with the system clipboard for one-click password copying.
- Specifically optimized for the COSMIC panel context in the applet view.
