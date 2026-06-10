use cosmic::app::Task;
use cosmic::Action;
use cosmic_bwarden_core::protocol::{Action as AgentAction, Response, EntryType};
use cosmic_bwarden_core::agent_client::AgentClient;
use crate::message::Message;

pub fn fetch_sidebar_entries(id: u32, query: Option<String>, entry_type: Option<String>, only_pinned: bool) -> Task<Message> {
    Task::perform(async move {
        let agent = AgentClient::new();
        let et = match entry_type.as_deref() {
            Some("login") => Some(EntryType::Login),
            Some("note") => Some(EntryType::SecureNote),
            Some("ssh") => Some(EntryType::SshKey),
            _ => None,
        };
        match agent.send(AgentAction::GetSidebarEntries { query, entry_type: et, only_pinned }).await {
            Ok(Response::SidebarEntries { entries }) => Ok(entries),
            Ok(Response::Error { message }) => Err(message),
            _ => Err("unexpected response".to_string()),
        }
    }, move |res| Action::App(Message::EntriesReceived(id, res)))
}

pub fn fetch_top_entries(limit: usize, days: Option<u32>) -> Task<Message> {
    Task::perform(async move {
        let agent = AgentClient::new();
        match agent.send(AgentAction::GetTopFrequent { limit, days }).await {
            Ok(Response::SidebarEntries { entries }) => Ok(entries),
            Ok(Response::Error { message }) => Err(message),
            _ => Err("unexpected response".to_string()),
        }
    }, |res| Action::App(Message::TopEntriesReceived(res)))
}
