use crate::app::CosmicBWardenApp;
use crate::fl;
use crate::message::Message;
use crate::view::style::muted_text;
use crate::MIN_PIN_LEN;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{
    button, container, list_column, row, secure_input, settings as cosmic_settings, slider, text,
};
use cosmic::Element;

const LOCK_MIN: u32 = 5;
const LOCK_MAX: u32 = 90;
const LOCK_STEP: u32 = 5;

impl CosmicBWardenApp {
    pub fn view_settings(&self) -> Element<'_, Message> {
        let is_editing = self.editing_config.is_some();
        let config = self.editing_config.as_ref().unwrap_or(&self.config);

        let mut col = list_column();

        if is_editing {
            col = col.add(cosmic_settings::item(
                fl!("server"),
                cosmic::widget::text_input::text_input(
                    fl!("server-label"),
                    config.base_url.as_deref().unwrap_or(""),
                )
                .on_input(Message::SettingsServerChanged)
                .width(Length::Fixed(300.0)),
            ));

            {
                let minutes =
                    (config.lock_timeout / 60).clamp(LOCK_MIN as u64, LOCK_MAX as u64) as u32;
                col = col.add(cosmic_settings::item(
                    fl!("autolock-minutes", minutes = minutes),
                    slider(
                        LOCK_MIN..=LOCK_MAX,
                        minutes,
                        Message::SettingsLockTimeoutChanged,
                    )
                    .step(LOCK_STEP)
                    .width(Length::Fixed(300.0)),
                ));
            }
        } else {
            col = col.add(cosmic_settings::item(
                fl!("account"),
                text::body(config.email.as_deref().unwrap_or("—").to_string()),
            ));
            col = col.add(cosmic_settings::item(
                fl!("server-label"),
                text::body(
                    config
                        .base_url
                        .as_deref()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| fl!("bitwarden-cloud")),
                ),
            ));
            let lock_minutes = config.lock_timeout / 60;
            let lock_label = fl!("minutes-fmt", minutes = lock_minutes);
            col = col.add(cosmic_settings::item(
                fl!("auto-lock"),
                text::body(lock_label),
            ));
        }

        let header = cosmic::widget::row::with_capacity(2)
            .spacing(10)
            .align_y(Alignment::Center)
            .push(text::title2(fl!("settings")).width(Length::Fill))
            .push(if is_editing {
                Element::from(
                    row::with_capacity(2)
                        .spacing(5)
                        .push(button::suggested(fl!("save")).on_press(Message::SettingsSaveClicked))
                        .push(
                            button::standard(fl!("cancel"))
                                .on_press(Message::SettingsCancelClicked),
                        ),
                )
            } else {
                button::suggested(fl!("edit"))
                    .on_press(Message::SettingsEditClicked)
                    .into()
            });

        let mut content = cosmic::widget::column::with_capacity(3)
            .spacing(20)
            .push(header)
            .push(col);

        // TPM section — shown whenever the feature is compiled in
        let tpm_section = self.tpm_settings_section();
        content = content.push(tpm_section);

        if !is_editing {
            content = content.push(
                button::custom(
                    text::caption(format!("v{}", cosmic_bwarden_core::version()))
                        .class(muted_text()),
                )
                .on_press(Message::CopyToClipboard(
                    cosmic_bwarden_core::version().to_string(),
                ))
                .class(cosmic::theme::Button::Text)
                .padding([2, 0]),
            );
        }

        container(cosmic::widget::scrollable(content).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(20)
            .into()
    }

    fn tpm_settings_section(&self) -> Element<'_, Message> {
        let mut col = list_column();
        col = col.add(text::title3(fl!("pin-unlock-title")));

        if !self.tpm_status_known {
            // Still waiting for the first CheckTpm response — show nothing
            // rather than a misleading "not accessible" message.
            col = col.add(text::body(fl!("checking-tpm")).class(muted_text()));
            return col.into();
        }

        if !self.tpm_available {
            // Hardware not accessible — show diagnostics to help the user fix it.
            if self.tpm_diagnostics.is_empty() {
                col = col.add(text::body(fl!("tpm-not-accessible")).class(muted_text()));
            } else {
                for (label, passed, hint) in &self.tpm_diagnostics {
                    let icon = if *passed { "✅" } else { "❌" };
                    let row_content = cosmic::widget::column::with_capacity(2)
                        .push(text::body(format!("{} {}", icon, label)))
                        .push(if !passed {
                            Element::from(text::caption(hint.as_str()).class(muted_text()))
                        } else {
                            Element::from(text::caption(""))
                        });
                    col = col.add(row_content);
                }
            }
            return col.into();
        }

        // TPM is available — show the toggle.
        // The toggle appears ON while the setup form is open (even before saved).
        let tpm_toggle_state = self.tpm_configured || self.show_tpm_setup_form;
        let tpm_toggle = cosmic::widget::toggler(tpm_toggle_state).on_toggle(|on| {
            if on {
                Message::TpmSetupFormToggle
            } else {
                Message::TpmDisableFormToggle
            }
        });
        let status_label = if tpm_toggle_state {
            fl!("status-active")
        } else {
            fl!("status-not-configured")
        };
        col = col.add(cosmic_settings::item(
            fl!("pin-unlock"),
            row::with_capacity(2)
                .spacing(10)
                .align_y(cosmic::iced::Alignment::Center)
                .push(text::caption(status_label).class(muted_text()))
                .push(tpm_toggle),
        ));

        if let Some(err) = &self.tpm_error {
            col = col.add(
                text::body(fl!("error-fmt", error = err.clone()))
                    // Theme's destructive color instead of a hardcoded red so the
                    // error text tracks light/dark/high-contrast variants.
                    .class(cosmic::theme::Text::Custom(|theme| {
                        cosmic::iced::widget::text::Style {
                            color: Some(theme.cosmic().destructive_text_color().into()),
                            ..Default::default()
                        }
                    })),
            );
        }

        // Dictionary-attack lockout status (attempts remaining before the TPM
        // locks out). Refreshed on settings open and after enable/disable.
        if let Some(line) = self.tpm_da_line() {
            col = col.add(text::caption(line).class(muted_text()));
        }

        if self.tpm_configured && self.show_tpm_disable_form {
            // No master password required — vault is already unlocked (that's the authorization).
            col = col.add(text::body(fl!("pin-will-be-removed")).class(muted_text()));
            col = col.add(
                row::with_capacity(2)
                    .spacing(5)
                    .push(
                        button::destructive(fl!("disable-pin-unlock"))
                            .on_press(Message::TpmDisableSubmitted),
                    )
                    .push(button::standard(fl!("cancel")).on_press(Message::TpmDisableFormToggle)),
            );
        } else if !self.tpm_configured && self.show_tpm_setup_form {
            // Setup form: only needs the new PIN (vault is already unlocked).
            let pin = secure_input(
                fl!("new-pin-min-chars", count = MIN_PIN_LEN),
                &self.tpm_setup_pin,
                Some(Message::TpmSetupPinRevealToggled),
                !self.tpm_setup_pin_revealed,
            )
            .on_input(Message::TpmSetupPinChanged)
            .on_submit(|_| Message::TpmSetupSubmitted)
            .width(Length::Fixed(300.0));
            col = col.add(cosmic_settings::item(fl!("new-pin"), pin));
            col = col.add(text::caption(fl!("pin-tpm-note-settings")).class(muted_text()));
            col = col.add(
                row::with_capacity(2)
                    .spacing(5)
                    .push(button::suggested(fl!("enable")).on_press(Message::TpmSetupSubmitted))
                    .push(button::standard(fl!("cancel")).on_press(Message::TpmSetupFormToggle)),
            );
        }

        // Server credentials toggle — only shown when PIN unlock is configured.
        if self.tpm_configured {
            let creds_toggle = cosmic::widget::toggler(self.tpm_server_credentials)
                .on_toggle(Message::TpmServerCredentialsToggled);
            col = col.add(cosmic_settings::item(
                fl!("store-hashed-password-tpm"),
                creds_toggle,
            ));
            col = col.add(text::caption(fl!("store-hashed-password-tpm-note")).class(muted_text()));
        }

        col.into()
    }
}
