use crate::app::applet_search;
use crate::app::state::CosmardenApp;
use crate::app::update::{generator_actions, vault_actions};
use crate::message::Message;
use cosmarden_core::agent_client::AgentClient;
use cosmarden_core::protocol::{Action as AgentAction, EntryType, Response};
use cosmic::app::Task;
use cosmic::Action;

/// Compare against `cosmarden_core::PROTOCOL_VERSION`, not the build
/// version string — the two are intentionally decoupled (differently-timed
/// builds of the same protocol stay compatible), so comparing build versions
/// here would report a mismatch unconditionally.
fn protocol_mismatch(agent_protocol: &str) -> bool {
    cosmarden_core::PROTOCOL_VERSION != agent_protocol
}

pub fn check_protocol_version() -> Task<Message> {
    Task::perform(
        async move {
            let agent = AgentClient::new();
            match agent.send(AgentAction::Version).await {
                Ok(Response::Version {
                    protocol_version, ..
                }) => {
                    let mismatch = protocol_mismatch(&protocol_version);
                    if mismatch {
                        tracing::error!(
                            local_protocol = %cosmarden_core::PROTOCOL_VERSION,
                            agent_protocol = %protocol_version,
                            "Protocol version mismatch"
                        );
                    }
                    Ok(mismatch)
                }
                Err(e) => Err(format!("failed to check version: {}", e)),
                _ => Err("unexpected version response".to_string()),
            }
        },
        |res| Action::App(Message::ProtocolVersionCheck(res)),
    )
}

pub fn fetch_sidebar_entries(
    id: u32,
    query: Option<String>,
    entry_type: Option<EntryType>,
    only_pinned: bool,
) -> Task<Message> {
    Task::perform(
        async move {
            let agent = AgentClient::new();
            match agent
                .send(vault_actions::sidebar_entries(
                    query,
                    entry_type,
                    only_pinned,
                ))
                .await
            {
                Ok(Response::SidebarEntries { entries }) => Ok(entries),
                Ok(Response::Error { message }) => Err(message),
                _ => Err("unexpected response".to_string()),
            }
        },
        move |res| Action::App(Message::EntriesReceived(id, res)),
    )
}

/// Fetch the applet rows for the current query and favourites toggle, and
/// return the task for the caller's batch.
///
/// One definition for both lifecycle refreshes, because a caller that passed
/// its own `only_pinned` showed the pinned set under a non-empty search box.
pub fn applet_rows_task(app: &mut CosmardenApp) -> Task<Message> {
    app.applet_search_id += 1;
    let (query, only_pinned) =
        applet_rows_request(&app.applet_search_query, app.applet_search_only_favourites);
    fetch_applet_search(app.applet_search_id, query, only_pinned)
}

/// Pure request behind [`applet_rows_task`]: an empty query always means
/// "favourites only" (`effective_only_pinned`), and then `query` is sent as
/// `None` rather than an empty string. Kept separate so the pairing is
/// testable: passing a hardcoded `only_pinned` with `query = None` showed the
/// pinned set under a non-empty search box.
fn applet_rows_request(query: &str, favourites_toggle: bool) -> (Option<String>, bool) {
    let only_pinned = applet_search::effective_only_pinned(query, favourites_toggle);
    let query = if query.trim().is_empty() {
        None
    } else {
        Some(query.to_string())
    };
    (query, only_pinned)
}

pub fn fetch_applet_search(id: u32, query: Option<String>, only_pinned: bool) -> Task<Message> {
    Task::perform(
        async move {
            let agent = AgentClient::new();
            match agent
                .send(vault_actions::applet_search(query, only_pinned))
                .await
            {
                Ok(Response::SidebarEntries { entries }) => Ok(entries),
                Ok(Response::Error { message }) => Err(message),
                _ => Err("unexpected response".to_string()),
            }
        },
        move |res| Action::App(Message::AppletSearchResultsReceived(id, res)),
    )
}

pub fn fetch_generator_settings() -> Task<Message> {
    Task::perform(
        async move {
            let agent = AgentClient::new();
            match agent.send(generator_actions::fetch_settings()).await {
                Ok(Response::GeneratorSettings { settings }) => Ok(settings),
                Ok(Response::Error { message }) => Err(message),
                _ => Err("unexpected response".to_string()),
            }
        },
        |res| Action::App(Message::GeneratorSettingsReceived(res)),
    )
}

pub fn fetch_generator_history() -> Task<Message> {
    Task::perform(
        async move {
            let agent = AgentClient::new();
            match agent.send(generator_actions::fetch_history()).await {
                Ok(Response::PasswordHistory { entries }) => Ok(entries),
                Ok(Response::Error { message }) => Err(message),
                _ => Err("unexpected response".to_string()),
            }
        },
        |res| Action::App(Message::GeneratorHistoryReceived(res)),
    )
}

pub fn fetch_applet_secret(id: String, password: Option<String>) -> Task<Message> {
    Task::perform(
        async move {
            let agent = AgentClient::new();
            match agent
                .send(vault_actions::fetch_password(id.clone(), password))
                .await
            {
                Ok(Response::Password { password }) => Ok(password),
                Ok(Response::Error { message }) => Err((id, message)),
                _ => Err((id, "unexpected response".to_string())),
            }
        },
        |res| Action::App(Message::AppletSecretReceived(res)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_protocol_version_is_not_a_mismatch() {
        assert!(!protocol_mismatch(cosmarden_core::PROTOCOL_VERSION));
    }

    #[test]
    fn differing_protocol_version_is_a_mismatch() {
        assert!(protocol_mismatch("not-a-real-protocol-version"));
    }

    #[test]
    fn build_version_string_is_not_compared_against_protocol_version() {
        // Regression test: this used to compare the full build version string
        // (e.g. "2026.07-319457-aea2ef7") against PROTOCOL_VERSION ("1"),
        // which can never match — reporting a mismatch unconditionally
        // regardless of actual compatibility.
        let build_version = cosmarden_core::version();
        assert_ne!(build_version, cosmarden_core::PROTOCOL_VERSION);
        assert!(!protocol_mismatch(cosmarden_core::PROTOCOL_VERSION));
    }

    #[test]
    fn applet_rows_request_keeps_query_and_toggle_in_step() {
        // Empty query: favourites only, and no query string on the wire.
        assert_eq!(applet_rows_request("", false), (None, true));
        assert_eq!(applet_rows_request("   ", true), (None, true));
        // Non-empty query: the toggle decides, and the query is sent.
        assert_eq!(
            applet_rows_request("mail", false),
            (Some("mail".to_string()), false)
        );
        assert_eq!(
            applet_rows_request("mail", true),
            (Some("mail".to_string()), true)
        );
    }
}
