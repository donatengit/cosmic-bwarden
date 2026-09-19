//! Shared UI crate: the COSMIC applet and the vault window, built into the
//! single `cosmarden-applet` binary (`src/main.rs`). The Plasma tray is a
//! separate crate that links no libcosmic (`cosmarden-plasma-tray`); nothing
//! here serves it.
//!
//! The crate is a library so the applet and vault entry points share the
//! `Application` impl and the views.

pub mod app;
#[cfg(feature = "kde")]
pub mod kde;
pub mod localize;
pub mod message;
mod runtime;
pub mod view;

pub use app::{AppFlags, CosmardenApp, APP_ID};
pub use runtime::{parse_cli_flags, run_applet, run_application};

/// Minimum PIN length for TPM-backed unlock; single source in core. Used in
/// input captions and submit validation; the agent enforces it
/// authoritatively.
pub const MIN_PIN_LEN: usize = cosmarden_core::MIN_PIN_LEN;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    Applet,
    Application,
}

/// The COSMIC panel sets `COSMIC_PANEL_NAME`; `spawn_vault_app` sets
/// `COSMARDEN_MODE=application`. Anything else is the vault window.
pub fn detect_run_mode() -> RunMode {
    if std::env::var("COSMIC_PANEL_NAME").is_ok() {
        RunMode::Applet
    } else {
        RunMode::Application
    }
}

pub fn setup_logs() {
    use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

    let fmt_layer = fmt::layer().with_target(false);
    // The HTTP-stack caps mirror the agent's (`[P1-7]`): this process links
    // the same core API client, and trace-level reqwest/hyper output includes
    // `Authorization` headers — journald persists it to disk.
    let filter_layer = EnvFilter::try_from_default_env().unwrap_or(EnvFilter::new(format!(
        "warn,{}=info",
        env!("CARGO_CRATE_NAME")
    )));
    // Plasma has no COSMIC session D-Bus, so libcosmic's theme and config
    // lookups fail from the first frame on. Those crates log at ERROR, so they
    // need `=off` rather than `=warn` — see `kde::THEME_NOISE_DIRECTIVES`.
    #[cfg(feature = "kde")]
    let filter_layer = crate::kde::apply_theme_noise_filter(filter_layer);
    let filter_layer = filter_layer
        .add_directive("reqwest=info".parse().expect("static directive"))
        .add_directive("hyper=info".parse().expect("static directive"))
        .add_directive("hyper_util=info".parse().expect("static directive"))
        .add_directive("h2=info".parse().expect("static directive"))
        .add_directive("rustls=info".parse().expect("static directive"));

    if let Ok(journal_layer) = tracing_journald::layer() {
        tracing_subscriber::registry()
            .with(filter_layer)
            .with(fmt_layer)
            .with(journal_layer)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter_layer)
            .with(fmt_layer)
            .init();
    }
}
