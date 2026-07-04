use crate::common::{setup_env, TestEnv};
use serde_json::json;
use std::io::{Read, Write};
use std::process::Command;
use tempfile::tempdir;
use thirtyfour::prelude::*;
use tokio::time::{sleep, Duration};
use zip::write::SimpleFileOptions;

struct BrowserTest {
    driver: WebDriver,
    geckodriver: std::process::Child,
    test_home: tempfile::TempDir,
    extension_url: String,
}

impl BrowserTest {
    async fn setup(env: &TestEnv) -> anyhow::Result<Self> {
        let project_root = std::env::current_dir()?
            .parent()
            .ok_or_else(|| anyhow::anyhow!("no parent"))?
            .parent()
            .ok_or_else(|| anyhow::anyhow!("no grandparent"))?
            .to_path_buf();
        let agent_bin = project_root.join("target/debug/cosmic-bwarden-agent");
        let ext_dir = project_root.join("browser-extension");

        let test_home = tempdir()?;
        let manifest_dir = test_home.path().join(".mozilla/native-messaging-hosts");
        std::fs::create_dir_all(&manifest_dir)?;

        let wrapper_path = test_home
            .path()
            .join("cosmic-bwarden-browser-host-wrapper.sh");
        let log_path = test_home.path().join("browser-host.log");

        let config_home = env._temp_dir.path().join("config");
        let cache_home = env._temp_dir.path().join("cache");
        let data_home = env._temp_dir.path().join("data");

        let wrapper_content = format!(
            "#!/bin/bash\n\
            export HOME={home}\n\
            export COSMIC_BWARDEN_PROFILE={profile}\n\
            export XDG_CONFIG_HOME={config_home}\n\
            export XDG_CACHE_HOME={cache}\n\
            export XDG_DATA_HOME={data}\n\
            export RUST_LOG=debug\n\
            exec {agent} --socket {socket} --config {config} browser-host \"$@\" 2>> {log}\n",
            home = test_home.path().display(),
            profile = env.profile,
            config_home = config_home.display(),
            cache = cache_home.display(),
            data = data_home.display(),
            agent = agent_bin.display(),
            socket = env.socket_path.display(),
            config = env.config_path.display(),
            log = log_path.display()
        );
        std::fs::write(&wrapper_path, wrapper_content)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&wrapper_path, std::fs::Permissions::from_mode(0o755))?;
        }

        let host_name = "com.8bit.cosmic_bwarden";
        let manifest_content = json!({
            "name": host_name,
            "description": "COSMIC BWarden Test Host",
            "path": wrapper_path.to_str().unwrap(),
            "type": "stdio",
            "allowed_extensions": ["cosmic-bwarden@8bit.com"]
        });
        std::fs::write(
            manifest_dir.join(format!("{}.json", host_name)),
            serde_json::to_string_pretty(&manifest_content)?,
        )?;

        // Package extension
        let xpi_path = test_home.path().join("extension.xpi");
        {
            let file = std::fs::File::create(&xpi_path)?;
            let mut zip = zip::ZipWriter::new(file);
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

            for entry in walkdir::WalkDir::new(&ext_dir) {
                let entry = entry?;
                let path = entry.path();
                let name = path.strip_prefix(&ext_dir)?;

                if path.is_file() {
                    let name_str = name.to_str().unwrap();
                    if name_str.contains("node_modules") || name_str.contains(".git") {
                        continue;
                    }
                    zip.start_file(name_str, options)?;
                    if name_str == "manifest.json" {
                        let content = std::fs::read_to_string(path)?;
                        let mut json: serde_json::Value = serde_json::from_str(&content)?;
                        if let Some(bg) = json.get_mut("background") {
                            bg["persistent"] = json!(true);
                        }
                        zip.write_all(serde_json::to_string_pretty(&json)?.as_bytes())?;
                    } else if name_str == "background.js" {
                        let mut content = std::fs::read_to_string(path)?;
                        content.push_str("\nbrowser.tabs.create({ url: 'https://example.com/?uuid=' + browser.runtime.getURL('').split('/')[2] });\n");
                        zip.write_all(content.as_bytes())?;
                    } else {
                        let mut f = std::fs::File::open(path)?;
                        let mut buffer = Vec::new();
                        f.read_to_end(&mut buffer)?;
                        zip.write_all(&buffer)?;
                    }
                }
            }
            zip.finish()?;
        }

        let geckodriver = Command::new("geckodriver")
            .arg("--port")
            .arg("4444")
            .env("HOME", test_home.path())
            .spawn()?;

        sleep(Duration::from_secs(1)).await;

        let mut caps = DesiredCapabilities::firefox();
        caps.add_firefox_arg("-headless")?;
        caps.add_firefox_option("env", json!({ "HOME": test_home.path().to_str().unwrap() }))?;

        let driver = WebDriver::new("http://localhost:4444", caps).await?;

        // Install extension
        let session_id = driver.session_id().await?;
        let client = reqwest::Client::new();
        client
            .post(format!(
                "http://localhost:4444/session/{}/moz/addon/install",
                session_id
            ))
            .json(&json!({ "path": xpi_path.to_str().unwrap(), "temporary": true }))
            .send()
            .await?;

        // Discover UUID
        let mut uuid = None;
        for _ in 0..20 {
            sleep(Duration::from_millis(500)).await;
            let windows = driver.windows().await?;
            for win in windows {
                driver.switch_to_window(win).await?;
                let url = driver.current_url().await?;
                if url.to_string().contains("example.com/?uuid=") {
                    uuid = url
                        .query()
                        .and_then(|q| q.split('=').nth(1))
                        .map(|s| s.to_string());
                    break;
                }
            }
            if uuid.is_some() {
                break;
            }
        }

        let uuid = uuid.ok_or_else(|| anyhow::anyhow!("Failed to discover extension UUID"))?;
        let extension_url = format!("moz-extension://{}/popup/popup.html", uuid);

        Ok(Self {
            driver,
            geckodriver,
            test_home,
            extension_url,
        })
    }

    async fn open_popup(&self) -> anyhow::Result<()> {
        self.driver.goto(&self.extension_url).await?;
        sleep(Duration::from_secs(1)).await;
        self.driver
            .execute(
                "window.mockClipboard = ''; \
             navigator.clipboard = { \
                 writeText: (t) => { window.mockClipboard = t; return Promise.resolve(); }, \
                 readText: () => Promise.resolve(window.mockClipboard) \
             };",
                vec![],
            )
            .await?;
        Ok(())
    }

    async fn cleanup(mut self) -> anyhow::Result<()> {
        let _ = self.driver.quit().await;
        let _ = self.geckodriver.kill();
        Ok(())
    }
}

#[tokio::test]
async fn test_extension_full_lifecycle_persistence() -> anyhow::Result<()> {
    let env = setup_env().await?;
    let bt = BrowserTest::setup(&env).await?;

    // 1. Initial State: Should show "Vault is locked" or "Not logged in"
    bt.open_popup().await?;

    let mut status_text = String::new();
    for _ in 0..20 {
        status_text = bt
            .driver
            .execute(
                "return document.getElementById('status').textContent;",
                vec![],
            )
            .await?
            .convert::<String>()?
            .to_lowercase();

        if !status_text.is_empty() && status_text != "loading..." {
            break;
        }
        sleep(Duration::from_millis(500)).await;
    }

    if !(status_text.contains("locked")
        || status_text.contains("logged in")
        || status_text.contains("communicat"))
    {
        println!("Page Source (Initial):\n{}", bt.driver.source().await?);
        panic!("Unexpected initial status text: '{}'", status_text);
    }

    // 2. Unlock via Agent directly
    let client =
        cosmic_bwarden_core::agent_client::AgentClient::new_with_socket(env.socket_path.clone());
    crate::common::register_user(&env.vault_url, "test@example.com", "password123").await?;
    client
        .send(cosmic_bwarden_core::protocol::Action::Login {
            email: "test@example.com".to_string(),
            password: "password123".to_string(),
            server_url: Some(env.vault_url.clone()),
            remember_me: true,
            two_factor_token: None,
            two_factor_provider: None,
            two_factor_code: None,
            device_verification_code: None,
        })
        .await?;
    client
        .send(cosmic_bwarden_core::protocol::Action::Unlock {
            password: "password123".to_string(),
        })
        .await?;

    // 3. Verify Popup Unlocked
    bt.open_popup().await?;
    sleep(Duration::from_secs(2)).await;
    let results = bt.driver.query(By::Id("results")).first().await?;
    let results_text = results.text().await?;
    if !results_text.contains("No entries found") {
        println!("Page Source (Unlocked):\n{}", bt.driver.source().await?);
        panic!(
            "Results text doesn't contain 'No entries found': '{}'",
            results_text
        );
    }

    // 4. Add Entry
    bt.driver
        .query(By::Id("add-btn"))
        .first()
        .await?
        .click()
        .await?;
    bt.driver
        .query(By::Id("edit-name"))
        .first()
        .await?
        .send_keys("Full Login")
        .await?;
    bt.driver
        .query(By::Id("f-username"))
        .first()
        .await?
        .send_keys("myuser")
        .await?;
    bt.driver
        .query(By::Id("f-password"))
        .first()
        .await?
        .send_keys("mypassword")
        .await?;
    bt.driver
        .query(By::Id("f-totp"))
        .first()
        .await?
        .send_keys("JBSWY3DPEHPK3PXP")
        .await?;
    bt.driver
        .query(By::Id("edit-notes"))
        .first()
        .await?
        .send_keys("Sensitive notes")
        .await?;
    bt.driver
        .query(By::Id("save-btn"))
        .first()
        .await?
        .click()
        .await?;

    sleep(Duration::from_secs(2)).await;

    // 5. Persistence Cycle: Lock/Unlock
    client
        .send(cosmic_bwarden_core::protocol::Action::Lock)
        .await?;
    bt.open_popup().await?;

    let mut status_text = String::new();
    for _ in 0..10 {
        status_text = bt
            .driver
            .execute(
                "return document.getElementById('status').textContent;",
                vec![],
            )
            .await?
            .convert::<String>()?
            .to_lowercase();
        if status_text.contains("locked") {
            break;
        }
        sleep(Duration::from_millis(500)).await;
    }
    assert!(status_text.contains("locked"));

    client
        .send(cosmic_bwarden_core::protocol::Action::Unlock {
            password: "password123".to_string(),
        })
        .await?;
    bt.open_popup().await?;
    sleep(Duration::from_secs(2)).await;

    // 6. Verify Entry and Fields
    bt.driver
        .query(By::XPath("//div[contains(text(), 'Full Login')]"))
        .first()
        .await?
        .click()
        .await?;
    let detail = bt
        .driver
        .query(By::Id("detail-content"))
        .first()
        .await?
        .text()
        .await?;
    assert!(detail.contains("myuser"));
    assert!(detail.contains("Sensitive notes"));

    // 7. Verify Copy functionality
    bt.driver
        .query(By::XPath("//button[text()='Copy']"))
        .first()
        .await?
        .click()
        .await?;
    let clipboard = bt
        .driver
        .execute("return window.mockClipboard;", vec![])
        .await?
        .convert::<String>()?;
    assert_eq!(clipboard, "mypassword");

    // 8. Logout/Login Persistence
    client
        .send(cosmic_bwarden_core::protocol::Action::Logout)
        .await?;
    bt.open_popup().await?;

    let mut status_text = String::new();
    for _ in 0..10 {
        status_text = bt
            .driver
            .execute(
                "return document.getElementById('status').textContent;",
                vec![],
            )
            .await?
            .convert::<String>()?
            .to_lowercase();
        if status_text.contains("not logged in") {
            break;
        }
        sleep(Duration::from_millis(500)).await;
    }
    assert!(status_text.contains("not logged in"));

    client
        .send(cosmic_bwarden_core::protocol::Action::Login {
            email: "test@example.com".to_string(),
            password: "password123".to_string(),
            server_url: Some(env.vault_url.clone()),
            remember_me: true,
            two_factor_token: None,
            two_factor_provider: None,
            two_factor_code: None,
            device_verification_code: None,
        })
        .await?;
    client
        .send(cosmic_bwarden_core::protocol::Action::Unlock {
            password: "password123".to_string(),
        })
        .await?;

    bt.open_popup().await?;
    sleep(Duration::from_secs(2)).await;
    bt.driver
        .query(By::XPath("//div[contains(text(), 'Full Login')]"))
        .first()
        .await?
        .click()
        .await?;
    assert!(bt
        .driver
        .query(By::Id("detail-content"))
        .first()
        .await?
        .text()
        .await?
        .contains("myuser"));

    bt.cleanup().await?;
    Ok(())
}
