mod detail;
mod generator;
pub(crate) mod sidebar;

use crate::app::state::VaultPane;
use crate::app::CosmicBWardenApp;
use crate::fl;
use crate::message::{Message, View};
use cosmic::iced::Length;
use cosmic::widget::{container, pane_grid, text, PaneGrid};
use cosmic::Element;

impl CosmicBWardenApp {
    pub fn view_vault(&self) -> Element<'_, Message> {
        // padding: [top, right, bottom, left]
        PaneGrid::new(&self.vault_panes, move |_id, pane, _maximized| match pane {
            VaultPane::Sidebar => pane_grid::Content::new(
                container(self.view_sidebar())
                    .class(cosmic::theme::Container::Background)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .padding([12, 10, 12, 16]),
            ),
            VaultPane::Content => {
                let right_panel: Element<'_, Message> = if self.view == View::Settings {
                    self.view_settings()
                } else if self.view == View::PasswordGenerator {
                    self.view_generator()
                } else if let Some(entry) = &self.selected_entry {
                    self.view_entry_details(entry)
                } else {
                    container(text::body(fl!("select-entry")))
                        .center_x(Length::Fill)
                        .center_y(Length::Fill)
                        .into()
                };

                pane_grid::Content::new(
                    container(right_panel)
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .padding([12, 0, 0, 0]),
                )
            }
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .spacing(8)
        .on_resize(12, Message::PaneResized)
        .into()
    }
}
