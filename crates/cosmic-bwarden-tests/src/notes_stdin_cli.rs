use crate::common::{register_user, setup_env};
use anyhow::Result;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action, Response};
use std::io::Write;
use std::process::Stdio;

const CREDENTIALS_YAML: &str = "aws_access_key_id: AKIAEXAMPLE\naws_secret_access_key: super/secret+value\nregion: us-east-1\n";

/// Runs `cli_args` piping `stdin_content` in, and returns (success, stdout, stderr).
fn run_cli_with_stdin(
    env: &crate::common::TestEnv,
    cli_args: &[&str],
    stdin_content: &str,
) -> Result<(bool, String, String)> {
    let mut cmd = env.cli_cmd();
    cmd.args(cli_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn()?;
    child
        .stdin
        .take()
        .expect("child stdin")
        .write_all(stdin_content.as_bytes())?;

    let output = child.wait_with_output()?;
    Ok((
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    ))
}

#[tokio::test]
async fn test_add_note_from_stdin() -> Result<()> {
    let env = setup_env().await?;

    let email = "stdinnotes@example.com";
    let password = "stdinnotespassword123";
    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    client
        .send(Action::Login {
            email: email.to_string(),
            password: password.to_string(),
            server_url: Some(env.vault_url.clone()),
            remember_me: true,
            two_factor_token: None,
            two_factor_provider: None,
            two_factor_code: None,
            device_verification_code: None,
        })
        .await?;

    // Store a file's contents (e.g. `cat credentials.yaml | cosmic-bwarden-cli note add ... --stdin`)
    // as the note body, without ever putting the secret content on argv.
    let (success, _stdout, stderr) = run_cli_with_stdin(
        &env,
        &["note", "add", "AWS Creds", "--stdin"],
        CREDENTIALS_YAML,
    )?;
    assert!(success, "CLI add --stdin failed: {stderr}");

    client.send(Action::Sync).await?;
    let res = client
        .send(Action::GetEntries {
            query: Some("AWS Creds".to_string()),
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    let entry = if let Response::Entries { entries } = res {
        entries
            .into_iter()
            .next()
            .expect("AWS Creds entry not found")
    } else {
        anyhow::bail!("Expected entries, got: {res:?}");
    };

    let res = client
        .send(Action::GetEntry {
            id: entry.id.clone(),
            password: None,
        })
        .await?;
    let entry = if let Response::Entry { entry } = res {
        entry
    } else {
        anyhow::bail!("Expected full entry, got: {res:?}");
    };

    let notes = entry.notes.expect("notes should be set");
    assert_eq!(notes.expose(), CREDENTIALS_YAML);

    Ok(())
}

#[tokio::test]
async fn test_add_note_from_stdin_utf8() -> Result<()> {
    let env = setup_env().await?;

    let email = "utf8stdinnotes@example.com";
    let password = "utf8stdinnotespassword123";
    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    client
        .send(Action::Login {
            email: email.to_string(),
            password: password.to_string(),
            server_url: Some(env.vault_url.clone()),
            remember_me: true,
            two_factor_token: None,
            two_factor_provider: None,
            two_factor_code: None,
            device_verification_code: None,
        })
        .await?;

    // Emoji, Cyrillic, CJK, and accented Latin all appear in real credentials
    // files (e.g. a comment left by a non-English-speaking teammate, or a
    // 🔑 marker). Each is valid UTF-8 but multi-byte, unlike the plain-ASCII
    // content used by the other stdin tests.
    let utf8_content =
        "# 🔑 db credentials\nuser: naïve-café\npass: Пароль123\ncomment: 你好世界 🚀\n";

    let (success, _stdout, stderr) = run_cli_with_stdin(
        &env,
        &["note", "add", "UTF8 Creds", "--stdin"],
        utf8_content,
    )?;
    assert!(success, "CLI add --stdin failed: {stderr}");

    client.send(Action::Sync).await?;
    let res = client
        .send(Action::GetEntries {
            query: Some("UTF8 Creds".to_string()),
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    let entry = if let Response::Entries { entries } = res {
        entries
            .into_iter()
            .next()
            .expect("UTF8 Creds entry not found")
    } else {
        anyhow::bail!("Expected entries, got: {res:?}");
    };

    let res = client
        .send(Action::GetEntry {
            id: entry.id.clone(),
            password: None,
        })
        .await?;
    let entry = if let Response::Entry { entry } = res {
        entry
    } else {
        anyhow::bail!("Expected full entry, got: {res:?}");
    };

    let notes = entry.notes.expect("notes should be set");
    assert_eq!(notes.expose(), utf8_content);

    // Also verify `get -S` prints the content back byte-for-byte on stdout,
    // since that's the round-trip path a user would actually redirect to a file.
    let output = env
        .cli_cmd()
        .args(["get", "UTF8 Creds", "-S"])
        .output()?;
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)
        .expect("CLI stdout should be valid UTF-8");
    assert!(
        stdout.contains(utf8_content),
        "expected stdout to contain the UTF-8 note verbatim, got: {stdout}"
    );

    Ok(())
}

#[tokio::test]
async fn test_edit_note_from_stdin() -> Result<()> {
    let env = setup_env().await?;

    let email = "editstdinnotes@example.com";
    let password = "editstdinnotespassword123";
    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    client
        .send(Action::Login {
            email: email.to_string(),
            password: password.to_string(),
            server_url: Some(env.vault_url.clone()),
            remember_me: true,
            two_factor_token: None,
            two_factor_provider: None,
            two_factor_code: None,
            device_verification_code: None,
        })
        .await?;

    let output = env
        .cli_cmd()
        .args(["note", "add", "Env Vars", "notes=placeholder"])
        .output()?;
    assert!(
        output.status.success(),
        "CLI add failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated_content = "export API_TOKEN=abc123\nexport DB_URL=postgres://localhost/app\n";
    let (success, _stdout, stderr) = run_cli_with_stdin(
        &env,
        &["edit", "Env Vars", "--stdin"],
        updated_content,
    )?;
    assert!(success, "CLI edit --stdin failed: {stderr}");

    client.send(Action::Sync).await?;
    let res = client
        .send(Action::GetEntries {
            query: Some("Env Vars".to_string()),
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    let entry = if let Response::Entries { entries } = res {
        entries.into_iter().next().expect("Env Vars entry not found")
    } else {
        anyhow::bail!("Expected entries, got: {res:?}");
    };

    let res = client
        .send(Action::GetEntry {
            id: entry.id.clone(),
            password: None,
        })
        .await?;
    let entry = if let Response::Entry { entry } = res {
        entry
    } else {
        anyhow::bail!("Expected full entry, got: {res:?}");
    };

    let notes = entry.notes.expect("notes should be set");
    assert_eq!(notes.expose(), updated_content);

    Ok(())
}

#[tokio::test]
async fn test_note_stdin_round_trip_byte_exact() -> Result<()> {
    let env = setup_env().await?;

    let email = "roundtripnotes@example.com";
    let password = "roundtripnotespassword123";
    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    client
        .send(Action::Login {
            email: email.to_string(),
            password: password.to_string(),
            server_url: Some(env.vault_url.clone()),
            remember_me: true,
            two_factor_token: None,
            two_factor_provider: None,
            two_factor_code: None,
            device_verification_code: None,
        })
        .await?;

    // Store: `cat credentials.yaml | cosmic-bwarden-cli note add "AWS Creds" --stdin`
    let (success, _stdout, stderr) = run_cli_with_stdin(
        &env,
        &["note", "add", "AWS Creds", "--stdin"],
        CREDENTIALS_YAML,
    )?;
    assert!(success, "CLI add --stdin failed: {stderr}");
    client.send(Action::Sync).await?;

    // Restore: `cosmic-bwarden-cli get "AWS Creds" --fields notes --show-secrets > credentials.yaml`
    // must reproduce the original file byte-for-byte, with no "Notes:" label
    // or other field mixed in.
    let output = env
        .cli_cmd()
        .args(["get", "AWS Creds", "--fields", "notes", "--show-secrets"])
        .output()?;
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("CLI stdout should be valid UTF-8");
    assert_eq!(
        stdout, CREDENTIALS_YAML,
        "expected raw --fields notes output to match the original file byte-for-byte"
    );

    // Requesting a different field must not leak the note body alongside it.
    let output = env
        .cli_cmd()
        .args(["get", "AWS Creds", "--fields", "name", "--show-secrets"])
        .output()?;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("aws_access_key_id"),
        "notes should not leak when --fields excludes notes, got: {stdout}"
    );

    Ok(())
}

#[tokio::test]
async fn test_add_stdin_conflicts_with_notes_arg() -> Result<()> {
    let env = setup_env().await?;

    let email = "conflictstdinnotes@example.com";
    let password = "conflictstdinnotespassword123";
    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    client
        .send(Action::Login {
            email: email.to_string(),
            password: password.to_string(),
            server_url: Some(env.vault_url.clone()),
            remember_me: true,
            two_factor_token: None,
            two_factor_provider: None,
            two_factor_code: None,
            device_verification_code: None,
        })
        .await?;

    let (success, _stdout, stderr) = run_cli_with_stdin(
        &env,
        &["note", "add", "Conflicting", "notes=inline", "--stdin"],
        "from stdin\n",
    )?;
    assert!(
        !success,
        "expected CLI to reject notes=... combined with --stdin"
    );
    assert!(
        stderr.to_lowercase().contains("stdin"),
        "expected error message to mention --stdin conflict, got: {stderr}"
    );

    Ok(())
}
