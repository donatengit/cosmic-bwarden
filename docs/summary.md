# Cosmic BWarden Client: Minimalistic Native COSMIC Bitwarden Client

**Cosmic BWarden Client** is a high-performance, native Bitwarden client designed specifically for the COSMIC desktop environment. Built with Rust and the `libcosmic` toolkit, it provides a seamless and secure experience for managing your vault entries with a focus on speed, low memory footprint, and deep system integration.

## Key Features

- **Native COSMIC UI**: Built using `libcosmic` 1.0.0, adhering to the COSMIC design language and using the efficient Model-View-Update (MVU) architecture.
- **Dedicated Authentication Window**: Follows modern desktop patterns with a compact, focused window for Login and Unlock tasks, opening the full workspace only when authenticated.
- **Intelligent Window Management**: Seamlessly switches between Auth and Main windows, and automatically brings existing windows to the front when requested via the system applet.
- **Advanced Configuration**: Clean credential cards with expandable "Advanced" options for "Remember email" and custom server settings, using native COSMIC settings widgets.
- **Real-Time Server Synchronization**: Full CRUD (Create, Read, Update, Delete) support with immediate server-side synchronization for all entry modifications.
- **Manual Sync Button**: Direct control over vault refreshing via a dedicated Sync button in the vault sidebar.
- **Secure Background Agent**: A dedicated daemon (`cosmic-bwarden-agent`) manages your vault state in memory-locked regions, ensuring secrets are never swapped to disk and remain protected even when the UI is closed.
- **Diverse Vault Items**: Full support for standard logins, Secure Notes, and SSH Keys, with automatic synchronization and local caching.
- **Master Password Reprompt**: Implements granular security for sensitive items, requiring master password verification for specifically marked entries.
- **SSH Agent Integration**: Automatically serves SSH keys stored in your Bitwarden vault to the system's SSH agent.
- **Tray Applet with Frequent Access**: A specialized panel applet provides quick access to your top-5 most frequently used passwords.

## Architecture Overview

The project is structured as a Rust workspace with four main components, each following a strict modular decomposition (targeting <250 lines per file) to ensure high maintainability and security auditing:
- `cosmic-bwarden-core`: The internal library for cryptography, Bitwarden API communication, and data modeling.
- `cosmic-bwarden-agent`: The background service managing the unlocked vault and SSH agent.
- `cosmic-bwarden-ui`: The main application for searching and managing vault entries, following the MVU pattern.
- `cosmic-bwarden-tests`: A comprehensive E2E suite using Docker to verify the full stack.
