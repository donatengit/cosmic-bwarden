use crate::app::CosmicBWardenApp;
use crate::fl;
use crate::message::Message;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{button, container, icon, row, text, tooltip};
use cosmic::Element;

/// Top header row: "Open Vault" (or "⚠ Not synced" in red) + Lock|Logout icon buttons.
pub fn header_row(app: &CosmicBWardenApp) -> Element<'static, Message> {
    let is_unlocked = app.view.is_unlocked();

    let open_btn: Element<'static, Message> = if app.sync_failed && is_unlocked {
        button::destructive(fl!("open-vault-window"))
            .on_press(Message::OpenVaultRequested)
            .width(Length::Fill)
            .into()
    } else {
        button::text(fl!("open-vault-window"))
            .on_press(Message::OpenVaultRequested)
            .width(Length::Fill)
            .into()
    };

    let mut action_row = row::with_capacity(4).spacing(0).align_y(Alignment::Center);

    if app.sync_failed && is_unlocked {
        let session_expired = app
            .error
            .as_deref()
            .map(|e| e.contains("session token"))
            .unwrap_or(false);
        let (icon_name, tooltip_label, action) = if session_expired {
            (
                "dialog-password-symbolic",
                fl!("sync-session-expired-tooltip"),
                Message::LogoutClicked,
            )
        } else {
            (
                "network-error-symbolic",
                fl!("sync-not-synced-tooltip"),
                Message::SyncClicked,
            )
        };
        let not_synced_btn = tooltip(
            button::icon(icon::from_name(icon_name)).on_press(action),
            text::caption(tooltip_label),
            tooltip::Position::Bottom,
        );
        action_row = action_row.push(not_synced_btn);
    }

    // Unconditional (not gated by is_unlocked): generation is local RNG, not a
    // vault operation, so it must work while locked or even before login —
    // it always uses whatever settings were last saved by any surface.
    let generate_btn = tooltip(
        // TODO: temporary placeholder — insert-drawing-symbolic exists in the
        // Cosmic icon theme but reads as "draw/sketch", not "generate".
        button::icon(icon::from_name("insert-drawing-symbolic"))
            .on_press(Message::AppletGeneratePasswordRequested),
        text::caption(fl!("generate-password")),
        tooltip::Position::Bottom,
    );
    action_row = action_row.push(generate_btn);

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
        button::icon(icon::from_name("system-log-out-symbolic")).on_press(Message::LogoutClicked),
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

/// Quit footer: a single "Quit" button that expands to show sub-actions.
pub fn quit_footer(app: &CosmicBWardenApp) -> Vec<Element<'static, Message>> {
    let is_unlocked = app.view.is_unlocked();

    let label = if app.applet_quit_expanded {
        fl!("quit-menu-expanded", label = fl!("quit"))
    } else {
        fl!("quit-menu-collapsed", label = fl!("quit"))
    };

    let mut items: Vec<Element<'static, Message>> = vec![button::text(label)
        .on_press(Message::AppletQuitMenuToggle)
        .width(Length::Fill)
        .into()];

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
