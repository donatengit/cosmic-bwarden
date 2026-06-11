use cosmic::Element;
use cosmic::iced::Length;
use cosmic::widget::button;
use crate::app::CosmicBWardenApp;
use crate::message::{Message, View};
use crate::fl;

pub fn open_vault_button() -> Element<'static, Message> {
    button::text(fl!("open-vault-window"))
        .on_press(Message::SpawnApplication)
        .width(Length::Fill)
        .into()
}

pub fn footer_buttons(app: &CosmicBWardenApp) -> Vec<Element<'static, Message>> {
    let is_unlocked = matches!(app.view, View::Vault | View::Settings);

    let mut buttons = Vec::new();
    if is_unlocked {
        buttons.push(button::text(fl!("lock")).on_press(Message::LockClicked).width(Length::Fill).into());
    }
    buttons.push(button::text(fl!("logout-and-quit")).on_press(Message::LogoutAndQuit).width(Length::Fill).into());
    buttons.push(button::text(fl!("lock-and-quit")).on_press(Message::LockAndQuit).width(Length::Fill).into());
    buttons
}
