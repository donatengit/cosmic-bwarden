mod pin;

use crate::args::{Cli, Commands};
use crate::output::handle_response;
use anyhow::{Context, Result};
use cosmarden_core::agent_client::AgentClient;
use cosmarden_core::protocol::{Action, Response};

/// Compare the CLI's protocol version against the agent's. Both sides report
/// `cosmarden_core::PROTOCOL_VERSION`, which is independent of the build
/// version — differently-timed builds of the same protocol stay compatible.
pub fn check_protocol_compatibility(local: &str, protocol: &str) -> Result<()> {
    if local != protocol {
        anyhow::bail!(
            "Protocol mismatch! CLI (protocol v{}) is incompatible with agent (protocol v{}).",
            local,
            protocol,
        );
    }
    Ok(())
}

pub async fn handle_command(cli: &Cli, client: &AgentClient) -> Result<()> {
    match &cli.command {
        Commands::Register {
            email,
            server,
            password,
        } => {
            let password = match password {
                Some(p) => p.clone(),
                None => rpassword::prompt_password("Master Password: ")
                    .context("failed to read password")?,
            };

            let res = client
                .send(Action::Register {
                    email: email.clone(),
                    password,
                    server_url: server.clone(),
                })
                .await
                .context("failed to talk to agent")?;

            handle_response(res)?;
            println!("Account created successfully");
        }
        Commands::Login {
            email,
            server,
            password,
        } => {
            let password = match password {
                Some(p) => p.clone(),
                None => rpassword::prompt_password("Master Password: ")
                    .context("failed to read password")?,
            };

            let mut res = client
                .send(Action::Login {
                    email: email.clone(),
                    password: password.clone(),
                    server_url: server.clone(),
                    remember_me: true,
                    two_factor_token: None,
                    two_factor_provider: None,
                    two_factor_code: None,
                    device_verification_code: None,
                })
                .await
                .context("failed to talk to agent")?;

            if let Response::NewDeviceVerificationRequired = &res {
                println!("A verification code has been sent to your email.");
                let code = rpassword::prompt_password("Verification code: ")
                    .context("failed to read verification code")?;

                res = client
                    .send(Action::Login {
                        email: email.clone(),
                        password: password.clone(),
                        server_url: server.clone(),
                        remember_me: true,
                        two_factor_token: None,
                        two_factor_provider: None,
                        two_factor_code: None,
                        device_verification_code: Some(code),
                    })
                    .await
                    .context("failed to talk to agent")?;
            }

            if let Response::TwoFactorRequired { token, providers } = &res {
                println!("Two-factor authentication required.");
                if providers.contains(&1) {
                    println!("1. Email");
                }
                let provider = if providers.contains(&1) {
                    1
                } else {
                    providers[0]
                };

                let code = rpassword::prompt_password("Two-factor code: ")
                    .context("failed to read code")?;

                res = client
                    .send(Action::Login {
                        email: email.clone(),
                        password: password.clone(),
                        server_url: server.clone(),
                        remember_me: true,
                        two_factor_token: Some(token.clone()),
                        two_factor_provider: Some(provider),
                        two_factor_code: Some(code),
                        device_verification_code: None,
                    })
                    .await
                    .context("failed to talk to agent")?;
            }

            handle_response(res)?;
            println!("Logged in successfully");
            pin::offer_after_login(client).await?;
        }
        Commands::Unlock { password } => {
            let password = match password {
                Some(p) => p.clone(),
                None => rpassword::prompt_password("Master Password: ")
                    .context("failed to read password")?,
            };

            let res = client.send(Action::Unlock { password }).await?;
            handle_response(res)?;
            println!("Unlocked successfully");
        }
        Commands::Lock => {
            let res = client.send(Action::Lock).await?;
            handle_response(res)?;
            println!("Locked successfully");
        }
        Commands::Unlocked => {
            let res = client.send(Action::Version).await;
            match res {
                Ok(_) => println!("Agent is running and connected."),
                Err(e) => anyhow::bail!("Agent is not running or not reachable: {}", e),
            }
        }
        Commands::Logout => {
            let res = client.send(Action::Logout).await?;
            handle_response(res)?;
            println!("Logged out successfully");
        }
        Commands::Quit => {
            let res = client.send(Action::Quit).await?;
            handle_response(res)?;
            println!("Agent quit successfully");
        }
        Commands::Version => {
            let res = client.send(Action::Version).await?;
            if let Response::Version {
                version: agent_version,
                protocol_version,
            } = res
            {
                println!("Local version:     {}", cosmarden_core::version());
                println!("Agent version:     {}", agent_version);
                println!("Protocol version:  {}", protocol_version);
                check_protocol_compatibility(cosmarden_core::PROTOCOL_VERSION, &protocol_version)?;
                println!("Compatibility:     OK");
            } else {
                anyhow::bail!("Unexpected response: {:?}", res);
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_match_ok() {
        // Matching protocol constants are compatible regardless of build version.
        assert!(check_protocol_compatibility("1", "1").is_ok());
        assert!(check_protocol_compatibility(
            cosmarden_core::PROTOCOL_VERSION,
            cosmarden_core::PROTOCOL_VERSION
        )
        .is_ok());
    }

    #[test]
    fn test_version_mismatch_error() {
        let err = check_protocol_compatibility("1", "2").unwrap_err();
        assert!(err.to_string().contains("Protocol mismatch"));
    }
}
