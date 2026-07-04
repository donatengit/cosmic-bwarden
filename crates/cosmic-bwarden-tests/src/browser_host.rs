use crate::common::setup_env;
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn test_browser_host_proxy_comprehensive() -> anyhow::Result<()> {
    let env = setup_env().await?;

    // Path to agent binary
    let mut agent_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    agent_path.pop();
    agent_path.pop();
    agent_path.push("target/debug/cosmic-bwarden-agent");

    // Spawn agent in browser-host mode
    let mut host_process = Command::new(&agent_path)
        .arg("--socket")
        .arg(&env.socket_path)
        .arg("--config")
        .arg(&env.config_path)
        .arg("browser-host")
        .env("COSMIC_BWARDEN_PROFILE", &env.profile)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;

    let mut stdin = host_process.stdin.take().unwrap();
    let mut stdout = host_process.stdout.take().unwrap();

    // Give it a moment to connect to the agent socket
    sleep(Duration::from_millis(500)).await;

    // Helper to send JSON and read JSON response
    let send_receive_raw = |req: serde_json::Value,
                            stdin_ref: &mut dyn Write,
                            stdout_ref: &mut dyn Read|
     -> anyhow::Result<serde_json::Value> {
        let request_json = serde_json::to_vec(&req)?;
        let len = request_json.len() as u32;
        stdin_ref.write_all(&len.to_ne_bytes())?;
        stdin_ref.write_all(&request_json)?;
        stdin_ref.flush()?;

        let mut len_buf = [0u8; 4];
        stdout_ref.read_exact(&mut len_buf)?;
        let len = u32::from_ne_bytes(len_buf) as usize;
        let mut resp_buf = vec![0u8; len];
        stdout_ref.read_exact(&mut resp_buf)?;

        let resp: serde_json::Value = serde_json::from_slice(&resp_buf)?;
        eprintln!("Request: {:?} -> Response: {:?}", req, resp);
        Ok(resp)
    };

    // 1. Test Version (Unit variant)
    let resp = send_receive_raw(serde_json::json!("Version"), &mut stdin, &mut stdout)?;
    assert!(resp.get("Version").is_some());

    // 2. Test GetConfig (Unit variant)
    let resp = send_receive_raw(serde_json::json!("GetConfig"), &mut stdin, &mut stdout)?;
    assert!(resp.get("Config").is_some());
    // Since it's a fresh env, it should need login or have no account
    assert!(resp["Config"].get("needs_login").is_some());

    // 3. Test Sync (Unit variant)
    let resp = send_receive_raw(serde_json::json!("Sync"), &mut stdin, &mut stdout)?;
    // Should return Error because not logged in
    assert!(resp.get("Error").is_some());

    // 4. Test GetSidebarEntries (Struct variant)
    let resp = send_receive_raw(
        serde_json::json!({
            "GetSidebarEntries": {
                "query": "test",
                "entry_type": null,
                "only_pinned": false
            }
        }),
        &mut stdin,
        &mut stdout,
    )?;
    // Should return SidebarEntries (empty list because not logged in/no data)
    assert!(resp.get("SidebarEntries").is_some() || resp.get("Error").is_some());

    // 5. Test Lock (Unit variant)
    let resp = send_receive_raw(serde_json::json!("Lock"), &mut stdin, &mut stdout)?;
    // Response::Ack is a unit variant, so it's serialized as the string "Ack"
    assert!(resp.as_str() == Some("Ack") || resp.get("Error").is_some());

    // 6. Test GetPassword (Struct variant)
    let resp = send_receive_raw(
        serde_json::json!({
            "GetPassword": {
                "id": "some-id",
                "password": null
            }
        }),
        &mut stdin,
        &mut stdout,
    )?;
    // Should return Error (not found or locked)
    assert!(resp.get("Error").is_some());

    // 7. Test GetTotp
    let resp = send_receive_raw(
        serde_json::json!({
            "GetTotp": {
                "id": "some-id"
            }
        }),
        &mut stdin,
        &mut stdout,
    )?;
    assert!(resp.get("Error").is_some());

    // 8. Test AddEntry (Complex struct)
    let resp = send_receive_raw(
        serde_json::json!({
            "AddEntry": {
                "name": "Test Entry",
                "entry_type": "Login",
                "username": "testuser",
                "password": "testpassword",
                "notes": "some notes",
                "fields": [
                    {
                        "ty": 0,
                        "name": "CustomField",
                        "value": "CustomValue",
                        "linked_id": null
                    }
                ]
            }
        }),
        &mut stdin,
        &mut stdout,
    )?;
    // Should return Error (locked) but verifies serialization/routing
    assert!(resp.get("Error").is_some());
    let err_msg = resp["Error"]["message"].as_str().unwrap();
    assert!(err_msg.contains("locked") || err_msg.contains("logged in"));

    // 9. Test Invalid Protocol Message (Negative test)
    // Send something that isn't a valid Action
    let req = serde_json::json!({"InvalidAction": "blah"});
    let request_json = serde_json::to_vec(&req)?;
    let len = request_json.len() as u32;
    stdin.write_all(&len.to_ne_bytes())?;
    stdin.write_all(&request_json)?;
    stdin.flush()?;

    let mut len_buf = [0u8; 4];
    stdout.read_exact(&mut len_buf)?;
    let len = u32::from_ne_bytes(len_buf) as usize;
    let mut resp_buf = vec![0u8; len];
    stdout.read_exact(&mut resp_buf)?;
    let resp: serde_json::Value = serde_json::from_slice(&resp_buf)?;
    eprintln!("Invalid Request Response: {:?}", resp);
    assert!(resp.get("Error").is_some());
    assert!(resp["Error"]["message"]
        .as_str()
        .unwrap()
        .contains("Invalid protocol message"));

    // 10. Test Quit
    let _ = send_receive_raw(serde_json::json!("Quit"), &mut stdin, &mut stdout)?;

    // Host process should eventually exit
    let mut count = 0;
    while count < 20 {
        if let Ok(Some(_)) = host_process.try_wait() {
            return Ok(());
        }
        sleep(Duration::from_millis(100)).await;
        count += 1;
    }

    let _ = host_process.kill();
    Ok(())
}
