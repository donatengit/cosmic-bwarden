use crate::common::{register_user, setup_env};
use anyhow::Result;

#[tokio::test]
async fn test_cli_lifecycle() -> Result<()> {
    let env = setup_env().await?;

    let email = "cli-test@example.com";
    let password = "clipassword123";

    // 1. Register user
    register_user(&env.vault_url, email, password).await?;

    // 2. Login
    let output = env
        .cli_cmd()
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
    let output = env
        .cli_cmd()
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
    let output = env.cli_cmd().arg("sync").output()?;
    assert!(
        output.status.success(),
        "CLI sync failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // 5. List
    let output = env.cli_cmd().arg("ls").output()?;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("CLISite"));

    // 6. Get
    let output = env.cli_cmd().arg("get").arg("CLISite").arg("-S").output()?;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("clipass123"));

    Ok(())
}

#[tokio::test]
async fn test_cli_extended_features() -> Result<()> {
    let env = setup_env().await?;

    let email = "extended-test@example.com";
    let password = "extpassword123";

    // 1. Register user via CLI
    let output = env
        .cli_cmd()
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
    let output = env
        .cli_cmd()
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
    let output = env
        .cli_cmd()
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
    let output = env.cli_cmd().arg("sync").output()?;
    assert!(output.status.success());

    // 5. Verify Note
    let output = env.cli_cmd().arg("get").arg("MyNote").arg("-S").output()?;
    assert!(
        output.status.success(),
        "CLI get MyNote failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("This is a secret note"));

    Ok(())
}
