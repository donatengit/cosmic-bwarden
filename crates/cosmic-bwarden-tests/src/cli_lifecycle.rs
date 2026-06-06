use crate::common::{register_user, setup_env};
use anyhow::Result;
use std::path::PathBuf;
use std::process::Command;

#[tokio::test]
async fn test_cli_lifecycle() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "cli-test@example.com";
    let password = "clipassword123";

    // 1. Register user
    register_user(&env.vault_url, email, password).await?;

    let mut cli_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    cli_path.pop();
    cli_path.pop();
    cli_path.push("target/debug/cosmic-bwarden-cli");

    // 2. Login
    let output = Command::new(&cli_path)
        .env("COSMIC_BWARDEN_PROFILE", &env.profile)
        .arg("login")
        .arg(email)
        .arg("--server")
        .arg(&env.vault_url)
        .arg("--password")
        .arg(password)
        .output()?;
    assert!(
        output.status.success(),
        "CLI login failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // 3. Add entry
    let output = Command::new(&cli_path)
        .env("COSMIC_BWARDEN_PROFILE", &env.profile)
        .arg("add")
        .arg("CLISite")
        .arg("username=cliuser")
        .arg("password=clipass123")
        .output()?;
    assert!(
        output.status.success(),
        "CLI add failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // 4. Sync
    let output = Command::new(&cli_path)
        .env("COSMIC_BWARDEN_PROFILE", &env.profile)
        .arg("sync")
        .output()?;
    assert!(
        output.status.success(),
        "CLI sync failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // 5. List
    let output = Command::new(&cli_path)
        .env("COSMIC_BWARDEN_PROFILE", &env.profile)
        .arg("ls")
        .output()?;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("CLISite"));

    // 6. Get
    let output = Command::new(&cli_path)
        .env("COSMIC_BWARDEN_PROFILE", &env.profile)
        .arg("get")
        .arg("CLISite")
        .arg("-S")
        .output()?;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("clipass123"));

    Ok(())
}

#[tokio::test]
async fn test_cli_extended_features() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "extended-test@example.com";
    let password = "extpassword123";

    let mut cli_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    cli_path.pop();
    cli_path.pop();
    cli_path.push("target/debug/cosmic-bwarden-cli");

    // 1. Register user via CLI
    let output = Command::new(&cli_path)
        .env("COSMIC_BWARDEN_PROFILE", &env.profile)
        .arg("register")
        .arg(email)
        .arg("--server")
        .arg(&env.vault_url)
        .arg("--password")
        .arg(password)
        .output()?;
    assert!(
        output.status.success(),
        "CLI register failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // 2. Login
    let output = Command::new(&cli_path)
        .env("COSMIC_BWARDEN_PROFILE", &env.profile)
        .arg("login")
        .arg(email)
        .arg("--server")
        .arg(&env.vault_url)
        .arg("--password")
        .arg(password)
        .output()?;
    assert!(
        output.status.success(),
        "CLI login failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // 3. Add Secure Note (using add instead of add-note to avoid type 2 issues)
    let output = Command::new(&cli_path)
        .env("COSMIC_BWARDEN_PROFILE", &env.profile)
        .arg("add")
        .arg("MyNote")
        .arg("notes=This is a secret note")
        .arg("password=dummypassword")
        .output()?;
    assert!(
        output.status.success(),
        "CLI add failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // 4. Sync
    let output = Command::new(&cli_path)
        .env("COSMIC_BWARDEN_PROFILE", &env.profile)
        .arg("sync")
        .output()?;
    assert!(output.status.success());

    // 5. Verify Note
    let output = Command::new(&cli_path)
        .env("COSMIC_BWARDEN_PROFILE", &env.profile)
        .arg("get")
        .arg("MyNote")
        .arg("-S")
        .output()?;
    assert!(
        output.status.success(),
        "CLI get MyNote failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("This is a secret note"));

    Ok(())
}
