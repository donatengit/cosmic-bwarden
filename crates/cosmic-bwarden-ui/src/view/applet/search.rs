use cosmic::Element;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{button, column, container, icon, row, scrollable, search_input, secure_input, text, Id};
use crate::app::applet_search::{build_applet_rows, AppletRow};
use crate::app::CosmicBWardenApp;
use crate::message::Message;
use crate::fl;

/// Spacing between result rows.
const RESULTS_SPACING: f32 = 5.0;

/// Approximate height of a single result row, including its button padding.
const RESULT_ROW_HEIGHT: f32 = 44.0;

/// Number of result rows to show before the scrollable takes over.
const VISIBLE_RESULT_ROWS: f32 = 3.0;

/// Maximum height of the scrollable results area, so the popup doesn't
/// resize (and get repositioned by the compositor) as the result count
/// changes while typing. Sized to show `VISIBLE_RESULT_ROWS` rows; any
/// additional results are reachable by scrolling.
const RESULTS_MAX_HEIGHT: f32 = RESULT_ROW_HEIGHT * VISIBLE_RESULT_ROWS + RESULTS_SPACING * (VISIBLE_RESULT_ROWS - 1.0);

fn secret_label(key: &str) -> String {
    match key {
        "note-label" => fl!("note-label"),
        "private-key-label" => fl!("private-key-label"),
        "public-key-label" => fl!("public-key-label"),
        _ => fl!("password-label"),
    }
}

/// Muted/secondary text color, used for the "Note" hint on secure note rows.
fn muted_text() -> cosmic::theme::Text {
    cosmic::theme::Text::Custom(|theme| cosmic::iced::widget::text::Style {
        color: Some(cosmic::iced::Color::from(theme.cosmic().background.component.on_disabled)),
        ..Default::default()
    })
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
    let mut results_col = column::with_capacity(rows.len().max(1)).spacing(RESULTS_SPACING);
    if rows.is_empty() {
        let empty_text = if app.applet_search_query.trim().is_empty() { fl!("no-pinned-entries") } else { fl!("no-results") };
        results_col = results_col.push(container(text::body(empty_text)).padding(10));
    } else {
        for result_row in rows {
            if app.applet_reprompt_id.as_deref() == Some(result_row.id.as_str()) {
                results_col = results_col.push(reprompt_row(app));
            } else {
                results_col = results_col.push(result_row_view(result_row));
            }
        }
    }

    col = col.push(scrollable(results_col).height(Length::Fixed(RESULTS_MAX_HEIGHT)));

    col.into()
}

fn result_row_view(result_row: AppletRow) -> Element<'static, Message> {
    if result_row.single_button {
        return note_row_view(result_row);
    }

    let primary_id = result_row.id.clone();
    let secret_id = result_row.id.clone();

    row::with_capacity(2)
        .spacing(5)
        .push(button::custom(text::body(result_row.primary_label))
            .on_press_maybe(result_row.primary_value.map(|_| Message::AppletCopyPrimary(primary_id)))
            .width(Length::FillPortion(1))
            .class(cosmic::theme::Button::Text))
        .push(button::suggested(secret_label(result_row.secret_label_key))
            .on_press(Message::AppletCopySecret(secret_id))
            .width(Length::FillPortion(1)))
        .into()
}

fn note_row_view(result_row: AppletRow) -> Element<'static, Message> {
    let secret_id = result_row.id.clone();

    let content = row::with_capacity(2)
        .spacing(5)
        .align_y(Alignment::Center)
        .push(text::body(result_row.primary_label).width(Length::Fill))
        .push(text::caption(secret_label(result_row.secret_label_key)).class(muted_text()));

    button::custom(content)
        .on_press(Message::AppletCopySecret(secret_id))
        .width(Length::Fill)
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
