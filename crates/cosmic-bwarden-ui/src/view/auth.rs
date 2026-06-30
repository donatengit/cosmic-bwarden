use cosmic::Element;
use cosmic::iced::Length;
use cosmic::widget::{button, list_column, settings as cosmic_settings, text, secure_input};
use crate::app::CosmicBWardenApp;
use crate::message::{Message, View};
use crate::fl;
use crate::view::style::muted_text;


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

            // Grid-aligned togglers section: "Remember email" and "Enable PIN unlock".
            // Using list_column + settings::item aligns all togglers on the right edge.
            let mut togglers = list_column();
            togglers = togglers.add(cosmic_settings::item(
                "Remember email",
                cosmic::widget::toggler(self.login_remember)
                    .on_toggle(Message::RememberChanged),
            ));

            if self.tpm_available && !self.tpm_configured {
                togglers = togglers.add(cosmic_settings::item(
                    "Enable PIN unlock after login",
                    cosmic::widget::toggler(self.login_pin_enabled)
                        .on_toggle(Message::LoginPinEnabledToggled),
                ));
            }
            login_col = login_col.push(togglers);

            // PIN input + description (only shown when toggle is on)
            if self.tpm_available && !self.tpm_configured && self.login_pin_enabled {
                login_col = login_col.push(
                    secure_input("PIN (min 4 characters)", &self.login_pin, Some(Message::LoginPinRevealToggled), !self.login_pin_revealed)
                        .on_input(Message::LoginPinChanged)
                        .on_submit(|_| Message::LoginSubmitted)
                        .width(Length::Fill),
                );
                login_col = login_col.push(
                    text::caption("Secured by your device's hardware chip (TPM 2.0) — your PIN only works on this computer.")
                        .class(muted_text()),
                );
            }

            if let Some(error) = &self.error {
                login_col = login_col.push(text::body(format!("Error: {}", error)));
            }

            let login_disabled = self.login_email.trim().is_empty()
                || self.login_password.is_empty()
                || (self.show_verification_input && self.login_verification_code.is_empty())
                || self.auth_loading;

            let label = if self.auth_loading { "…" } else { "Login" };
            let mut login_btn = button::suggested(label)
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

        } else if self.show_pin_unlock && self.tpm_configured {
            // PIN unlock view for the main window (mirrors applet unlock PIN view).
            let mut pin_col = cosmic::widget::column::with_capacity(6).spacing(10);

            if !self.login_email.is_empty() {
                pin_col = pin_col.push(text::body(&self.login_email).class(muted_text()));
            }

            let pin_input = secure_input(fl!("locked-need-pin"), &self.main_window_pin, None, true)
                .on_input(Message::MainWindowPinChanged)
                .on_submit(|_| Message::MainWindowPinSubmitted)
                .width(Length::Fill);
            pin_col = pin_col.push(pin_input);

            if let Some(error) = &self.error {
                pin_col = pin_col.push(text::body(format!("Error: {}", error)));
            }

            let pin_label = if self.auth_loading { "…" } else { "Unlock" };
            let mut pin_btn = button::suggested(pin_label).width(Length::Fill);
            if !self.auth_loading && !self.main_window_pin.is_empty() {
                pin_btn = pin_btn.on_press(Message::MainWindowPinSubmitted);
            }

            let fallback_btn = button::standard("Use master password instead")
                .on_press(Message::AppletUseMasterPasswordInstead);

            cosmic::widget::dialog()
                .title("Vault Locked")
                .body("Enter your PIN to unlock.")
                .control(pin_col)
                .primary_action(pin_btn)
                .secondary_action(fallback_btn)
                .width(Length::Fixed(400.0))
                .into()

        } else {
            // Master password unlock view.
            let mut unlock_col = cosmic::widget::column::with_capacity(6).spacing(10);

            if !self.login_email.is_empty() {
                unlock_col = unlock_col.push(text::body(&self.login_email).class(muted_text()));
            }

            let password_input = secure_input("Master Password", &self.unlock_password, Some(Message::ToggleMasterPasswordReveal), !self.master_password_revealed)
                .on_input(Message::UnlockPasswordChanged)
                .on_submit(|_| Message::UnlockSubmitted)
                .width(Length::Fill);
            unlock_col = unlock_col.push(password_input);

            if let Some(error) = &self.error {
                unlock_col = unlock_col.push(text::body(format!("Error: {}", error)));
            }

            let unlock_disabled = self.unlock_password.is_empty() || self.auth_loading;
            let unlock_label = if self.auth_loading { "…" } else { "Unlock" };
            let mut unlock_btn = button::suggested(unlock_label).width(Length::Fill);
            if !unlock_disabled {
                unlock_btn = unlock_btn.on_press(Message::UnlockSubmitted);
            }

            let logout_btn = button::standard(fl!("logout")).on_press(Message::LogoutClicked);

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
