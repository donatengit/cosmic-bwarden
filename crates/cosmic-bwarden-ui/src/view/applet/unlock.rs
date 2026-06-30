use cosmic::Element;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{button, column, icon, row, secure_input, Id};
use crate::app::CosmicBWardenApp;
use crate::message::Message;
use crate::fl;

pub fn password_input_id() -> Id {
    Id::new("applet-unlock-password")
}

pub fn pin_input_id() -> Id {
    Id::new("applet-unlock-pin")
}

pub fn view(app: &CosmicBWardenApp) -> Element<'_, Message> {
    if app.show_pin_unlock {
        let pin_input = secure_input(
            fl!("locked-need-pin"),
            &app.applet_pin,
            Some(Message::AppletTogglePinReveal),
            !app.applet_pin_revealed,
        )
        .id(pin_input_id())
        .on_input(Message::AppletPinChanged)
        .on_submit(|_| Message::AppletPinSubmitted)
        .width(Length::Fill);

        let submit_btn = button::icon(icon::from_name("changes-allow-symbolic"))
            .on_press(Message::AppletPinSubmitted);

        let pin_row = row::with_capacity(2)
            .spacing(5)
            .align_y(Alignment::Center)
            .push(pin_input)
            .push(submit_btn);

        let fallback_btn = button::text(fl!("use-master-password-instead"))
            .on_press(Message::AppletUseMasterPasswordInstead);

        column::with_capacity(2)
            .spacing(5)
            .push(pin_row)
            .push(fallback_btn)
            .into()
    } else {
        let password_input = secure_input(
            fl!("locked-need-password"),
            &app.applet_unlock_password,
            Some(Message::AppletToggleUnlockPasswordReveal),
            !app.applet_unlock_password_revealed,
        )
        .id(password_input_id())
        .on_input(Message::AppletUnlockPasswordChanged)
        .on_submit(|_| Message::AppletUnlockSubmitted)
        .width(Length::Fill);

        let submit_btn = button::icon(icon::from_name("changes-allow-symbolic"))
            .on_press(Message::AppletUnlockSubmitted);

        row::with_capacity(2)
            .spacing(5)
            .align_y(Alignment::Center)
            .push(password_input)
            .push(submit_btn)
            .into()
    }
}
