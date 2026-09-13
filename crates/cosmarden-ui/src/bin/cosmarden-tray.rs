//! Agent-only Plasma tray: search, unlock, copy, generate, lock/logout.
//! No vault window. Requires `--features kde`.

use cosmarden_ui::{localize, parse_cli_flags, run_tray, setup_logs};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    cosmarden_core::dirs::adopt_legacy_env();
    setup_logs();
    let flags = parse_cli_flags();
    let mode = cosmarden_ui::detect_run_mode();
    tracing::info!(
        ?mode,
        version = %cosmarden_core::version(),
        protocol_version = cosmarden_core::PROTOCOL_VERSION,
        "Starting Cosmarden tray"
    );
    localize::localize();
    run_tray(flags).map_err(|e| e.into())
}
