use crate::state::State;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_stream::StreamExt;
use zbus::MatchRule;

/// Take a logind *delay* inhibitor for sleep+shutdown. While the returned fd
/// is held, logind waits (up to `InhibitDelayMaxSec`, default 5s) for it to be
/// closed before suspending/hibernating/shutting down — guaranteeing
/// `State::lock()`'s zeroize completes before a hibernate image of RAM is
/// written to disk. Without it, the agent merely races the suspend and usually
/// (but not provably) wins. Best-effort: on failure (e.g. polkit denies
/// `org.freedesktop.login1.inhibit-delay-sleep`) we log and continue with the
/// racy behavior rather than degrade lock-on-sleep entirely.
async fn take_delay_inhibitor(connection: &zbus::Connection) -> Option<zbus::zvariant::OwnedFd> {
    match connection
        .call_method(
            Some("org.freedesktop.login1"),
            "/org/freedesktop/login1",
            Some("org.freedesktop.login1.Manager"),
            "Inhibit",
            &(
                "sleep:shutdown",
                "cosmic-bwarden-agent",
                "zeroizing vault secrets",
                "delay",
            ),
        )
        .await
    {
        Ok(reply) => match reply.body().deserialize::<zbus::zvariant::OwnedFd>() {
            Ok(fd) => Some(fd),
            Err(e) => {
                log::warn!("logind Inhibit: could not read returned fd: {e}");
                None
            }
        },
        Err(e) => {
            log::warn!(
                "logind Inhibit unavailable ({e}); vault lock will race \
                 suspend/shutdown instead of delaying it"
            );
            None
        }
    }
}

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
            .interface(interface)
            .expect("valid interface name")
            .member(member)
            .expect("valid member name")
            .build();
        proxy.add_match_rule(rule).await?;
    }

    let mut stream = zbus::MessageStream::from(&connection);

    // Held across the loop; dropped (releasing logind to proceed) only after
    // the vault is locked, re-acquired on resume.
    let mut inhibitor = take_delay_inhibitor(&connection).await;

    while let Some(msg) = stream.next().await {
        match msg {
            Ok(m) => {
                let header = m.header();
                let interface = header.interface();
                let member = header.member();

                let iface = interface.map(|i| i.as_str());
                let mbr = member.map(|m| m.as_str());
                match (iface, mbr) {
                    (Some("org.freedesktop.login1.Session"), Some("Lock")) => {
                        log::info!("vault locked: session lock");
                        let mut state_guard = state.lock().await;
                        state_guard.lock();
                    }
                    (
                        Some("org.freedesktop.login1.Manager"),
                        Some(sig @ ("PrepareForSleep" | "PrepareForShutdown")),
                    ) => {
                        // Both signals carry one bool: true right before the
                        // transition, false on resume (or a canceled shutdown).
                        // Fail toward locking if the body can't be read.
                        let starting = m.body().deserialize::<bool>().unwrap_or(true);
                        if starting {
                            log::info!("vault locked: {sig}");
                            {
                                let mut state_guard = state.lock().await;
                                state_guard.lock();
                            }
                            // Secrets are zeroized — release the delay
                            // inhibitor so logind can proceed with the
                            // suspend/hibernate/shutdown.
                            inhibitor = None;
                        } else {
                            // Resume: the vault is already locked, so don't
                            // lock() again (it would broadcast a spurious
                            // Locked event). Re-arm the inhibitor for the
                            // next sleep/shutdown.
                            if inhibitor.is_none() {
                                inhibitor = take_delay_inhibitor(&connection).await;
                            }
                        }
                    }
                    _ => {}
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
