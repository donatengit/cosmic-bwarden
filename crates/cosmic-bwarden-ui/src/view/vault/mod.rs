mod detail;
pub(crate) mod sidebar;

use cosmic::Element;
use cosmic::iced::Length;
use cosmic::widget::{container, divider, text};
use crate::app::CosmicBWardenApp;
use crate::message::{Message, View};
use crate::fl;

impl CosmicBWardenApp {
    pub fn view_vault(&self) -> Element<'_, Message> {
        let sidebar = self.view_sidebar();

        let right_panel: Element<'_, Message> = if self.view == View::Settings {
            self.view_settings()
        } else if let Some(entry) = &self.selected_entry {
            self.view_entry_details(entry)
        } else {
            container(text::body(fl!("select-entry"))).center_x(Length::Fill).center_y(Length::Fill).into()
        };

        cosmic::widget::row::with_capacity(2).spacing(0)
            .push(container(sidebar)
                .class(cosmic::theme::Container::Background)
                .width(Length::Fixed(300.0))
                .height(Length::Fill)
                .padding(10))
            .push(divider::vertical::default())
            .push(container(right_panel).width(Length::Fill).height(Length::Fill))
            .into()
    }
}
