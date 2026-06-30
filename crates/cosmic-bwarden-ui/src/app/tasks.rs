use crate::message::Message;
use cosmic::app::Task;
use cosmic::Action;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action as AgentAction, EntryType, Response};

pub fn check_protocol_version() -> Task<Message> {
    Task::perform(
        async move {
            let local = cosmic_bwarden_core::version().to_string();
            let agent = AgentClient::new();
            match agent.send(AgentAction::Version).await {
                Ok(Response::Version {
                    protocol_version, ..
                }) => {
                    let mismatch = local != protocol_version;
                    if mismatch {
                        tracing::error!(
                            local_version = %local,
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
                .send(AgentAction::GetSidebarEntries {
                    query,
                    entry_type,
                    only_pinned,
                })
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
                .send(AgentAction::GetSidebarEntries {
                    query,
                    entry_type: None,
                    only_pinned,
                })
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

pub fn fetch_applet_secret(id: String, password: Option<String>) -> Task<Message> {
    Task::perform(
        async move {
            let agent = AgentClient::new();
            match agent
                .send(AgentAction::GetPassword {
                    id: id.clone(),
                    password,
                })
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
