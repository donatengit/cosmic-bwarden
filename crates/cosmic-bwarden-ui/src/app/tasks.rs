use crate::app::update::{generator_actions, vault_actions};
use crate::message::Message;
use cosmic::app::Task;
use cosmic::Action;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action as AgentAction, EntryType, Response};

/// Compare against `cosmic_bwarden_core::PROTOCOL_VERSION`, not the build
/// version string — the two are intentionally decoupled (differently-timed
/// builds of the same protocol stay compatible), so comparing build versions
/// here would report a mismatch unconditionally.
fn protocol_mismatch(agent_protocol: &str) -> bool {
    cosmic_bwarden_core::PROTOCOL_VERSION != agent_protocol
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
                            local_protocol = %cosmic_bwarden_core::PROTOCOL_VERSION,
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
        assert!(!protocol_mismatch(cosmic_bwarden_core::PROTOCOL_VERSION));
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
        let build_version = cosmic_bwarden_core::version();
        assert_ne!(build_version, cosmic_bwarden_core::PROTOCOL_VERSION);
        assert!(!protocol_mismatch(cosmic_bwarden_core::PROTOCOL_VERSION));
    }
}
