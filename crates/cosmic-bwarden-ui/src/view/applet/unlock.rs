use cosmic::Element;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{button, icon, row, secure_input, Id};
use crate::app::CosmicBWardenApp;
use crate::message::Message;
use crate::fl;

pub fn password_input_id() -> Id {
    Id::new("applet-unlock-password")
}

pub fn view(app: &CosmicBWardenApp) -> Element<'_, Message> {
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
