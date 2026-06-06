//! Verify that secret fields are masked when the `--show-secrets` flag is NOT used.

use crate::common::{register_user, setup_env};
use anyhow::Result;
use std::path::PathBuf;
use std::process::Command;

#[tokio::test]
async fn test_cli_secret_masked() -> Result<()> {
    // 1️⃣ Set up a fresh Vaultwarden container + agent
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    // 2️⃣ Register a user (same as other CLI tests)
    let email = "masked-test@example.com";
    let password = "maskedpass123";
    register_user(&env.vault_url, email, password).await?;

    // 3️⃣ Build the path to the compiled CLI binary
    let mut cli_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    cli_path.pop(); // …/crates/cosmic-bwarden-tests
    cli_path.pop(); // …/crates
    cli_path.push("target/debug/cosmic-bwarden-cli");

    // 4️⃣ Login
    let login_out = Command::new(&cli_path)
        .env("COSMIC_BWARDEN_PROFILE", &env.profile)
        .arg("login")
        .arg(email)
        .arg("--server")
        .arg(&env.vault_url)
        .arg("--password")
        .arg(password)
        .output()?;
    assert!(
        login_out.status.success(),
        "CLI login failed: {}",
        String::from_utf8_lossy(&login_out.stderr)
    );

    // 5️⃣ Add a Secure Note (no `-S` flag)
    let add_out = Command::new(&cli_path)
        .env("COSMIC_BWARDEN_PROFILE", &env.profile)
        .arg("add")
        .arg("MaskedNote")
        .arg("notes=This is a secret note")
        .arg("password=dummypassword")
        .output()?;
    assert!(
        add_out.status.success(),
        "CLI add failed: {}",
        String::from_utf8_lossy(&add_out.stderr)
    );

    // 6️⃣ Sync so the note is persisted on the server
    let sync_out = Command::new(&cli_path)
        .env("COSMIC_BWARDEN_PROFILE", &env.profile)
        .arg("sync")
        .output()?;
    assert!(
        sync_out.status.success(),
        "CLI sync failed: {}",
        String::from_utf8_lossy(&sync_out.stderr)
    );

    // 7️⃣ Retrieve the note **without** `-S`
    let get_out = Command::new(&cli_path)
        .env("COSMIC_BWARDEN_PROFILE", &env.profile)
        .arg("get")
        .arg("MaskedNote")
        .output()?;
    assert!(
        get_out.status.success(),
        "CLI get failed: {}",
        String::from_utf8_lossy(&get_out.stderr)
    );

    let stdout = String::from_utf8_lossy(&get_out.stdout);

    // 8️⃣ The secret note text must NOT appear in the output.
    assert!(
        !stdout.contains("This is a secret note"),
        "Secret note was exposed without `-S` flag"
    );

    // 9️⃣ Ensure the entry name appears, proving we got the right entry.
    assert!(
        stdout.contains("MaskedNote"),
        "Output does not contain the entry name"
    );

    Ok(())
}
