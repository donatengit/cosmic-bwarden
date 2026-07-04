/// Muted/secondary text color, used for hints and debug captions.
pub fn muted_text() -> cosmic::theme::Text {
    cosmic::theme::Text::Custom(|theme| cosmic::iced::widget::text::Style {
        color: Some(cosmic::iced::Color::from(
            theme.cosmic().background.component.on_disabled,
        )),
        ..Default::default()
    })
}
