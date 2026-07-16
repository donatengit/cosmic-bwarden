use crate::args::{Cli, Commands};
use crate::output::handle_response;
use anyhow::Result;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action, GeneratorSettings, Response};

pub async fn handle_command(cli: &Cli, client: &AgentClient) -> Result<()> {
    let Commands::Generate {
        uppercase,
        lowercase,
        numbers,
        special,
        length,
        history,
    } = &cli.command
    else {
        unreachable!("commands::run only routes Commands::Generate here");
    };

    if *history {
        return print_history(client).await;
    }

    // clap bools can't distinguish "explicitly false" from "not passed", so
    // any one of -U/-l/-n/-s present means the user is fully specifying this
    // run's character groups (matching the desktop pane's Generate button,
    // which always submits its full checkbox state). None present means
    // "reuse whatever was last saved" (applet/browser/bare-CLI behavior).
    let settings = if *uppercase || *lowercase || *numbers || *special || length.is_some() {
        let length = match length {
            Some(l) => *l,
            None => current_length(client).await?,
        };
        Some(GeneratorSettings {
            uppercase: *uppercase,
            lowercase: *lowercase,
            numbers: *numbers,
            special: *special,
            length,
        })
    } else {
        None
    };

    let res = client.send(Action::GeneratePassword { settings }).await?;
    match res {
        Response::GeneratedPassword { password } => println!("{password}"),
        other => handle_response(other)?,
    }
    Ok(())
}

/// Fetch the currently persisted length, for a run that specifies character
/// groups but not `--length` (reuse the saved length rather than forcing a
/// default onto an otherwise-explicit request).
async fn current_length(client: &AgentClient) -> Result<u8> {
    let res = client.send(Action::GetGeneratorSettings).await?;
    match res {
        Response::GeneratorSettings { settings } => Ok(settings.length),
        _ => Ok(GeneratorSettings::default().length),
    }
}

async fn print_history(client: &AgentClient) -> Result<()> {
    let res = client.send(Action::GetPasswordHistory).await?;
    match res {
        Response::PasswordHistory { entries } => {
            for entry in entries {
                println!("{} | {}", entry.created_at, entry.password);
            }
        }
        other => handle_response(other)?,
    }
    Ok(())
}
