use anyhow::{Context, Result};
use crate::args::{Cli, Commands};
use crate::output::handle_response;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action, Response};

pub fn check_protocol_compatibility(local: &str, protocol: &str) -> Result<()> {
    if local != protocol {
        anyhow::bail!(
            "Version mismatch! CLI (v{}) is incompatible with agent (protocol v{}).",
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
            let password = if let Some(p) = password {
                p.clone()
            } else {
                rpassword::prompt_password("Master Password: ")
                    .context("failed to read password")?
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
            let password = if let Some(p) = password {
                p.clone()
            } else {
                rpassword::prompt_password("Master Password: ")
                    .context("failed to read password")?
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

            // Offer PIN unlock setup if TPM2 is available and not yet configured.
            if let Ok(Response::TpmStatus { available: true, configured: false, .. }) =
                client.send(Action::CheckTpm).await
            {
                eprintln!(
                    "TPM2 available. Enter a PIN (min 6 chars) to enable PIN unlock, or leave empty to skip:"
                );
                if let Ok(pin) = rpassword::prompt_password("PIN: ") {
                    let pin = pin.trim().to_string();
                    if !pin.is_empty() {
                        if pin.len() < 6 {
                            eprintln!("PIN must be at least 6 characters — skipping.");
                        } else {
                            let setup_res = client
                                .send(Action::SetupTpmPinFromUnlocked { pin })
                                .await
                                .context("failed to set up TPM PIN")?;
                            match setup_res {
                                Response::Ack => println!("PIN unlock enabled."),
                                Response::Error { message } => {
                                    eprintln!("PIN setup failed: {}", message)
                                }
                                _ => eprintln!("Unexpected response from PIN setup"),
                            }
                        }
                    }
                }
            }
        }
        Commands::Unlock { password } => {
            let password = if let Some(p) = password {
                p.clone()
            } else {
                rpassword::prompt_password("Master Password: ")
                    .context("failed to read password")?
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
                let local = cosmic_bwarden_core::version();
                println!("Local version:     {}", local);
                println!("Agent version:     {}", agent_version);
                println!("Protocol version:  {}", protocol_version);
                check_protocol_compatibility(local, &protocol_version)?;
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
        assert!(check_protocol_compatibility("2026.06-100-abc", "2026.06-100-abc").is_ok());
    }

    #[test]
    fn test_version_mismatch_error() {
        let err =
            check_protocol_compatibility("2026.06-100-abc", "0.0.0-fake").unwrap_err();
        assert!(err.to_string().contains("Version mismatch"));
    }
}
