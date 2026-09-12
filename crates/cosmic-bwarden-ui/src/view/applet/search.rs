use crate::app::applet_search::{build_applet_rows, AppletRow, AppletRowKind};
use crate::app::CosmicBWardenApp;
use crate::fl;
use crate::message::Message;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{
    button, column, container, icon, row, scrollable, search_input, secure_input, text, tooltip, Id,
};
use cosmic::Element;

const RESULTS_SPACING: f32 = 5.0;
const RESULT_ROW_HEIGHT: f32 = 50.0;
const VISIBLE_RESULT_ROWS: f32 = 3.0;
const RESULTS_MAX_HEIGHT: f32 =
    RESULT_ROW_HEIGHT * VISIBLE_RESULT_ROWS + RESULTS_SPACING * (VISIBLE_RESULT_ROWS - 1.0);

pub fn view(app: &CosmicBWardenApp) -> Element<'_, Message> {
    let star_icon = if app.applet_search_only_favourites {
        "starred-symbolic"
    } else {
        "non-starred-symbolic"
    };

    let search_row = row::with_capacity(2)
        .spacing(5)
        .align_y(Alignment::Center)
        .push(
            search_input(fl!("search"), &app.applet_search_query)
                .on_input(Message::AppletSearchChanged)
                .width(Length::Fill),
        )
        .push(
            button::icon(icon::from_name(star_icon))
                .on_press(Message::AppletToggleFavouritesFilter),
        );

    let mut col = column::with_capacity(2).spacing(5);
    col = col.push(search_row);

    let rows = build_applet_rows(&app.applet_search_results);
    let mut results_col = column::with_capacity(rows.len().max(1)).spacing(RESULTS_SPACING);
    if rows.is_empty() {
        let empty_text = if app.applet_search_query.trim().is_empty() {
            fl!("no-pinned-entries")
        } else {
            fl!("no-results")
        };
        results_col = results_col.push(container(text::caption(empty_text)).padding(10));
    } else {
        for result_row in rows {
            if app.applet_reprompt_id.as_deref() == Some(result_row.id.as_str()) {
                results_col = results_col.push(reprompt_row(app));
            } else {
                results_col = results_col.push(result_row_view(result_row));
            }
        }
    }

    col = col.push(
        scrollable(container(results_col).padding([0, 12, 0, 0]))
            .height(Length::Fixed(RESULTS_MAX_HEIGHT)),
    );
    col.into()
}

fn row_action_btn(
    icon_name: &'static str,
    tooltip: String,
    on_press: Option<Message>,
) -> Element<'static, Message> {
    button::icon(icon::from_name(icon_name))
        .on_press_maybe(on_press)
        .tooltip(tooltip)
        .into()
}

fn result_row_view(row_data: AppletRow) -> Element<'static, Message> {
    match row_data.kind {
        AppletRowKind::Login { username, link } => {
            login_row_view(row_data.id, row_data.label, username, link)
        }
        AppletRowKind::SecureNote | AppletRowKind::SshKey => {
            secret_row_view(row_data.id, row_data.label)
        }
    }
}

fn login_row_view(
    id: String,
    label: String,
    username: Option<String>,
    link: Option<String>,
) -> Element<'static, Message> {
    let copy_id = id.clone();
    let label_btn = button::custom(text::body(label))
        .on_press_maybe(
            username
                .is_some()
                .then(|| Message::AppletCopyPrimary(copy_id)),
        )
        .width(Length::Fill)
        .class(cosmic::theme::Button::Text);

    // Move username into the tooltip so it lives as Cow::Owned('static)
    let label_el: Element<'static, Message> = if let Some(u) = username {
        tooltip(label_btn, text::caption(u), tooltip::Position::Bottom).into()
    } else {
        label_btn.into()
    };

    let vault_btn = row_action_btn(
        crate::view::symbolic::applet_open_in_vault_icon(),
        fl!("open-in-vault"),
        Some(Message::AppletOpenInVault(id.clone())),
    );
    let link_btn = row_action_btn(
        crate::view::symbolic::applet_open_link_icon(),
        fl!("open-uri"),
        link.map(Message::AppletOpenLink),
    );
    let secret_btn = row_action_btn(
        crate::view::symbolic::applet_copy_secret_icon(),
        fl!("copy-secret"),
        Some(Message::AppletCopySecret(id)),
    );

    row::with_capacity(2)
        .spacing(4)
        .align_y(Alignment::Center)
        .push(label_el)
        .push(
            row::with_capacity(3)
                .spacing(2)
                .push(vault_btn)
                .push(link_btn)
                .push(secret_btn),
        )
        .into()
}

fn secret_row_view(id: String, label: String) -> Element<'static, Message> {
    let vault_btn = row_action_btn(
        crate::view::symbolic::applet_open_in_vault_icon(),
        fl!("open-in-vault"),
        Some(Message::AppletOpenInVault(id.clone())),
    );
    let secret_btn = row_action_btn(
        crate::view::symbolic::applet_copy_secret_icon(),
        fl!("copy-secret"),
        Some(Message::AppletCopySecret(id.clone())),
    );

    let label_btn = button::custom(text::body(label))
        .on_press(Message::AppletCopySecret(id))
        .width(Length::Fill)
        .class(cosmic::theme::Button::Text);

    row::with_capacity(2)
        .spacing(4)
        .align_y(Alignment::Center)
        .push(label_btn)
        .push(
            row::with_capacity(2)
                .spacing(2)
                .push(vault_btn)
                .push(secret_btn),
        )
        .into()
}

pub fn reprompt_input_id() -> Id {
    Id::new("applet-reprompt-password")
}

fn reprompt_row(app: &CosmicBWardenApp) -> Element<'_, Message> {
    let password_input = secure_input(
        fl!("master-password"),
        &app.applet_reprompt_password,
        Some(Message::AppletToggleRepromptPasswordReveal),
        !app.applet_reprompt_password_revealed,
    )
    .id(reprompt_input_id())
    .on_input(Message::AppletRepromptPasswordChanged)
    .on_submit(|_| Message::AppletRepromptSubmitted)
    .width(Length::Fill);

    row::with_capacity(3)
        .spacing(5)
        .align_y(Alignment::Center)
        .push(password_input)
        .push(
            button::icon(icon::from_name("object-select-symbolic"))
                .on_press(Message::AppletRepromptSubmitted),
        )
        .push(
            button::icon(icon::from_name("window-close-symbolic"))
                .on_press(Message::AppletRepromptCancelled),
        )
        .into()
}
