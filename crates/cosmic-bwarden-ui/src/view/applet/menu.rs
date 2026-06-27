use cosmic::Element;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{button, container, icon, row, text, tooltip};
use crate::app::CosmicBWardenApp;
use crate::message::{Message, View};
use crate::fl;

/// Top header row: "Open Vault" + Lock|Logout icon buttons (or just Logout when locked).
pub fn header_row(app: &CosmicBWardenApp) -> Element<'static, Message> {
    let open_btn = button::text(fl!("open-vault-window"))
        .on_press(Message::OpenVaultRequested)
        .width(Length::Fill);

    let is_unlocked = matches!(app.view, View::Vault | View::Settings);

    let mut action_row = row::with_capacity(2).spacing(0).align_y(Alignment::Center);

    if is_unlocked {
        let lock_btn = tooltip(
            button::icon(icon::from_name("system-lock-screen-symbolic"))
                .on_press(Message::LockClicked),
            text::caption(fl!("lock")),
            tooltip::Position::Bottom,
        );
        action_row = action_row.push(lock_btn);
    }

    let logout_btn = tooltip(
        button::icon(icon::from_name("system-log-out-symbolic"))
            .on_press(Message::LogoutClicked),
        text::caption(fl!("logout")),
        tooltip::Position::Bottom,
    );
    action_row = action_row.push(logout_btn);

    row::with_capacity(2)
        .spacing(5)
        .align_y(Alignment::Center)
        .push(open_btn)
        .push(action_row)
        .into()
}

/// Version caption row, shown below the header.
pub fn version_row() -> Element<'static, Message> {
    container(
        text::caption(cosmic_bwarden_core::version())
            .class(crate::view::style::muted_text()),
    )
    .padding([0, 4])
    .into()
}

/// Quit footer: a single "Quit" button that expands to show sub-actions.
pub fn quit_footer(app: &CosmicBWardenApp) -> Vec<Element<'static, Message>> {
    let is_unlocked = matches!(app.view, View::Vault | View::Settings);

    let label = if app.applet_quit_expanded {
        format!("▾ {}", fl!("quit"))
    } else {
        format!("▸ {}", fl!("quit"))
    };

    let mut items: Vec<Element<'static, Message>> = vec![
        button::text(label)
            .on_press(Message::AppletQuitMenuToggle)
            .width(Length::Fill)
            .into(),
    ];

    if app.applet_quit_expanded {
        if is_unlocked {
            items.push(
                container(
                    button::text(fl!("lock-and-quit"))
                        .on_press(Message::LockAndQuit)
                        .width(Length::Fill),
                )
                .padding([0, 0, 0, 16])
                .into(),
            );
            items.push(
                container(
                    button::text(fl!("logout-and-quit"))
                        .on_press(Message::LogoutAndQuit)
                        .width(Length::Fill),
                )
                .padding([0, 0, 0, 16])
                .into(),
            );
        }
        items.push(
            container(
                button::text(fl!("just-quit"))
                    .on_press(Message::Exit)
                    .width(Length::Fill),
            )
            .padding([0, 0, 0, 16])
            .into(),
        );
    }

    items
}
