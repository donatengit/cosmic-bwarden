use std::sync::Arc;
use tokio::sync::Mutex;
use crate::state::State;
use futures_util::StreamExt;

pub async fn listen_to_logind(state: Arc<Mutex<State>>) -> zbus::Result<()> {
    let connection = zbus::Connection::system().await?;

    // Subscribe to Session Lock signal
    // org.freedesktop.login1.Session.Lock

    // Use zbus Proxy for easier signal handling if possible, or raw MatchRule
    // For simplicity, we can use a basic Stream of messages
    let mut stream = zbus::MessageStream::from(&connection);

    // We also want PrepareForShutdown
    // interface='org.freedesktop.login1.Manager', member='PrepareForShutdown'

    while let Some(msg) = stream.next().await {
        match msg {
            Ok(m) => {
                let header = m.header();
                let interface = header.interface();
                let member = header.member();

                if (interface.map(|i| i.as_str()) == Some("org.freedesktop.login1.Session")
                    && member.map(|m| m.as_str()) == Some("Lock"))
                    || (interface.map(|i| i.as_str()) == Some("org.freedesktop.login1.Manager")
                        && member.map(|m| m.as_str()) == Some("PrepareForShutdown"))
                {
                    log::info!("received lock/shutdown signal from logind, locking vault");
                    let mut state_guard = state.lock().await;
                    state_guard.lock();
                }
            }
            Err(e) => log::error!("logind message error: {}", e),
        }
    }

    Ok(())
}
