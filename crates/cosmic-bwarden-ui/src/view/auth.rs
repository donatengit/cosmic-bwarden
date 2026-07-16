use crate::app::CosmicBWardenApp;
use crate::fl;
use crate::message::{Message, View};
use crate::view::style::{brand_icon, muted_text};
use crate::MIN_PIN_LEN;
use cosmic::iced::Length;
use cosmic::widget::{button, list_column, secure_input, settings as cosmic_settings, text};
use cosmic::Element;

impl CosmicBWardenApp {
    pub fn view_auth(&self) -> Element<'_, Message> {
        if self.view == View::Setup {
            let mut login_col = cosmic::widget::column::with_capacity(10).spacing(10);

            login_col = login_col.push(
                cosmic::widget::text_input::text_input(fl!("email"), &self.login_email)
                    .on_input(Message::EmailChanged)
                    .width(Length::Fill),
            );

            let password_input = secure_input(
                fl!("master-password"),
                &self.login_password,
                Some(Message::ToggleMasterPasswordReveal),
                !self.master_password_revealed,
            )
            .on_input(Message::PasswordChanged)
            .on_submit(|_| Message::LoginSubmitted)
            .width(Length::Fill);
            login_col = login_col.push(password_input);

            if self.show_verification_input {
                login_col = login_col.push(text::body(fl!("new-device-verification")));
                login_col = login_col.push(
                    cosmic::widget::text_input::text_input(
                        fl!("verification-code"),
                        &self.login_verification_code,
                    )
                    .on_input(Message::VerificationCodeChanged)
                    .on_submit(|_| Message::LoginSubmitted)
                    .width(Length::Fill),
                );
            }

            let advanced_btn = button::standard(if self.show_advanced {
                fl!("hide-advanced")
            } else {
                fl!("show-advanced")
            })
            .on_press(Message::ToggleAdvanced);
            login_col = login_col.push(advanced_btn);

            if self.show_advanced {
                login_col = login_col.push(
                    cosmic::widget::text_input::text_input(
                        fl!("server-url-optional"),
                        &self.login_server,
                    )
                    .on_input(Message::ServerChanged)
                    .width(Length::Fill),
                );
            }

            // Grid-aligned togglers section: "Remember email" and "Enable PIN unlock".
            // Using list_column + settings::item aligns all togglers on the right edge.
            let mut togglers = list_column();
            togglers = togglers.add(cosmic_settings::item(
                fl!("remember-email"),
                cosmic::widget::toggler(self.login_remember).on_toggle(Message::RememberChanged),
            ));

            if self.tpm_available && !self.tpm_configured {
                togglers = togglers.add(cosmic_settings::item(
                    fl!("enable-pin-after-login"),
                    cosmic::widget::toggler(self.login_pin_enabled)
                        .on_toggle(Message::LoginPinEnabledToggled),
                ));
            }
            login_col = login_col.push(togglers);

            // PIN input + description (only shown when toggle is on)
            if self.tpm_available && !self.tpm_configured && self.login_pin_enabled {
                login_col = login_col.push(
                    secure_input(
                        fl!("pin-min-chars", count = MIN_PIN_LEN),
                        &self.login_pin,
                        Some(Message::LoginPinRevealToggled),
                        !self.login_pin_revealed,
                    )
                    .on_input(Message::LoginPinChanged)
                    .on_submit(|_| Message::LoginSubmitted)
                    .width(Length::Fill),
                );
                login_col =
                    login_col.push(text::caption(fl!("pin-tpm-note-login")).class(muted_text()));
            }

            if let Some(error) = &self.error {
                login_col = login_col.push(text::body(fl!("error-fmt", error = error.clone())));
            }

            let login_disabled = self.login_email.trim().is_empty()
                || self.login_password.is_empty()
                || (self.show_verification_input && self.login_verification_code.is_empty())
                || self.auth_loading;

            let label = if self.auth_loading {
                "…".to_string()
            } else {
                fl!("login")
            };
            let mut login_btn = button::suggested(label).width(Length::Fill);
            if !login_disabled {
                login_btn = login_btn.on_press(Message::LoginSubmitted);
            }

            cosmic::widget::dialog()
                .icon(brand_icon(48))
                .title(fl!("welcome-title"))
                .body(fl!("welcome-body"))
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

            // Show the attempts-remaining counter only after a wrong PIN.
            if self.pin_incorrect {
                pin_col = pin_col.push(text::caption(self.pin_feedback_line()).class(muted_text()));
            }

            if let Some(error) = &self.error {
                pin_col = pin_col.push(text::body(fl!("error-fmt", error = error.clone())));
            }

            let pin_label = if self.auth_loading {
                "…".to_string()
            } else {
                fl!("unlock")
            };
            let mut pin_btn = button::suggested(pin_label).width(Length::Fill);
            if !self.auth_loading && !self.main_window_pin.is_empty() {
                pin_btn = pin_btn.on_press(Message::MainWindowPinSubmitted);
            }

            let fallback_btn = button::standard(fl!("use-master-password-instead"))
                .on_press(Message::AppletUseMasterPasswordInstead);

            cosmic::widget::dialog()
                .icon(brand_icon(48))
                .title(fl!("vault-locked"))
                .body(fl!("enter-pin-to-unlock"))
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

            let password_input = secure_input(
                fl!("master-password"),
                &self.unlock_password,
                Some(Message::ToggleMasterPasswordReveal),
                !self.master_password_revealed,
            )
            .on_input(Message::UnlockPasswordChanged)
            .on_submit(|_| Message::UnlockSubmitted)
            .width(Length::Fill);
            unlock_col = unlock_col.push(password_input);

            // PIN (re-)enable field. Offered on every master-password unlock when a
            // TPM is present, so the user can re-establish PIN unlock after a TPM
            // mismatch (or turn it off) without visiting Settings.
            if self.tpm_available {
                let pin_note = if self.tpm_configured {
                    fl!("pin-reenable-note")
                } else {
                    fl!("pin-optional-note")
                };
                unlock_col = unlock_col.push(
                    secure_input(
                        fl!("pin-min-chars", count = MIN_PIN_LEN),
                        &self.unlock_pin,
                        Some(Message::UnlockPinRevealToggled),
                        !self.unlock_pin_revealed,
                    )
                    .on_input(Message::UnlockPinChanged)
                    .on_submit(|_| Message::UnlockSubmitted)
                    .width(Length::Fill),
                );
                unlock_col = unlock_col.push(text::caption(pin_note).class(muted_text()));
            }

            if let Some(error) = &self.error {
                unlock_col = unlock_col.push(text::body(fl!("error-fmt", error = error.clone())));
            }

            let unlock_disabled = self.unlock_password.is_empty() || self.auth_loading;
            let unlock_label = if self.auth_loading {
                "…".to_string()
            } else {
                fl!("unlock")
            };
            let mut unlock_btn = button::suggested(unlock_label).width(Length::Fill);
            if !unlock_disabled {
                unlock_btn = unlock_btn.on_press(Message::UnlockSubmitted);
            }

            let logout_btn = button::standard(fl!("logout")).on_press(Message::LogoutClicked);

            cosmic::widget::dialog()
                .icon(brand_icon(48))
                .title(fl!("vault-locked"))
                .body(fl!("enter-master-password-to-unlock"))
                .control(unlock_col)
                .primary_action(unlock_btn)
                .secondary_action(logout_btn)
                .width(Length::Fixed(400.0))
                .into()
        }
    }
}
