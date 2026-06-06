use cosmic::Element;
use cosmic::iced::Length;
use cosmic::widget::{button, text, secure_input};
use crate::app::CosmicBWardenApp;
use crate::message::{Message, View};

impl CosmicBWardenApp {
    pub fn view_auth(&self) -> Element<'_, Message> {
        if self.view == View::Setup {
            let mut login_col = cosmic::widget::column::with_capacity(10).spacing(10);

            login_col = login_col.push(cosmic::widget::text_input::text_input("Email", &self.login_email)
                .on_input(Message::EmailChanged)
                .width(Length::Fill));

            let password_input = secure_input("Master Password", &self.login_password, Some(Message::ToggleMasterPasswordReveal), !self.master_password_revealed)
                .on_input(Message::PasswordChanged)
                .on_submit(|_| Message::LoginSubmitted)
                .width(Length::Fill);
            login_col = login_col.push(password_input);

            if self.show_verification_input {
                login_col = login_col.push(text::body("New device verification required. Please check your email."));
                login_col = login_col.push(cosmic::widget::text_input::text_input("Verification Code", &self.login_verification_code)
                    .on_input(Message::VerificationCodeChanged)
                    .on_submit(|_| Message::LoginSubmitted)
                    .width(Length::Fill));
            }

            let advanced_btn = button::standard(if self.show_advanced { "Hide Advanced" } else { "Show Advanced" })
                .on_press(Message::ToggleAdvanced);
            login_col = login_col.push(advanced_btn);

            if self.show_advanced {
                login_col = login_col.push(cosmic::widget::text_input::text_input("Server URL (optional)", &self.login_server)
                    .on_input(Message::ServerChanged)
                    .width(Length::Fill));
            }

            login_col = login_col.push(cosmic::widget::toggler(self.login_remember)
                .label(Some("Remember email".to_string()))
                .on_toggle(Message::RememberChanged));

            if let Some(error) = &self.error {
                login_col = login_col.push(text::body(format!("Error: {}", error)));
            }

            let login_disabled = self.login_email.trim().is_empty() || self.login_password.is_empty() || (self.show_verification_input && self.login_verification_code.is_empty());

            let mut login_btn = button::suggested("Login")
                .width(Length::Fill);
            if !login_disabled {
                login_btn = login_btn.on_press(Message::LoginSubmitted);
            }

            cosmic::widget::dialog()
                .title("Welcome to CosmicBWarden")
                .body("Sign in to your Bitwarden vault.")
                .control(login_col)
                .primary_action(login_btn)
                .width(Length::Fixed(400.0))
                .into()
        } else {
            let mut unlock_col = cosmic::widget::column::with_capacity(5).spacing(10);

            unlock_col = unlock_col.push(text::body(format!("Logged in as {}", self.login_email)));

            let password_input = secure_input("Master Password", &self.unlock_password, Some(Message::ToggleMasterPasswordReveal), !self.master_password_revealed)
                .on_input(Message::UnlockPasswordChanged)
                .on_submit(|_| Message::UnlockSubmitted)
                .width(Length::Fill);
            unlock_col = unlock_col.push(password_input);

            if let Some(error) = &self.error {
                unlock_col = unlock_col.push(text::body(format!("Error: {}", error)));
            }

            let unlock_disabled = self.unlock_password.is_empty();
            let mut unlock_btn = button::suggested("Unlock")
                .width(Length::Fill);
            if !unlock_disabled {
                unlock_btn = unlock_btn.on_press(Message::UnlockSubmitted);
            }

            let logout_btn = button::standard("Log Out")
                .on_press(Message::LogoutClicked);

            cosmic::widget::dialog()
                .title("Vault Locked")
                .body("Enter your master password to unlock.")
                .control(unlock_col)
                .primary_action(unlock_btn)
                .secondary_action(logout_btn)
                .width(Length::Fixed(400.0))
                .into()
        }
    }
}
