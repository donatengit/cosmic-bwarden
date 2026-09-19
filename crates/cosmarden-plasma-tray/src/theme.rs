//! Whether the *panel* is dark, so the tray icon must be light — and the watch
//! that keeps that answer current when the user switches Plasma's theme.
//!
//! Order, mirroring the retired libcosmic tray so both pick the same glyph on
//! the same desktop: the freedesktop appearance portal first (Plasma 6 answers
//! it, and it is the only source that emits a change signal), then `kdeglobals`
//! (`[Colors:Window] BackgroundNormal`, then the look-and-feel package name),
//! then GTK's `gtk-application-prefer-dark-theme`, then dark.
//!
//! Everything that decides is pure and unit-tested; the portal read and the
//! file lookups are the only I/O.

use zbus::zvariant::OwnedValue;

fn xdg_config(rel: &str) -> std::path::PathBuf {
    if let Ok(home) = std::env::var("XDG_CONFIG_HOME") {
        return std::path::PathBuf::from(home).join(rel);
    }
    std::env::var("HOME")
        .map(|h| std::path::PathBuf::from(h).join(".config").join(rel))
        .unwrap_or_else(|_| std::path::PathBuf::from(rel))
}

/// Freedesktop portal `org.freedesktop.appearance` `color-scheme`.
/// 0 = no preference, 1 = prefer dark, 2 = prefer light.
pub fn portal_prefers_dark(color_scheme: u32) -> Option<bool> {
    match color_scheme {
        1 => Some(true),
        2 => Some(false),
        _ => None,
    }
}

/// Relative luminance (sRGB) of a window background. Below 0.5 is dark.
pub fn rgb_is_dark(r: u8, g: u8, b: u8) -> bool {
    let lin = |c: u8| {
        let s = f64::from(c) / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b) < 0.5
}

/// `[Colors:Window] BackgroundNormal=r,g,b` and LookAndFeel `*dark*`.
pub fn parse_kdeglobals_is_dark(text: &str) -> Option<bool> {
    let mut in_window = false;
    let mut from_bg = None;
    let mut from_laf = None;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') {
            in_window = t.eq_ignore_ascii_case("[Colors:Window]");
            continue;
        }
        if let Some(rest) = t.strip_prefix("LookAndFeelPackage=") {
            from_laf = Some(rest.to_ascii_lowercase().contains("dark"));
        }
        if in_window {
            if let Some(rest) = t.strip_prefix("BackgroundNormal=") {
                let mut parts = rest.split(',');
                let r = parts.next()?.trim().parse().ok()?;
                let g = parts.next()?.trim().parse().ok()?;
                let b = parts.next()?.trim().parse().ok()?;
                from_bg = Some(rgb_is_dark(r, g, b));
            }
        }
    }
    from_bg.or(from_laf)
}

pub fn parse_gtk_prefer_dark(text: &str) -> Option<bool> {
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("gtk-application-prefer-dark-theme=") {
            return Some(rest.trim() == "true" || rest.trim() == "1");
        }
    }
    None
}

fn owned_u32(v: &OwnedValue) -> Option<u32> {
    match &**v {
        zbus::zvariant::Value::U32(n) => Some(*n),
        zbus::zvariant::Value::Value(inner) => match inner.as_ref() {
            zbus::zvariant::Value::U32(n) => Some(*n),
            _ => u32::try_from(v).ok(),
        },
        _ => u32::try_from(v).ok(),
    }
}

async fn portal_color_scheme() -> Option<u32> {
    let conn = zbus::Connection::session().await.ok()?;
    let proxy = SettingsProxy::new(&conn).await.ok()?;
    let value = proxy
        .read("org.freedesktop.appearance", "color-scheme")
        .await
        .ok()?;
    owned_u32(&value)
}

/// True when the desktop wants a dark panel (use a *light* tray glyph).
///
/// Defaults to dark: Plasma's Breeze panel is dark, and a light glyph on an
/// unreadable config is the safer of the two wrong answers.
pub async fn panel_is_dark() -> bool {
    if let Some(v) = portal_color_scheme().await {
        if let Some(dark) = portal_prefers_dark(v) {
            return dark;
        }
    }
    if let Ok(text) = std::fs::read_to_string(xdg_config("kdeglobals")) {
        if let Some(dark) = parse_kdeglobals_is_dark(&text) {
            return dark;
        }
    }
    if let Ok(text) = std::fs::read_to_string(xdg_config("gtk-3.0/settings.ini")) {
        if let Some(dark) = parse_gtk_prefer_dark(&text) {
            return dark;
        }
    }
    true
}

#[zbus::proxy(
    interface = "org.freedesktop.portal.Settings",
    default_service = "org.freedesktop.portal.Desktop",
    default_path = "/org/freedesktop/portal/desktop"
)]
trait Settings {
    fn read(&self, namespace: &str, key: &str) -> zbus::Result<OwnedValue>;
    #[zbus(signal)]
    fn setting_changed(&self, namespace: &str, key: &str, value: OwnedValue) -> zbus::Result<()>;
}

/// Call `on_change(is_dark)` whenever the portal color-scheme flips.
///
/// Returns when the portal is unavailable or the signal stream ends, so the
/// caller can spawn this and forget it: a Plasma session without the portal
/// (or a D-Bus hiccup) simply keeps the startup glyph.
pub async fn watch_panel_dark<F>(mut on_change: F)
where
    F: FnMut(bool) + Send,
{
    let Ok(conn) = zbus::Connection::session().await else {
        tracing::debug!("no session bus; the tray icon keeps its startup contrast");
        return;
    };
    let Ok(proxy) = SettingsProxy::new(&conn).await else {
        tracing::debug!("no settings portal; the tray icon keeps its startup contrast");
        return;
    };
    let Ok(mut stream) = proxy.receive_setting_changed().await else {
        tracing::debug!("settings portal has no signal stream");
        return;
    };
    use futures_util::StreamExt;
    while let Some(sig) = stream.next().await {
        let Ok(args) = sig.args() else { continue };
        if *args.namespace() != "org.freedesktop.appearance" || *args.key() != "color-scheme" {
            continue;
        }
        if let Some(v) = owned_u32(args.value()) {
            if let Some(dark) = portal_prefers_dark(v) {
                tracing::info!(dark, "panel colour scheme changed; swapping the tray glyph");
                on_change(dark);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_window_background_is_dark() {
        assert!(rgb_is_dark(36, 36, 36));
        assert!(rgb_is_dark(51, 51, 51));
        assert!(!rgb_is_dark(255, 255, 255));
        assert!(!rgb_is_dark(242, 242, 243));
    }

    #[test]
    fn portal_1_is_dark_2_is_light() {
        assert_eq!(portal_prefers_dark(1), Some(true));
        assert_eq!(portal_prefers_dark(2), Some(false));
        assert_eq!(portal_prefers_dark(0), None);
    }

    /// The window colour wins over the look-and-feel name: a `*-dark` package
    /// with a light window background is a light panel.
    #[test]
    fn window_background_decides_before_the_package_name() {
        let text = "\
[KDE]
LookAndFeelPackage=com.github.vinceliuice.WhiteSur-dark
[Colors:Window]
BackgroundNormal=255,255,255
";
        assert_eq!(parse_kdeglobals_is_dark(text), Some(false));
    }

    #[test]
    fn the_package_name_is_the_fallback() {
        let text = "[KDE]\nLookAndFeelPackage=org.kde.breezedark.desktop\n";
        assert_eq!(parse_kdeglobals_is_dark(text), Some(true));
        // Nothing to go on at all: unknown, so the caller picks the default.
        assert_eq!(parse_kdeglobals_is_dark("[KDE]\n"), None);
    }

    #[test]
    fn only_the_window_section_is_read() {
        let text = "\
[Colors:View]
BackgroundNormal=255,255,255
";
        assert_eq!(
            parse_kdeglobals_is_dark(text),
            None,
            "a colour from another section must not decide the panel"
        );
    }

    #[test]
    fn gtk_ini_prefer_dark() {
        assert_eq!(
            parse_gtk_prefer_dark("gtk-application-prefer-dark-theme=true\n"),
            Some(true)
        );
        assert_eq!(
            parse_gtk_prefer_dark("gtk-application-prefer-dark-theme=false\n"),
            Some(false)
        );
        assert_eq!(parse_gtk_prefer_dark(""), None);
    }
}
