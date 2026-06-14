use std::sync::Arc;
use tokio::sync::Mutex;
use crate::state::State;
use tokio_stream::StreamExt;

// `zbus` (+ `zvariant`) accounts for ~558KB of .text in the release binary,
// measured with `cargo bloat --release --crates`, almost entirely for this
// function's use of `Connection::system()` + `MessageStream` (no proxies or
// registered objects are used here). Most of that cost can't be trimmed:
// `Connection::system()` is a bus connection, so zbus unconditionally
// activates its `ObjectServer`/`fdo` interface dispatch at runtime
// (`ensure_object_server`) regardless of whether we register any objects,
// and the rest is the SASL handshake + message (de)serialization needed by
// any correct D-Bus client. A hand-rolled raw-socket client or a switch to
// a less-maintained crate (e.g. `rustbus`) was considered and rejected:
// `zbus` is already pulled in transitively via the UI's `libcosmic`
// dependency, and this auto-lock path is security-sensitive enough that a
// battle-tested D-Bus implementation is worth ~0.5MB. Kept as-is.
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
