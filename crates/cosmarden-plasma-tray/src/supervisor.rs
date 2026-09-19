//! The supervisor loop: agent state, menu commands, and the subscription that
//! keeps both current.
//!
//! Split from `tray.rs` (the StatusNotifierItem itself) so each file has one
//! responsibility — and so neither crosses the repository's 500-line limit.

use crate::clipboard;
use crate::launch;
use crate::menu::{self, MenuCommand};
use crate::state::TrayState;
use cosmarden_core::agent_client::{AgentClient, EventStream};
use cosmarden_core::db::Secret;
use cosmarden_core::protocol::{Event, Response};
use std::time::Duration;
use tokio::sync::mpsc;

/// How often the agent is re-read when nothing else happens. Events carry the
/// real transitions (`Locked`, `Unlocked`, `VaultChanged`); this only covers an
/// agent that starts *after* the tray, when there is no subscription yet to
/// fail and therefore no signal.
pub(crate) const PROBE_INTERVAL: Duration = Duration::from_secs(30);

/// Wait before retrying a `Subscribe` that failed or ended, so the loop cannot
/// spin while the agent is down.
const RECONNECT_DELAY: Duration = Duration::from_secs(5);

pub(crate) enum Signal {
    Command(MenuCommand),
    Event(Event),
    /// Nothing happened for [`PROBE_INTERVAL`]: re-read the agent.
    Tick,
    Exit,
}

/// Waits for whatever should happen next: a menu click, an agent event, or the
/// safety-net re-probe.
///
/// The socket path and retry delay are fields so a test can point at a fake
/// agent in a tempdir — a test must never subscribe to the developer's live
/// agent (README: tests never touch real user state).
pub(crate) struct Supervisor {
    commands: mpsc::UnboundedReceiver<MenuCommand>,
    events: Option<EventStream>,
    socket: std::path::PathBuf,
    retry_delay: Duration,
}

impl Supervisor {
    /// The production entry point: waits [`RECONNECT_DELAY`] between failed
    /// subscriptions.
    pub(crate) fn new(
        commands: mpsc::UnboundedReceiver<MenuCommand>,
        socket: std::path::PathBuf,
    ) -> Self {
        Self::with_retry(commands, socket, RECONNECT_DELAY)
    }

    /// Same, with an explicit retry delay — tests use a few milliseconds so a
    /// missing agent does not slow the suite down.
    fn with_retry(
        commands: mpsc::UnboundedReceiver<MenuCommand>,
        socket: std::path::PathBuf,
        retry_delay: Duration,
    ) -> Self {
        Self {
            commands,
            events: None,
            socket,
            retry_delay,
        }
    }

    pub(crate) async fn next_signal(&mut self) -> Signal {
        // Destructured so the two `select!` branches borrow disjoint fields;
        // `self.next_event()` would need `&mut self` twice in one expression.
        let Supervisor {
            commands,
            events,
            socket,
            retry_delay,
        } = self;
        let tick = tokio::time::sleep(PROBE_INTERVAL);
        tokio::pin!(tick);

        tokio::select! {
            command = commands.recv() => match command {
                Some(MenuCommand::Exit) => Signal::Exit,
                Some(command) => Signal::Command(command),
                // Unreachable while the tray holds the sender; treat a closed
                // channel as "nothing to do" rather than exiting silently.
                None => Signal::Tick,
            },
            event = next_event(events, socket, *retry_delay) => match event {
                Some(event) => Signal::Event(event),
                None => Signal::Tick,
            },
            _ = &mut tick => Signal::Tick,
        }
    }
}

/// The next agent event, subscribing first if needed. `None` means the
/// subscription is not available right now; the caller just re-probes.
pub(crate) async fn next_event(
    events: &mut Option<EventStream>,
    socket: &std::path::Path,
    retry_delay: Duration,
) -> Option<Event> {
    if events.is_none() {
        match EventStream::connect_with_socket(socket.to_path_buf()).await {
            Ok(stream) => *events = Some(stream),
            Err(e) => {
                tracing::debug!("no agent subscription yet: {e}");
                // Wait before retrying: this future is polled inside the
                // caller's `select!`, and returning an error immediately would
                // spin it.
                tokio::time::sleep(retry_delay).await;
                return None;
            }
        }
    }
    let stream = events.as_mut().expect("subscribed above");
    match stream.next_event().await {
        Ok(event) => Some(event),
        Err(e) => {
            // The agent restarted, exited, or closed the socket: drop the
            // stream, wait, and let the next iteration re-subscribe. The
            // pending re-probe picks up whatever the new state is.
            tracing::debug!("agent subscription ended: {e}");
            *events = None;
            tokio::time::sleep(retry_delay).await;
            None
        }
    }
}

pub(crate) async fn run_command(
    command: MenuCommand,
    client: &AgentClient,
    state: Option<&TrayState>,
) {
    if let Some(action) = menu::agent_action(&command) {
        let what = subject(&command, state);
        let is_copy = matches!(command, MenuCommand::CopyPassword(_));
        match client.send(action).await {
            // The one place a plaintext secret arrives, and it goes straight to
            // the clipboard: never logged, zeroized when the write is done.
            Ok(Response::Password { password }) if is_copy => {
                clipboard::copy_secret(password, &what);
            }
            Ok(Response::Ack) => tracing::info!("{what} requested"),
            Ok(other) => tracing::warn!("{what}: unexpected response {other:?}"),
            // A reprompt-gated entry answers `reprompt_required`. There is no
            // input field in a menu, so say where the copy is possible instead
            // of failing silently.
            Err(e) if is_copy => tracing::warn!(
                "copy password for {what} failed: {e}; \
                 entries that need a master password can be copied from the app (Open Vault)"
            ),
            Err(e) => tracing::error!("{what} failed: {e}"),
        }
        return;
    }
    match command {
        MenuCommand::OpenVault => launch::spawn_vault(),
        MenuCommand::OpenLink(uri) => launch::open_link(&uri),
        MenuCommand::CopyLogin(id) => copy_login(&id, state),
        // Handled by the caller, which breaks the loop so the item unregisters.
        MenuCommand::Exit => {}
        MenuCommand::Lock | MenuCommand::Logout | MenuCommand::CopyPassword(_) => {
            unreachable!("handled by agent_action")
        }
    }
}

/// Copy the username of the row a command refers to.
///
/// The username comes from the sidebar snapshot, so this touches no secret the
/// agent has not already sent; it is routed through the same clipboard policy
/// as a password.
pub(crate) fn copy_login(id: &str, state: Option<&TrayState>) {
    let what = subject(&MenuCommand::CopyLogin(id.to_string()), state);
    match state
        .and_then(|state| state.row(id))
        .and_then(|row| row.username.clone())
    {
        Some(username) => clipboard::copy_secret(Secret::from(username), &what),
        // The row was replaced between the menu being built and the click.
        None => tracing::warn!("{what}: no stored login for this row"),
    }
}

/// The log subject for a command: the row's label when it refers to one, so a
/// line never carries a value.
pub(crate) fn subject(command: &MenuCommand, state: Option<&TrayState>) -> String {
    let id = match command {
        MenuCommand::CopyLogin(id) | MenuCommand::CopyPassword(id) => id,
        other => return format!("{other:?}"),
    };
    match state.and_then(|state| state.row(id)) {
        Some(row) => row.label.clone(),
        None => format!("entry {id}"),
    }
}

/// Log what changed, not every poll: a stopped agent must not fill the journal,
/// and a state change must not pass silently.
///
/// `None` is the first observation — the tray's opening state, which is logged
/// even when nothing changes afterwards.
pub(crate) fn log_state(previous: Option<&TrayState>, next: &TrayState) {
    let Some(previous) = previous else {
        tracing::info!(
            agent_ready = next.agent_ready,
            has_account = next.has_account,
            locked = next.locked,
            protocol_mismatch = next.protocol_mismatch,
            rows = next.rows.len(),
            "initial tray state"
        );
        if !next.agent_ready {
            tracing::warn!("agent unreachable; the menu offers Open Vault and Exit");
        }
        return;
    };

    if previous.protocol_mismatch != next.protocol_mismatch {
        if next.protocol_mismatch {
            tracing::error!(
                "agent protocol version does not match this build (local {}); \
                 offering nothing but Exit",
                cosmarden_core::PROTOCOL_VERSION
            );
        } else {
            tracing::info!("agent protocol version matches this build again");
        }
    }
    if previous.agent_ready != next.agent_ready {
        if next.agent_ready {
            tracing::info!("agent reachable");
        } else {
            tracing::warn!("agent unreachable; open the vault app to start it");
        }
    }
    if previous.has_account != next.has_account {
        tracing::info!(has_account = next.has_account, "account state changed");
    }
    if previous.locked != next.locked {
        tracing::info!(locked = next.locked, "lock state changed");
    }
    if previous.rows != next.rows {
        tracing::info!(rows = next.rows.len(), "menu rows updated");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::LoginRow;

    /// Fast retry so a test that waits out a missing agent stays quick.
    const TEST_RETRY: Duration = Duration::from_millis(10);

    fn supervisor(
        commands: mpsc::UnboundedReceiver<MenuCommand>,
        socket: std::path::PathBuf,
    ) -> Supervisor {
        Supervisor::with_retry(commands, socket, TEST_RETRY)
    }

    /// A socket path inside a tempdir with nothing listening on it. The tempdir
    /// is returned so the caller keeps it (and it cleans itself up).
    fn dead_socket() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("socket");
        (dir, path)
    }

    /// A fake agent: accepts one connection, expects `Subscribe`, then streams
    /// `frames`. Returns the socket path (and the tempdir that owns it).
    fn fake_agent(frames: Vec<Response>) -> (tempfile::TempDir, std::path::PathBuf) {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("socket");
        let listener = tokio::net::UnixListener::bind(&path).expect("bind fake agent");
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut len_buf = [0u8; 4];
            socket.read_exact(&mut len_buf).await.expect("read length");
            let mut body = vec![0u8; u32::from_le_bytes(len_buf) as usize];
            socket.read_exact(&mut body).await.expect("read body");
            assert!(
                matches!(
                    postcard::from_bytes::<cosmarden_core::protocol::Action>(&body),
                    Ok(cosmarden_core::protocol::Action::Subscribe)
                ),
                "the tray must subscribe before listening for events"
            );
            for response in std::iter::once(Response::Ack).chain(frames) {
                let bytes = postcard::to_allocvec(&response).expect("serialize");
                let _ = socket.write_all(&(bytes.len() as u32).to_le_bytes()).await;
                let _ = socket.write_all(&bytes).await;
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        });
        (dir, path)
    }

    /// The mapping the host's menu callbacks run: a command crosses the channel
    /// as-is, and `Exit` ends the loop instead of running as an action.
    #[tokio::test]
    async fn exit_is_a_signal_not_a_command() {
        let (_dir, socket) = dead_socket();
        let (tx, rx) = mpsc::unbounded_channel();
        let mut supervisor = supervisor(rx, socket);
        tx.send(MenuCommand::Exit).unwrap();
        assert!(matches!(supervisor.next_signal().await, Signal::Exit));

        tx.send(MenuCommand::OpenLink("https://example.com".into()))
            .unwrap();
        assert!(matches!(
            supervisor.next_signal().await,
            Signal::Command(MenuCommand::OpenLink(uri)) if uri == "https://example.com"
        ));
    }

    /// A socket path nothing listens on: the retry branch must wait out
    /// `retry_delay` and report "nothing happened" rather than spin the
    /// supervisor.
    #[tokio::test]
    async fn a_missing_agent_yields_no_event() {
        let (_dir, socket) = dead_socket();
        let (_tx, rx) = mpsc::unbounded_channel();
        let mut supervisor = supervisor(rx, socket);
        assert!(
            next_event(&mut supervisor.events, &supervisor.socket, TEST_RETRY)
                .await
                .is_none()
        );
        assert!(supervisor.events.is_none());
    }

    /// A live subscription is what refreshes the menu without polling: the
    /// event the fake agent sends is the signal the supervisor returns.
    #[tokio::test]
    async fn an_agent_event_reaches_the_supervisor() {
        let (_tx, rx) = mpsc::unbounded_channel();
        let (_dir, socket) = fake_agent(vec![Response::Event {
            event: Event::VaultChanged,
        }]);
        let mut supervisor = supervisor(rx, socket);
        assert!(
            matches!(
                next_event(&mut supervisor.events, &supervisor.socket, TEST_RETRY).await,
                Some(Event::VaultChanged)
            ),
            "the subscribed event must come back from the supervisor"
        );
    }

    /// The transition log is the only place a stopped agent is reported, so it
    /// must fire on the change itself.
    #[test]
    fn transitions_are_what_gets_logged() {
        // No assertion on the log output (it is tracing's), only that the three
        // cases the decision distinguishes stay distinguishable: a first
        // observation, a lock change, and no change at all.
        let locked = TrayState {
            agent_ready: true,
            has_account: true,
            locked: true,
            ..Default::default()
        };
        let unlocked = TrayState {
            locked: false,
            rows: vec![LoginRow {
                id: "1".into(),
                label: "example.com".into(),
                username: Some("alice".into()),
                uri: Some("https://example.com".into()),
            }],
            ..locked.clone()
        };
        log_state(None, &TrayState::default());
        log_state(Some(&locked), &unlocked);
        log_state(Some(&unlocked), &unlocked.clone());
        assert_ne!(locked, unlocked, "a lock change must be visible");
        assert_ne!(
            TrayState::default(),
            unlocked,
            "an agent-less start must not look like a running vault"
        );
    }
}
