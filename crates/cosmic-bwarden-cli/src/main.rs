mod args;
mod commands;
mod output;
mod utils;

use anyhow::{Context, Result};
use args::Cli;
use clap::Parser;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action, EntryType as ProtocolEntryType};
use output::handle_response;
use utils::preprocess_args;

#[tokio::main]
async fn main() -> Result<()> {
    let args = preprocess_args(std::env::args().collect());
    let cli = Cli::parse_from(args);

    if let Some(config_path) = &cli.config {
        cosmic_bwarden_core::dirs::set_config_override(config_path.clone());
    }
    if let Some(socket_path) = &cli.socket {
        cosmic_bwarden_core::dirs::set_socket_override(socket_path.clone());
    }

    // Load configuration to check for additional overrides if CLI didn't set socket
    if cli.socket.is_none() && std::env::var("COSMIC_BWARDEN_SOCKET").is_err() {
        if let Ok(config) = cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
            if let Some(path) = config.socket_path {
                cosmic_bwarden_core::dirs::set_socket_override(std::path::PathBuf::from(path));
            }
        }
    }

    let client = AgentClient::new();
    let entry_type = cli.type_.map(ProtocolEntryType::from);

    let res = commands::run(&cli, &client, entry_type).await;

    if let Err(e) = &res {
        if e.to_string().contains("agent is locked") {
            let password = rpassword::prompt_password("Vault is locked. Master Password: ")
                .context("failed to read password")?;

            let unlock_res = client.send(Action::Unlock { password }).await?;
            handle_response(unlock_res)?;

            // Retry the command once
            return commands::run(&cli, &client, entry_type).await;
        }
    }

    res
}
