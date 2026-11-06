// Full-lifecycle Firefox E2E against a real agent and Vaultwarden. The
// geckodriver/Firefox harness — extension packaging, profile, UUID discovery,
// and the waits that turn async popup renders into assertions — lives in
// `browser.rs`.
mod browser;

use crate::common::setup_env;
use thirtyfour::prelude::*;
use tokio::time::{sleep, Duration};

use browser::BrowserTest;

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
        println!(
            "Native host log ({}):\n{}",
            bt.host_log.display(),
            std::fs::read_to_string(&bt.host_log).unwrap_or_default()
        );
        panic!("Unexpected initial status text: '{}'", status_text);
    }

    // 2. Unlock via Agent directly
    let client =
        cosmarden_core::agent_client::AgentClient::new_with_socket(env.socket_path.clone());
    crate::common::register_user(&env.vault_url, "test@example.com", "password123").await?;
    client
        .send(cosmarden_core::protocol::Action::Login {
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
        .send(cosmarden_core::protocol::Action::Unlock {
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
    // The add form pre-fills the name with the active tab's domain, which for
    // an extension tab is its UUID. Replace it instead of appending.
    let name_input = bt.driver.query(By::Id("edit-name")).first().await?;
    name_input.clear().await?;
    name_input.send_keys("Full Login").await?;
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

    // Pin the new entry: with no real site tab open the popup falls back to
    // favourites, so an unpinned entry never appears in the list.
    let res = client
        .send(cosmarden_core::protocol::Action::GetSidebarEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
            domain: None,
        })
        .await?;
    let entry_id = match res {
        cosmarden_core::protocol::Response::SidebarEntries { entries } => entries
            .iter()
            .find(|e| e.name == "Full Login")
            .map(|e| e.id.clone()),
        _ => None,
    }
    .ok_or_else(|| anyhow::anyhow!("added entry not found via GetSidebarEntries"))?;
    client
        .send(cosmarden_core::protocol::Action::PinEntry { id: entry_id })
        .await?;

    // 5. Persistence Cycle: Lock/Unlock
    client.send(cosmarden_core::protocol::Action::Lock).await?;
    bt.open_popup().await?;

    // The locked state is a view, not the status bar: `#status` is hidden and
    // the popup swaps in `#view-locked`.
    let mut locked_visible = false;
    for _ in 0..10 {
        locked_visible = bt
            .driver
            .execute(
                "return !document.getElementById('view-locked').classList.contains('hidden');",
                vec![],
            )
            .await?
            .convert::<bool>()?;
        if locked_visible {
            break;
        }
        sleep(Duration::from_millis(500)).await;
    }
    assert!(locked_visible, "locked view did not appear after Lock");

    client
        .send(cosmarden_core::protocol::Action::Unlock {
            password: "password123".to_string(),
        })
        .await?;
    bt.open_popup().await?;
    sleep(Duration::from_secs(2)).await;

    // 6. Verify Entry and Fields. Clicking a Login row opens its site, so use
    // the eye button to reach the detail view.
    bt.click_view_details("Full Login").await?;
    bt.expect_detail("myuser").await?;
    // Notes are a secret-class slot: the meta-backed detail view shows the
    // label but never the plaintext.
    bt.expect_detail("Notes").await?;

    // 7. Verify Copy functionality. The first Copy button belongs to Username;
    // scope the selector to the Password secret row.
    bt.driver
        .query(By::XPath(
            "//div[contains(@class,'detail-item')][.//*[contains(@class,'secret-text')]]\
             //button[contains(@class,'copy-btn')]",
        ))
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
        .send(cosmarden_core::protocol::Action::Logout)
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
        .send(cosmarden_core::protocol::Action::Login {
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
        .send(cosmarden_core::protocol::Action::Unlock {
            password: "password123".to_string(),
        })
        .await?;

    bt.open_popup().await?;
    sleep(Duration::from_secs(2)).await;
    bt.click_view_details("Full Login").await?;
    bt.expect_detail("myuser").await?;

    bt.cleanup().await?;
    Ok(())
}
