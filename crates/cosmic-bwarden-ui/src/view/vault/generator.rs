use crate::app::CosmicBWardenApp;
use crate::fl;
use crate::message::Message;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{
    button, checkbox, column, container, icon, list_column, row, scrollable, secure_input,
    settings as cosmic_settings, slider, text,
};
use cosmic::Element;

const LENGTH_MIN: u32 = 8;
const LENGTH_MAX: u32 = 32;

/// Which section of the generator pane is active. Carried as the data of each
/// `generator_tabs` model item so the view can look up the active tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneratorTab {
    Settings,
    History,
}

/// Tab model for the generator pane. Item order is the tab order; the
/// Settings tab is activated both here and on every pane entry
/// (`GeneratorViewClicked`).
pub(crate) fn generator_tabs_model() -> cosmic::widget::segmented_button::SingleSelectModel {
    use cosmic::widget::segmented_button::Model;
    Model::builder()
        .insert(|b| {
            b.text(fl!("generator-tab-generate"))
                .data(GeneratorTab::Settings)
                .activate()
        })
        .insert(|b| {
            b.text(fl!("generator-tab-history"))
                .data(GeneratorTab::History)
        })
        .build()
}

impl CosmicBWardenApp {
    pub fn view_generator(&self) -> Element<'_, Message> {
        let tabs = cosmic::widget::tab_bar::horizontal(&self.generator_tabs)
            .on_activate(Message::GeneratorTabActivated);

        let active_tab = self
            .generator_tabs
            .active_data::<GeneratorTab>()
            .copied()
            .unwrap_or(GeneratorTab::Settings);
        let content = match active_tab {
            GeneratorTab::Settings => self.view_generator_settings(),
            GeneratorTab::History => self.view_generator_history(),
        };

        container(
            scrollable(
                cosmic::widget::column::with_capacity(6)
                    .spacing(20)
                    .push(text::title2(fl!("password-generator")))
                    .push(tabs)
                    .push(content),
            )
            .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(20)
        .into()
    }

    fn view_generator_settings(&self) -> Element<'_, Message> {
        let s = &self.generator_settings;

        let mut options = list_column();
        options = options.add(
            checkbox(s.uppercase)
                .label(fl!("uppercase"))
                .on_toggle(Message::GeneratorUppercaseToggled),
        );
        options = options.add(
            checkbox(s.lowercase)
                .label(fl!("lowercase"))
                .on_toggle(Message::GeneratorLowercaseToggled),
        );
        options = options.add(
            checkbox(s.numbers)
                .label(fl!("numbers"))
                .on_toggle(Message::GeneratorNumbersToggled),
        );
        options = options.add(
            checkbox(s.special)
                .label(fl!("special-characters"))
                .on_toggle(Message::GeneratorSpecialToggled),
        );
        let length_u32 = s.length as u32;
        options = options.add(cosmic_settings::item(
            fl!("password-length", length = length_u32),
            slider(
                LENGTH_MIN..=LENGTH_MAX,
                s.length as u32,
                Message::GeneratorLengthChanged,
            )
            .width(Length::Fixed(300.0)),
        ));

        let buttons = row::with_capacity(2)
            .spacing(10)
            .push(button::suggested(fl!("generate")).on_press(Message::GeneratorGenerateClicked))
            .push(button::standard(fl!("reset")).on_press(Message::GeneratorResetClicked));

        let result_row: Element<'_, Message> = if let Some(pw) = &self.generator_result {
            row::with_capacity(2)
                .spacing(10)
                .align_y(Alignment::Center)
                .push(
                    secure_input(
                        "",
                        pw.expose(),
                        Some(Message::GeneratorRevealToggled),
                        !self.generator_result_revealed,
                    )
                    .width(Length::Fill),
                )
                .push(
                    button::icon(icon::from_name("edit-copy-symbolic"))
                        .on_press(Message::CopyToClipboard(pw.expose().to_string())),
                )
                .into()
        } else {
            text::caption(fl!("no-password-generated-yet")).into()
        };

        let mut content = column::with_capacity(6)
            .spacing(20)
            .push(options)
            .push(buttons)
            .push(result_row);

        if let Some(err) = &self.generator_error {
            content = content.push(text::body(fl!("error-fmt", error = err.clone())).class(
                cosmic::theme::Text::Custom(|theme| cosmic::iced::widget::text::Style {
                    color: Some(theme.cosmic().destructive_text_color().into()),
                    ..Default::default()
                }),
            ));
        }

        content.into()
    }

    fn view_generator_history(&self) -> Element<'_, Message> {
        let mut list = list_column();
        list = list.add(text::title3(fl!("recent-passwords")));

        if self.generator_history.is_empty() {
            list = list.add(text::caption(fl!("no-recent-passwords")));
            return list.into();
        }

        for (idx, entry) in self.generator_history.iter().enumerate() {
            let revealed = self.generator_history_revealed.contains(&idx);
            list = list.add(
                row::with_capacity(4)
                    .spacing(10)
                    .align_y(Alignment::Center)
                    .push(
                        text::caption(crate::view::format_unix_timestamp_utc(entry.created_at))
                            .width(Length::Fixed(120.0)),
                    )
                    .push(
                        secure_input(
                            "",
                            entry.password.expose(),
                            Some(Message::GeneratorHistoryRevealToggled(idx)),
                            !revealed,
                        )
                        .width(Length::Fill),
                    )
                    .push(
                        button::icon(icon::from_name("edit-copy-symbolic")).on_press(
                            Message::CopyToClipboard(entry.password.expose().to_string()),
                        ),
                    )
                    .push(
                        button::icon(icon::from_name("user-trash-symbolic"))
                            .on_press(Message::GeneratorHistoryDeleteRequested(idx)),
                    ),
            );
        }

        list.into()
    }
}
