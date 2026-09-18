//! Thin Plasma tray: `cosmarden-plasma-tray`.
//!
//! A StatusNotifierItem whose only surface is one menu. Left- and right-click
//! both open it, because there is no popup to show: the item advertises
//! `ItemIsMenu`, which Plasma ≥ 6.4 honours by never calling `Activate` (see
//! `tray::PlasmaTray::MENU_ON_ACTIVATE`).
//!
//! What it deliberately does *not* have, and why:
//!
//! - **No unlock.** The menu cannot host a text field — `com.canonical.dbusmenu`
//!   has no text-input item, and the host draws the menu, so an embedded widget
//!   cannot cross the bus (docs/kde_applet_parity.md, "Tray menus cannot host
//!   input fields"). `Open Vault` starts the full app, which is the only unlock
//!   path from here — and the only way to serve a reprompt-gated copy.
//! - **Copy lives here now, with the UI's discipline.** `Copy login` reads the
//!   username out of the sidebar snapshot; `Copy password` asks the agent
//!   (`GetPassword`) and hands the `Secret` straight to the clipboard, which is
//!   the crate's only plaintext boundary (`clipboard.rs`). The value is never
//!   logged, is zeroized on drop, and the selection is cleared after
//!   `CLIPBOARD_CLEAR_SECS` if it is still ours. A reprompt-gated entry cannot
//!   be copied from here (no input field exists), so that path logs and points
//!   at `Open Vault`.
//! - **No libcosmic.** The COSMIC applet is the full surface there; on Plasma
//!   this is a menu and a hand-off. Every dependency here is a protocol, D-Bus,
//!   or tokio crate.

mod clipboard;
mod icon;
mod launch;
mod menu;
mod state;
mod supervisor;
mod theme;
mod tray;

use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() {
    cosmarden_core::dirs::adopt_legacy_env();
    setup_logs();
    tracing::info!(
        version = %cosmarden_core::version(),
        protocol_version = cosmarden_core::PROTOCOL_VERSION,
        "Starting cosmarden-plasma-tray"
    );
    if let Err(e) = tray::run().await {
        // Never fail silently: without this line a tray that could not register
        // just leaves no icon, and the user has nothing to report.
        tracing::error!("tray exited: {e}");
        std::process::exit(1);
    }
}

/// stderr logging, `info` for this binary by default. No journald layer (the
/// full tray has one): this process is started from a session and its output
/// belongs wherever the session put it.
fn setup_logs() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("warn,{}=info", env!("CARGO_CRATE_NAME"))));
    // The HTTP stack's caps mirror the agent's: this process links the same
    // core API client, and trace-level reqwest/hyper output includes
    // `Authorization` headers.
    let filter = filter
        .add_directive("reqwest=info".parse().expect("static directive"))
        .add_directive("hyper=info".parse().expect("static directive"))
        .add_directive("hyper_util=info".parse().expect("static directive"))
        .add_directive("h2=info".parse().expect("static directive"))
        .add_directive("rustls=info".parse().expect("static directive"));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(false))
        .init();
}
