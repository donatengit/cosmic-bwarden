// Geckodriver/Firefox harness for the real-agent extension E2E: package the
// extension into an XPI, install it in a temporary profile, discover the
// moz-extension UUID, and drive the popup. The test flow itself lives in
// `extension_e2e/mod.rs`.
use crate::common::TestEnv;
use serde_json::json;
use std::io::{Read, Write};
use std::net::TcpListener;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::process::{Child, Command};
use tempfile::tempdir;
use thirtyfour::prelude::*;
use tokio::time::{sleep, Duration};
use zip::write::SimpleFileOptions;

// Kill geckodriver on every exit path, including a panic between spawn and the
// point where the test owns a BrowserTest. Without this, a failed WebDriver
// handshake leaves geckodriver (and the Firefox it spawned) behind.
struct GeckoDriver(Child);

impl Drop for GeckoDriver {
    fn drop(&mut self) {
        // geckodriver spawns Firefox as a child and does not reliably reap it
        // when killed, so start both in their own process group and kill the
        // whole group. A leaked headless Firefox holds a profile dir and a
        // memory-mapped cache for the rest of the test run.
        #[cfg(unix)]
        unsafe {
            libc::kill(-(self.0.id() as i32), libc::SIGKILL);
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub(super) struct BrowserTest {
    pub(super) driver: WebDriver,
    _geckodriver: GeckoDriver,
    _test_home: tempfile::TempDir,
    extension_url: String,
    pub(super) host_log: std::path::PathBuf,
}

fn free_port() -> anyhow::Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}

impl BrowserTest {
    pub(super) async fn setup(env: &TestEnv) -> anyhow::Result<Self> {
        let project_root = std::env::current_dir()?
            .parent()
            .ok_or_else(|| anyhow::anyhow!("no parent"))?
            .parent()
            .ok_or_else(|| anyhow::anyhow!("no grandparent"))?
            .to_path_buf();
        let agent_bin = project_root.join("target/debug/cosmarden-agent");
        let ext_dir = project_root.join("browser-extension");

        let test_home = tempdir()?;
        let manifest_dir = test_home.path().join(".mozilla/native-messaging-hosts");
        std::fs::create_dir_all(&manifest_dir)?;

        let wrapper_path = test_home.path().join("cosmarden-browser-host-wrapper.sh");
        let log_path = test_home.path().join("browser-host.log");

        let config_home = env._temp_dir.path().join("config");
        let cache_home = env._temp_dir.path().join("cache");
        let data_home = env._temp_dir.path().join("data");

        let wrapper_content = format!(
            "#!/bin/bash\n\
            export HOME={home}\n\
            export COSMARDEN_PROFILE={profile}\n\
            export XDG_CONFIG_HOME={config_home}\n\
            export XDG_CACHE_HOME={cache}\n\
            export XDG_DATA_HOME={data}\n\
            export RUST_LOG=debug\n\
            exec {agent} --socket {socket} --config {config} browser-host 2>> {log}\n",
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

        let host_name = "com.enikeev.cosmarden";
        let manifest_content = json!({
            "name": host_name,
            "description": "Cosmarden Test Host",
            "path": wrapper_path.to_str().unwrap(),
            "type": "stdio",
            "allowed_extensions": ["cosmarden@enikeev.com"]
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

        let port = free_port()?;
        // `--allow-system-access` is required to touch a moz-extension:// page;
        // without it Marionette answers "does not support browsing contexts in
        // privileged scope". geckodriver gained the flag in 0.37.
        let mut geckodriver_cmd = Command::new("geckodriver");
        geckodriver_cmd
            .arg("--port")
            .arg(port.to_string())
            .arg("--allow-system-access")
            .env("HOME", test_home.path());
        #[cfg(unix)]
        geckodriver_cmd.process_group(0);
        let geckodriver = GeckoDriver(geckodriver_cmd.spawn()?);

        sleep(Duration::from_secs(1)).await;

        let mut caps = DesiredCapabilities::firefox();
        caps.add_arg("-headless")?;
        caps.set_browser_option("env", json!({ "HOME": test_home.path().to_str().unwrap() }))?;

        let driver = WebDriver::new(format!("http://localhost:{port}"), caps).await?;

        // Install extension
        let session_id = driver.session_id().to_string();
        let client = reqwest::Client::new();
        client
            .post(format!(
                "http://localhost:{port}/session/{}/moz/addon/install",
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
            _geckodriver: geckodriver,
            _test_home: test_home,
            extension_url,
            host_log: log_path,
        })
    }

    pub(super) async fn open_popup(&self) -> anyhow::Result<()> {
        self.driver.goto(&self.extension_url).await?;
        sleep(Duration::from_secs(1)).await;
        self.driver
            .execute(
                "window.mockClipboard = ''; \
             Object.defineProperty(navigator, 'clipboard', { \
                 configurable: true, \
                 value: { \
                     writeText: (t) => { window.mockClipboard = t; return Promise.resolve(); }, \
                     readText: () => Promise.resolve(window.mockClipboard) \
                 } \
             });",
                vec![],
            )
            .await?;
        Ok(())
    }

    /// Waits for the list row for `entry_name`, then clicks its eye button.
    ///
    /// The list is rendered by an async `GetConfig` + `GetSidebarEntries`
    /// round trip through the background script and the native host, so a
    /// query issued right after `open_popup` can run before the row exists.
    pub(super) async fn click_view_details(&self, entry_name: &str) -> anyhow::Result<()> {
        let xpath = format!(
            "//div[contains(@class,'entry')]\
             [.//div[contains(@class,'entry-name') and text()='{entry_name}']]\
             //button[@aria-label='View details']"
        );
        for _ in 0..20 {
            if let Ok(button) = self.driver.query(By::XPath(&xpath)).first().await {
                button.click().await?;
                return Ok(());
            }
            sleep(Duration::from_millis(500)).await;
        }
        anyhow::bail!(
            "no 'View details' button for entry {entry_name:?} appeared in the list\n\
             Page source:\n{}",
            self.driver.source().await?
        )
    }

    /// Waits for the detail view to render `needle`, and reports what was on
    /// screen instead when it never does.
    ///
    /// `showDetail` fills `#detail-content` from an async `GetEntryMeta` round
    /// trip, so reading the element right after the click races the render.
    /// That race failed the post-relogin check in the nightly run on a runner
    /// slow enough for the round trip to outlast the immediate read.
    pub(super) async fn expect_detail(&self, needle: &str) -> anyhow::Result<()> {
        let mut text = String::new();
        for _ in 0..20 {
            text = self
                .driver
                .query(By::Id("detail-content"))
                .first()
                .await?
                .text()
                .await?;
            if text.contains(needle) {
                return Ok(());
            }
            sleep(Duration::from_millis(500)).await;
        }
        let status = self
            .driver
            .execute(
                "return document.getElementById('status').textContent;",
                vec![],
            )
            .await?
            .convert::<String>()?;
        anyhow::bail!(
            "detail view never showed {needle:?}; status = {status:?}, content = {text:?}\n\
             Native host log ({}):\n{}",
            self.host_log.display(),
            std::fs::read_to_string(&self.host_log).unwrap_or_default()
        )
    }

    pub(super) async fn cleanup(self) -> anyhow::Result<()> {
        let _ = self.driver.quit().await;
        Ok(())
    }
}
