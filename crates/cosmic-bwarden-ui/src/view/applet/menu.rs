use crate::app::applet_menu::{quit_actions, QuitAction};
use crate::app::CosmicBWardenApp;
use crate::fl;
use crate::message::Message;
use cosmic::applet::{menu_button, menu_control_padding};
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{button, icon, row, space, text, tooltip};
use cosmic::Element;

/// Top header row: "Open Vault" (sync-failed uses a destructive style) + icon buttons.
pub fn header_row(app: &CosmicBWardenApp) -> Element<'static, Message> {
    let is_unlocked = app.view.is_unlocked();

    // Same `menu_control_padding` as the Quit `menu_button` and search rows
    // so the Open Vault label lines up with the rows below.
    let open_btn: Element<'static, Message> = if app.sync_failed && is_unlocked {
        button::destructive(fl!("open-vault-window"))
            .on_press(Message::OpenVaultRequested)
            .padding(menu_control_padding())
            .width(Length::Fill)
            .into()
    } else {
        menu_button(text::body(fl!("open-vault-window")))
            .on_press(Message::OpenVaultRequested)
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

    // Same total right gutter as search-row action icons (`space_xs` on the
    // overlay plus `space_xs` on the scrollable) so the last header icon
    // matches those rows instead of sitting on the popup edge.
    let space_xs = cosmic::theme::active().cosmic().spacing.space_xs;
    row::with_capacity(3)
        .spacing(5)
        .align_y(Alignment::Center)
        .push(open_btn)
        .push(action_row)
        .push(space::horizontal().width(Length::Fixed(f32::from(space_xs.saturating_mul(2)))))
        .into()
}

/// Single native `menu_button` Quit/Exit row. AppletMenu chrome (not
/// Destructive) — filled red buttons are not how COSMIC applet menus look.
pub fn quit_footer() -> Vec<Element<'static, Message>> {
    quit_actions().iter().copied().map(quit_row).collect()
}

fn quit_row(action: QuitAction) -> Element<'static, Message> {
    menu_button(text::body(action.label()))
        .on_press(action.message())
        .into()
}
