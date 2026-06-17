use anyhow::{Context, Result};
use crate::args::{Cli, Commands};
use crate::output::handle_response;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action, Response};

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
        Commands::Quit => {
            let res = client.send(Action::Quit).await?;
            handle_response(res)?;
            println!("Agent quit successfully");
        }
        _ => unreachable!(),
    }
    Ok(())
}
