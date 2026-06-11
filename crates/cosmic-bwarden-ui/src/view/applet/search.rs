use cosmic::Element;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{button, column, container, icon, row, search_input, secure_input, text, Id};
use crate::app::applet_search::build_applet_rows;
use crate::app::CosmicBWardenApp;
use crate::message::Message;
use crate::fl;

fn secret_label(key: &str) -> String {
    match key {
        "note-label" => fl!("note-label"),
        "private-key-label" => fl!("private-key-label"),
        _ => fl!("password-label"),
    }
}

pub fn view(app: &CosmicBWardenApp) -> Element<'_, Message> {
    let star_icon = if app.applet_search_only_favourites { "starred-symbolic" } else { "non-starred-symbolic" };

    let search_row = row::with_capacity(2)
        .spacing(5)
        .align_y(Alignment::Center)
        .push(search_input(fl!("search"), &app.applet_search_query)
            .on_input(Message::AppletSearchChanged)
            .width(Length::Fill))
        .push(button::icon(icon::from_name(star_icon)).on_press(Message::AppletToggleFavouritesFilter));

    let mut col = column::with_capacity(2).spacing(5);
    col = col.push(search_row);

    let rows = build_applet_rows(&app.applet_search_results);
    if rows.is_empty() {
        let empty_text = if app.applet_search_query.trim().is_empty() { fl!("no-pinned-entries") } else { fl!("no-results") };
        col = col.push(container(text::body(empty_text)).padding(10));
    } else {
        for result_row in rows {
            if app.applet_reprompt_id.as_deref() == Some(result_row.id.as_str()) {
                col = col.push(reprompt_row(app));
            } else {
                col = col.push(result_row_view(result_row));
            }
        }
    }

    col.into()
}

fn result_row_view(result_row: crate::app::applet_search::AppletRow) -> Element<'static, Message> {
    let primary_id = result_row.id.clone();
    let secret_id = result_row.id.clone();

    row::with_capacity(2)
        .spacing(5)
        .push(button::text(result_row.primary_label)
            .on_press_maybe(result_row.primary_value.map(|_| Message::AppletCopyPrimary(primary_id)))
            .width(Length::FillPortion(1)))
        .push(button::text(secret_label(result_row.secret_label_key))
            .on_press(Message::AppletCopySecret(secret_id))
            .width(Length::FillPortion(1)))
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
        .push(button::icon(icon::from_name("object-select-symbolic")).on_press(Message::AppletRepromptSubmitted))
        .push(button::icon(icon::from_name("window-close-symbolic")).on_press(Message::AppletRepromptCancelled))
        .into()
}
