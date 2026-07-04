use crate::common::setup_env;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action, Response};

/// Verify the agent's Version response has valid format and includes both fields.
#[tokio::test]
async fn test_version_response_format() -> anyhow::Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    let res = client.send(Action::Version).await?;

    match res {
        Response::Version {
            version,
            protocol_version,
        } => {
            // Format: YYYY.MM-NNNNN-<git_id>
            assert!(!version.is_empty(), "version should not be empty");
            assert!(
                version.contains('.'),
                "version should contain a dot: {}",
                version
            );
            assert!(
                version.contains('-'),
                "version should contain a dash: {}",
                version
            );
            assert_eq!(
                protocol_version,
                cosmic_bwarden_core::PROTOCOL_VERSION,
                "protocol_version is the independent wire-protocol constant, \
                 not the build version"
            );
        }
        other => anyhow::bail!("Expected Version response, got: {:?}", other),
    }
    Ok(())
}

/// Verify the CLI version subcommand produces expected output.
#[tokio::test]
async fn test_cli_version_subcommand_match() -> anyhow::Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let output = env.cli_cmd().arg("version").output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "CLI version command failed.\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("Local version:"),
        "stdout should contain local version"
    );
    assert!(
        stdout.contains("Agent version:"),
        "stdout should contain agent version"
    );
    assert!(
        stdout.contains("Protocol version:"),
        "stdout should contain protocol version"
    );
    assert!(stdout.contains("OK"), "should report compatibility OK");

    Ok(())
}
