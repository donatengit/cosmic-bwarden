use std::sync::Arc;
use tokio::sync::Mutex;
use crate::state::State;
use tokio_stream::StreamExt;
use zbus::MatchRule;

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

    // On the D-Bus system bus, the daemon filters messages by default.
    // Without explicit AddMatch calls the daemon never routes broadcast
    // signals to this connection — MessageStream would receive nothing.
    // Register each signal we care about before entering the stream loop.
    let proxy = zbus::fdo::DBusProxy::new(&connection).await?;
    for (interface, member) in [
        ("org.freedesktop.login1.Session", "Lock"),
        ("org.freedesktop.login1.Manager", "PrepareForShutdown"),
        ("org.freedesktop.login1.Manager", "PrepareForSleep"),
    ] {
        let rule = MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface(interface).expect("valid interface name")
            .member(member).expect("valid member name")
            .build();
        proxy.add_match_rule(rule).await?;
    }

    let mut stream = zbus::MessageStream::from(&connection);

    while let Some(msg) = stream.next().await {
        match msg {
            Ok(m) => {
                let header = m.header();
                let interface = header.interface();
                let member = header.member();

                let iface = interface.map(|i| i.as_str());
                let mbr = member.map(|m| m.as_str());
                let should_lock =
                    (iface == Some("org.freedesktop.login1.Session") && mbr == Some("Lock"))
                    || (iface == Some("org.freedesktop.login1.Manager")
                        && (mbr == Some("PrepareForShutdown") || mbr == Some("PrepareForSleep")));
                if should_lock {
                    let reason = match mbr {
                        Some("Lock") => "session lock",
                        Some("PrepareForShutdown") => "system shutdown",
                        Some("PrepareForSleep") => "system sleep/suspend",
                        _ => "logind signal",
                    };
                    log::info!("vault locked: {} (signal: {}/{})",
                        reason,
                        iface.unwrap_or("?"),
                        mbr.unwrap_or("?"));
                    let mut state_guard = state.lock().await;
                    state_guard.lock();
                }
            }
            Err(e) => log::error!("logind message error: {}", e),
        }
    }

    // Stream ended — D-Bus connection was dropped. This causes the agent to
    // exit via the select! in main (logind task completing is treated as a
    // shutdown signal), so log at error level before returning.
    log::error!("logind: D-Bus system bus connection closed unexpectedly — lock-on-sleep/suspend will not work until agent restarts");
    Ok(())
}
