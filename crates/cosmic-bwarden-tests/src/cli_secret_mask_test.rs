//! Verify that secret fields are masked when the `--show-secrets` flag is NOT used.

use crate::common::{register_user, setup_env};
use anyhow::Result;

#[tokio::test]
async fn test_cli_secret_masked() -> Result<()> {
    // 1️⃣ Set up a fresh Vaultwarden container + agent
    let env = setup_env().await?;

    // 2️⃣ Register a user (same as other CLI tests)
    let email = "masked-test@example.com";
    let password = "maskedpass123";
    register_user(&env.vault_url, email, password).await?;

    // 4️⃣ Login (master password is prompted interactively — drive it via a pty)
    let (success, _stdout, stderr) = env.run_cli_with_tty(
        &["login", email, "--server", env.vault_url.as_str()],
        &format!("{password}\n\n"),
    )?;
    assert!(success, "CLI login failed: {stderr}");

    // 5️⃣ Add a Secure Note (no `-S` flag)
    let add_out = env.cli_cmd()
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
    let sync_out = env.cli_cmd()
        .arg("sync")
        .output()?;
    assert!(
        sync_out.status.success(),
        "CLI sync failed: {}",
        String::from_utf8_lossy(&sync_out.stderr)
    );

    // 7️⃣ Retrieve the note **without** `-S`
    let get_out = env.cli_cmd()
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
