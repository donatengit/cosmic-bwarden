//! E2E coverage for the password generator: charset/length combinations,
//! input validation, "last used settings" reuse (`settings: None`), and the
//! local password-history round trip. Deliberately never logs in — generation
//! must work with no account configured, which is exercised here by simply
//! not calling `Action::Login` at all.

use crate::common::setup_env;
use anyhow::Result;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action, GeneratorSettings, Response};

#[tokio::test]
async fn test_generate_password_various_charsets_and_lengths() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);
    let client = AgentClient::new_with_socket(env.socket_path.clone());

    let cases = [
        (true, true, true, true, 14u8),
        (true, false, false, false, 20u8),
        (false, false, true, true, 8u8),
        (true, true, false, false, 32u8),
    ];

    for (uppercase, lowercase, numbers, special, length) in cases {
        let settings = GeneratorSettings {
            uppercase,
            lowercase,
            numbers,
            special,
            length,
        };
        let res = client
            .send(Action::GeneratePassword {
                settings: Some(settings),
            })
            .await?;
        let password = match res {
            Response::GeneratedPassword { password } => password,
            other => anyhow::bail!("expected GeneratedPassword, got {other:?}"),
        };
        assert_eq!(password.chars().count(), length as usize);
        if uppercase {
            assert!(password.chars().any(|c| c.is_ascii_uppercase()));
        }
        if lowercase {
            assert!(password.chars().any(|c| c.is_ascii_lowercase()));
        }
        if numbers {
            assert!(password.chars().any(|c| c.is_ascii_digit()));
        }
        if special {
            assert!(password.chars().any(|c| !c.is_ascii_alphanumeric()));
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_generate_password_rejects_invalid_input() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);
    let client = AgentClient::new_with_socket(env.socket_path.clone());

    // No character group selected.
    let res = client
        .send(Action::GeneratePassword {
            settings: Some(GeneratorSettings {
                uppercase: false,
                lowercase: false,
                numbers: false,
                special: false,
                length: 14,
            }),
        })
        .await?;
    assert!(matches!(res, Response::Error { .. }), "expected Error, got {res:?}");

    // Length outside 8..=32.
    let res = client
        .send(Action::GeneratePassword {
            settings: Some(GeneratorSettings {
                length: 4,
                ..GeneratorSettings::default()
            }),
        })
        .await?;
    assert!(matches!(res, Response::Error { .. }), "expected Error, got {res:?}");

    Ok(())
}

#[tokio::test]
async fn test_generate_password_none_reuses_last_saved_settings() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);
    let client = AgentClient::new_with_socket(env.socket_path.clone());

    let chosen = GeneratorSettings {
        uppercase: false,
        lowercase: true,
        numbers: false,
        special: false,
        length: 22,
    };
    let res = client
        .send(Action::GeneratePassword {
            settings: Some(chosen),
        })
        .await?;
    assert!(matches!(res, Response::GeneratedPassword { .. }), "unexpected {res:?}");

    // A caller that doesn't specify settings (applet quick-gen, browser
    // extension, bare CLI) must reuse exactly what was just persisted.
    let res = client
        .send(Action::GeneratePassword { settings: None })
        .await?;
    let password = match res {
        Response::GeneratedPassword { password } => password,
        other => anyhow::bail!("expected GeneratedPassword, got {other:?}"),
    };
    assert_eq!(password.chars().count(), 22);
    assert!(password.chars().all(|c| c.is_ascii_lowercase()));

    let res = client.send(Action::GetGeneratorSettings).await?;
    match res {
        Response::GeneratorSettings { settings } => assert_eq!(settings, chosen),
        other => anyhow::bail!("expected GeneratorSettings, got {other:?}"),
    }

    Ok(())
}

#[tokio::test]
async fn test_password_history_round_trip() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);
    let client = AgentClient::new_with_socket(env.socket_path.clone());

    let mut generated = Vec::new();
    for _ in 0..3 {
        let res = client
            .send(Action::GeneratePassword {
                settings: Some(GeneratorSettings::default()),
            })
            .await?;
        match res {
            Response::GeneratedPassword { password } => generated.push(password),
            other => anyhow::bail!("expected GeneratedPassword, got {other:?}"),
        }
        // Ensure distinct `created_at` ordering is observable.
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    }

    let res = client.send(Action::GetPasswordHistory).await?;
    let entries = match res {
        Response::PasswordHistory { entries } => entries,
        other => anyhow::bail!("expected PasswordHistory, got {other:?}"),
    };
    assert!(
        entries.len() >= 3,
        "expected at least the 3 just-generated entries, got {}",
        entries.len()
    );
    for pw in &generated {
        assert!(
            entries.iter().any(|e| &e.password == pw),
            "generated password missing from history"
        );
    }
    // Newest first.
    for pair in entries.windows(2) {
        assert!(pair[0].created_at >= pair[1].created_at);
    }

    Ok(())
}

#[tokio::test]
async fn test_delete_password_history_entry() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);
    let client = AgentClient::new_with_socket(env.socket_path.clone());

    let mut generated = Vec::new();
    for _ in 0..2 {
        let res = client
            .send(Action::GeneratePassword {
                settings: Some(GeneratorSettings::default()),
            })
            .await?;
        match res {
            Response::GeneratedPassword { password } => generated.push(password),
            other => anyhow::bail!("expected GeneratedPassword, got {other:?}"),
        }
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    }

    let res = client.send(Action::GetPasswordHistory).await?;
    let entries = match res {
        Response::PasswordHistory { entries } => entries,
        other => anyhow::bail!("expected PasswordHistory, got {other:?}"),
    };
    let to_delete = entries
        .iter()
        .find(|e| e.password == generated[0])
        .expect("first generated password missing from history");
    let created_at = to_delete.created_at;

    let res = client
        .send(Action::DeleteGeneratedPassword { created_at })
        .await?;
    assert!(matches!(res, Response::Ack), "expected Ack, got {res:?}");

    let res = client.send(Action::GetPasswordHistory).await?;
    let entries = match res {
        Response::PasswordHistory { entries } => entries,
        other => anyhow::bail!("expected PasswordHistory, got {other:?}"),
    };
    assert!(
        !entries.iter().any(|e| e.password == generated[0]),
        "deleted password still present in history"
    );
    assert!(
        entries.iter().any(|e| e.password == generated[1]),
        "the other password should not have been affected"
    );

    Ok(())
}

#[tokio::test]
async fn test_cli_generate_subcommand() -> Result<()> {
    let env = setup_env().await?;

    // Bare `generate` works with no account configured at all.
    let output = env.cli_cmd().arg("generate").output()?;
    assert!(
        output.status.success(),
        "cli generate failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let password = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert_eq!(password.chars().count(), 14, "default length is 14");

    // Explicit flags fully specify this run's character groups and length.
    let output = env
        .cli_cmd()
        .arg("generate")
        .arg("--numbers")
        .arg("--length")
        .arg("10")
        .output()?;
    assert!(output.status.success());
    let password = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert_eq!(password.chars().count(), 10);
    assert!(password.chars().all(|c| c.is_ascii_digit()));

    // --history lists what was just generated.
    let output = env.cli_cmd().arg("generate").arg("--history").output()?;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(&password));

    Ok(())
}
