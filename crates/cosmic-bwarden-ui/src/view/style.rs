/// Muted/secondary text color, used for hints and debug captions.
pub fn muted_text() -> cosmic::theme::Text {
    cosmic::theme::Text::Custom(|theme| cosmic::iced::widget::text::Style {
        color: Some(cosmic::iced::Color::from(
            theme.cosmic().background(false).component.on_disabled,
        )),
        ..Default::default()
    })
}

/// Full detailed brand mark (`icons/black_simplified.svg`'s small-size sibling
/// is used for the panel icon instead — see `view::applet::APPLET_ICON_SVG`),
/// for contexts with room to render it legibly: dialog/popup headings and the
/// Settings pane.
const FULL_ICON_SVG: &[u8] =
    include_bytes!("../../resources/icons/cosmic-bwarden-full-symbolic.svg");

/// The full brand mark as a widget, recolored to the theme's foreground color
/// (same `symbolic(true)` treatment as the panel icon) so it matches
/// surrounding heading text in both light and dark themes.
pub fn brand_icon(size: u16) -> cosmic::widget::Icon {
    cosmic::widget::icon(cosmic::widget::icon::from_svg_bytes(FULL_ICON_SVG).symbolic(true))
        .size(size)
}
