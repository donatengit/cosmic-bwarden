use cosmic::Element;
use cosmic::iced::{Length, Alignment};
use cosmic::widget::{button, text};
use crate::app::CosmicBWardenApp;
use crate::message::{Message, View};
use crate::fl;

pub fn open_vault_button() -> Element<'static, Message> {
    let btn = button::text(fl!("open-vault-window"))
        .on_press(Message::OpenVaultRequested)
        .width(Length::Fill);

    let version_text = text::caption(cosmic_bwarden_core::version())
        .class(crate::view::style::muted_text());

    cosmic::widget::row::with_capacity(2)
        .spacing(10)
        .align_y(Alignment::Center)
        .push(btn)
        .push(version_text)
        .into()
}

pub fn footer_buttons(app: &CosmicBWardenApp) -> Vec<Element<'static, Message>> {
    let is_unlocked = matches!(app.view, View::Vault | View::Settings);

    let mut buttons = Vec::new();
    if is_unlocked {
        buttons.push(button::text(fl!("lock")).on_press(Message::LockClicked).width(Length::Fill).into());
        buttons.push(button::text(fl!("logout")).on_press(Message::LogoutClicked).width(Length::Fill).into());
        buttons.push(button::text(fl!("lock-and-quit")).on_press(Message::LockAndQuit).width(Length::Fill).into());
        buttons.push(button::text(fl!("logout-and-quit")).on_press(Message::LogoutAndQuit).width(Length::Fill).into());
    }
    buttons.push(button::text(fl!("quit")).on_press(Message::Exit).width(Length::Fill).into());
    buttons
}
