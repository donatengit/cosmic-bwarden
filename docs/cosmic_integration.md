# Integration with COSMIC DE

To properly integrate **cosmic-bwarden** with the COSMIC desktop environment, especially for the applet to appear in the panel rather than as a standalone window, you need to provide metadata and desktop files.

## 1. The Applet (`cosmic-bwarden-ui`)

COSMIC applets are discovered by the panel using metadata files.

### Metadata File
Create a file at `/usr/share/cosmic/applets/org.cosmic-bwarden.applet.ron` (or in your local `~/.local/share/cosmic/applets/`):

```ron
(
    name: "cosmic-bwarden Applet",
    description: "Quick access to frequent Bitwarden entries",
    identifier: "org.cosmic-bwarden.applet",
    icon: "password-manager-symbolic",
)
```

### Desktop File
Create a file at `/usr/share/applications/org.cosmic-bwarden.applet.desktop`:

```ini
[Desktop Entry]
Name=cosmic-bwarden Applet
Exec=cosmic-bwarden-ui
Icon=password-manager-symbolic
Terminal=false
Type=Application
Categories=Utility;
```

## 2. Architecture: Why 2 Apps?

In the COSMIC ecosystem:
- **Applications** (`cosmic-bwarden-ui`) are full-featured windows for complex tasks (searching the whole vault, configuring settings).
- **Applets** (`cosmic-bwarden-ui`) are minimalistic components that live inside the system panel.

They are separate binaries because the COSMIC panel process "embeds" the applet. This design ensures that if an applet crashes, it doesn't take down the whole desktop, and the panel can manage applet lifecycle independently. Both connect to the `cosmic-bwarden-agent` background service, which keeps the vault unlocked in memory.

## 3. Custom Servers

The login screen now supports entering a custom server URL. This allows you to use `cosmic-bwarden` with official Bitwarden instances, self-hosted Vaultwarden instances, or custom enterprise installations.

## 4. "Remember Me"

Selecting "Remember Email" will persist your email address in `config.json` so you only need to enter your master password on subsequent launches.
