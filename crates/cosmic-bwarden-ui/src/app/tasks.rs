use cosmic::app::Task;
use cosmic::Action;
use cosmic_bwarden_core::protocol::{Action as AgentAction, Response, EntryType};
use cosmic_bwarden_core::agent_client::AgentClient;
use crate::message::Message;

pub fn fetch_sidebar_entries(id: u32, query: Option<String>, entry_type: Option<EntryType>, only_pinned: bool) -> Task<Message> {
    Task::perform(async move {
        let agent = AgentClient::new();
        match agent.send(AgentAction::GetSidebarEntries { query, entry_type, only_pinned }).await {
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

pub fn fetch_applet_search(id: u32, query: Option<String>, only_pinned: bool) -> Task<Message> {
    Task::perform(async move {
        let agent = AgentClient::new();
        match agent.send(AgentAction::GetSidebarEntries { query, entry_type: None, only_pinned }).await {
            Ok(Response::SidebarEntries { entries }) => Ok(entries),
            Ok(Response::Error { message }) => Err(message),
            _ => Err("unexpected response".to_string()),
        }
    }, move |res| Action::App(Message::AppletSearchResultsReceived(id, res)))
}

pub fn fetch_applet_secret(id: String, password: Option<String>) -> Task<Message> {
    Task::perform(async move {
        let agent = AgentClient::new();
        match agent.send(AgentAction::GetPassword { id: id.clone(), password }).await {
            Ok(Response::Password { password }) => Ok(password),
            Ok(Response::Error { message }) => Err((id, message)),
            _ => Err((id, "unexpected response".to_string())),
        }
    }, |res| Action::App(Message::AppletSecretReceived(res)))
}
