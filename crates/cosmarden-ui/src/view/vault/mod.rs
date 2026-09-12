mod detail;
pub(crate) mod generator;
pub(crate) mod sidebar;

use crate::app::state::{VaultPane, SIDEBAR_MIN_WIDTH};
use crate::app::CosmardenApp;
use crate::fl;
use crate::message::{Message, View};
use cosmic::iced::Length;
use cosmic::widget::{container, pane_grid, text, PaneGrid};
use cosmic::Element;

impl CosmardenApp {
    /// The content (right) pane: settings, generator, entry details, or an
    /// empty "select an entry" placeholder. Shared by the placeholder layout
    /// and the resizable split so the two can't drift.
    fn view_vault_right_panel(&self) -> Element<'_, Message> {
        if self.view == View::Settings {
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
        }
    }

    /// The resizable sidebar|content split. Only built once the first window
    /// width is known; until then [`vault_window_resized`] would snap the
    /// split, and building it earlier means the first frame lays the sidebar
    /// out at a guessed percentage that then visibly jumps to the real width.
    fn view_vault_split(&self) -> Element<'_, Message> {
        // padding: [top, right, bottom, left]
        PaneGrid::new(&self.vault_panes, move |_id, pane, _maximized| match pane {
            VaultPane::Sidebar => pane_grid::Content::new(
                container(self.view_sidebar())
                    .class(cosmic::theme::Container::Background)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .padding([12, 10, 12, 16]),
            ),
            VaultPane::Content => pane_grid::Content::new(
                container(self.view_vault_right_panel())
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .padding([12, 0, 0, 0]),
            ),
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .spacing(8)
        .on_resize(12, Message::PaneResized)
        .into()
    }

    pub fn view_vault(&self) -> Element<'_, Message> {
        if self.vault_window_width.is_none() {
            // No window width yet: render a *fixed* layout with the sidebar at
            // exactly `SIDEBAR_MIN_WIDTH` so it comes up at the same size the
            // resizable split will settle on once `vault_window_resized` runs.
            // Building the chunked PaneGrid here would draw the sidebar at
            // `SIDEBAR_DEFAULT_RATIO` (a guessed share of the window), which
            // then snaps to the pixel minimum on the first resize report and
            // reads as a flicker/redraw.
            cosmic::widget::row::with_capacity(2)
                .spacing(8)
                .push(
                    container(self.view_sidebar())
                        .class(cosmic::theme::Container::Background)
                        .width(Length::Fixed(SIDEBAR_MIN_WIDTH))
                        .height(Length::Fill)
                        .padding([12, 10, 12, 16]),
                )
                .push(
                    container(self.view_vault_right_panel())
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .padding([12, 0, 0, 0]),
                )
                .into()
        } else {
            self.view_vault_split()
        }
    }
}
