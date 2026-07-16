use crate::app::state::CosmicBWardenApp;
use crate::app::tasks::{fetch_generator_history, fetch_generator_settings};
use crate::message::{Message, View};
use cosmic::app::Task;
use cosmic::Action;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action as AgentAction, GeneratorSettings, Response};

impl CosmicBWardenApp {
    pub fn update_pwgen(&mut self, message: Message) -> Option<Task<Message>> {
        match message {
            Message::GeneratorViewClicked => {
                self.view = View::PasswordGenerator;
                self.selected_entry_id = None;
                self.selected_entry = None;
                self.editing_entry = None;
                Some(Task::batch(vec![
                    fetch_generator_settings(),
                    fetch_generator_history(),
                ]))
            }
            Message::GeneratorUppercaseToggled(v) => {
                self.generator_settings.uppercase = v;
                Some(Task::none())
            }
            Message::GeneratorLowercaseToggled(v) => {
                self.generator_settings.lowercase = v;
                Some(Task::none())
            }
            Message::GeneratorNumbersToggled(v) => {
                self.generator_settings.numbers = v;
                Some(Task::none())
            }
            Message::GeneratorSpecialToggled(v) => {
                self.generator_settings.special = v;
                Some(Task::none())
            }
            Message::GeneratorLengthChanged(v) => {
                self.generator_settings.length = v as u8;
                Some(Task::none())
            }
            Message::GeneratorResetClicked => {
                self.generator_settings = GeneratorSettings::default();
                Some(Task::none())
            }
            Message::GeneratorGenerateClicked => {
                let settings = self.generator_settings;
                Some(Task::perform(
                    async move {
                        let agent = AgentClient::new();
                        match agent
                            .send(AgentAction::GeneratePassword {
                                settings: Some(settings),
                            })
                            .await
                        {
                            Ok(Response::GeneratedPassword { password }) => Ok(password),
                            Ok(Response::Error { message }) => Err(message),
                            _ => Err("unexpected response".to_string()),
                        }
                    },
                    |res| Action::App(Message::GeneratorGenerated(res)),
                ))
            }
            Message::GeneratorGenerated(res) => {
                match res {
                    Ok(pw) => {
                        self.generator_result = Some(pw);
                        self.generator_result_revealed = false;
                        self.generator_error = None;
                        // Refresh the list so the just-generated entry shows up.
                        return Some(fetch_generator_history());
                    }
                    Err(e) => self.generator_error = Some(e),
                }
                Some(Task::none())
            }
            Message::GeneratorRevealToggled => {
                self.generator_result_revealed = !self.generator_result_revealed;
                Some(Task::none())
            }
            Message::GeneratorSettingsReceived(res) => {
                if let Ok(s) = res {
                    self.generator_settings = s;
                }
                Some(Task::none())
            }
            Message::GeneratorHistoryReceived(res) => {
                match res {
                    Ok(entries) => {
                        self.generator_history = entries;
                        self.generator_history_revealed.clear();
                    }
                    Err(e) => self.generator_error = Some(e),
                }
                Some(Task::none())
            }
            Message::GeneratorHistoryRevealToggled(idx) => {
                if !self.generator_history_revealed.remove(&idx) {
                    self.generator_history_revealed.insert(idx);
                }
                Some(Task::none())
            }
            Message::GeneratorHistoryDeleteRequested(idx) => {
                self.generator_history_delete_pending = Some(idx);
                Some(Task::none())
            }
            Message::GeneratorHistoryDeleteCancelled => {
                self.generator_history_delete_pending = None;
                Some(Task::none())
            }
            Message::GeneratorHistoryDeleteConfirmed => {
                let Some(idx) = self.generator_history_delete_pending.take() else {
                    return Some(Task::none());
                };
                let Some(entry) = self.generator_history.get(idx) else {
                    return Some(Task::none());
                };
                let created_at = entry.created_at;
                Some(Task::perform(
                    async move {
                        let agent = AgentClient::new();
                        match agent
                            .send(AgentAction::DeleteGeneratedPassword { created_at })
                            .await
                        {
                            Ok(Response::Ack) => Ok(()),
                            Ok(Response::Error { message }) => Err(message),
                            _ => Err("unexpected response".to_string()),
                        }
                    },
                    |res| Action::App(Message::GeneratorHistoryDeleted(res)),
                ))
            }
            Message::GeneratorHistoryDeleted(res) => {
                match res {
                    Ok(()) => return Some(fetch_generator_history()),
                    Err(e) => self.generator_error = Some(e),
                }
                Some(Task::none())
            }
            _ => None,
        }
    }
}
