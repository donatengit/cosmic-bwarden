pub mod applet;
pub mod auth;
pub mod settings;
pub mod style;
pub mod vault;

use crate::app::CosmicBWardenApp;
use crate::fl;
use crate::message::{Message, View, WindowState};
use cosmic::iced::{window, Length};
use cosmic::widget::{button, container, text};
use cosmic::Element;

/// Format a duration in seconds as a compact human string (e.g. "2h", "90m").
fn format_secs(secs: u32) -> String {
    if secs == 0 {
        fl!("duration-moment")
    } else if secs % 3600 == 0 {
        format!("{}h", secs / 3600)
    } else if secs >= 3600 {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    } else if secs >= 60 {
        format!("{}m", secs / 60)
    } else {
        format!("{secs}s")
    }
}

impl CosmicBWardenApp {
    /// One-line summary of the TPM dictionary-attack lockout state for display on
    /// the PIN screens and in Settings. `None` when the TPM is unavailable or the
    /// status hasn't been fetched. Note: the counter is TPM-global (shared by all
    /// DA-protected objects), so it's phrased as a machine-wide budget.
    pub fn tpm_da_line(&self) -> Option<String> {
        let da = self.tpm_da.as_ref()?;
        if !da.available {
            return None;
        }
        if da.in_lockout {
            return Some(match da.recovery_interval_secs {
                Some(s) => fl!("tpm-lockout-wait", time = format_secs(s)),
                None => fl!("tpm-lockout"),
            });
        }
        match (da.remaining, da.max_tries) {
            (Some(rem), Some(max)) => Some(fl!("tpm-attempts-remaining", rem = rem, max = max)),
            (Some(rem), None) => Some(fl!("tpm-attempts-remaining-simple", rem = rem)),
            _ => None,
        }
    }

    pub fn view_instance(&self, id: window::Id) -> Element<'_, Message> {
        let state = self.windows.get(&id);

        match state {
            Some(WindowState::Popup) => self.applet_popup_content(),
            None => {
                // Check if this is the panel providing a surface
                if std::env::var("COSMIC_PANEL_NAME").is_ok() {
                    self.applet_view()
                } else {
                    // standalone mode fallback
                    let content = self.view_content();
                    container(content)
                        .class(cosmic::theme::Container::WindowBackground)
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .padding(20)
                        .into()
                }
            }
        }
    }

    pub fn view_dialogs(&self) -> Option<Element<'_, Message>> {
        use cosmic::widget::{button, secure_input};
        if let Some(_id) = &self.show_delete_confirm {
            Some(
                cosmic::widget::dialog()
                    .title(fl!("delete-entry-title"))
                    .body(fl!("confirm-delete"))
                    .primary_action(button::destructive(fl!("delete")).on_press(Message::ConfirmDelete))
                    .secondary_action(button::standard(fl!("cancel")).on_press(Message::CancelDelete))
                    .width(Length::Fixed(400.0))
                    .into(),
            )
        } else if let Some(_) = &self.show_reprompt {
            let mut col = cosmic::widget::column::with_capacity(2).spacing(10);

            let password_input = secure_input(
                fl!("master-password"),
                &self.reprompt_password,
                Some(Message::ToggleRepromptPasswordReveal),
                !self.reprompt_password_revealed,
            )
            .on_input(Message::RepromptPasswordChanged)
            .on_submit(|_| Message::SubmitReprompt)
            .width(Length::Fill);
            col = col.push(password_input);

            Some(
                cosmic::widget::dialog()
                    .title(fl!("master-password-required"))
                    .body(fl!("enter-master-password"))
                    .control(col)
                    .primary_action(button::suggested(fl!("verify")).on_press(Message::SubmitReprompt))
                    .secondary_action(button::standard(fl!("cancel")).on_press(Message::CancelReprompt))
                    .width(Length::Fixed(400.0))
                    .into(),
            )
        } else {
            None
        }
    }

    pub fn view_content(&self) -> Element<'_, Message> {
        // If protocol versions don't match, show only the error and Quit.
        if self.protocol_mismatch {
            let col = cosmic::widget::column::with_capacity(2)
                .spacing(10)
                .push(text::body(fl!("protocol-version-mismatch")))
                .push(
                    button::text(fl!("quit"))
                        .on_press(Message::Exit)
                        .width(Length::Fill),
                );
            return container(col)
                .class(cosmic::theme::Container::WindowBackground)
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(20)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into();
        }

        match self.view {
            View::Loading => cosmic::widget::text::body(fl!("loading")).into(),
            View::Setup | View::Unlock => self.view_auth(),
            View::Vault | View::Settings => self.view_vault(),
        }
    }
}
