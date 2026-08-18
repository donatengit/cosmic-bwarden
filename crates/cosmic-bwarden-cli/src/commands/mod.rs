pub mod auth;
pub mod generator;
pub mod vault;

use crate::args::{Cli, Commands};
use anyhow::Result;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::EntryType as ProtocolEntryType;

pub async fn run(
    cli: &Cli,
    client: &AgentClient,
    entry_type: Option<ProtocolEntryType>,
) -> Result<()> {
    match &cli.command {
        Commands::Register { .. }
        | Commands::Login { .. }
        | Commands::Unlock { .. }
        | Commands::Lock
        | Commands::Logout
        | Commands::Quit
        | Commands::Unlocked
        | Commands::Version => auth::handle_command(cli, client).await,

        Commands::Sync
        | Commands::List { .. }
        | Commands::Pin { .. }
        | Commands::Unpin { .. }
        | Commands::Get { .. }
        | Commands::Add { .. }
        | Commands::Edit { .. }
        | Commands::AddNote { .. }
        | Commands::AddSshKey { .. } => vault::handle_command(cli, client, entry_type).await,

        Commands::Generate { .. } => generator::handle_command(cli, client).await,
    }
}
