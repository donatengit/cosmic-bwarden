//! `cosmarden-applet`: the COSMIC panel applet and the vault window, selected
//! by `detect_run_mode()`. The `Application` impl and the process entry points
//! live in the library (`runtime.rs`).

fn main() -> Result<(), Box<dyn std::error::Error>> {
    cosmarden_core::dirs::adopt_legacy_env();
    cosmarden_ui::setup_logs();

    let flags = cosmarden_ui::parse_cli_flags();
    let mode = cosmarden_ui::detect_run_mode();
    tracing::info!(
        ?mode,
        version = %cosmarden_core::version(),
        protocol_version = cosmarden_core::PROTOCOL_VERSION,
        "Starting Cosmarden UI"
    );
    cosmarden_ui::localize::localize();

    match mode {
        cosmarden_ui::RunMode::Applet => cosmarden_ui::run_applet(flags).map_err(|e| e.into()),
        cosmarden_ui::RunMode::Application => {
            cosmarden_ui::run_application(flags).map_err(|e| e.into())
        }
    }
}
