pub mod menu;
pub mod search;
pub mod unlock;

use crate::app::CosmicBWardenApp;
use crate::fl;
use crate::message::{Message, View};
use cosmic::iced::Length;
use cosmic::widget::{button, container, icon, list_column, text, toaster};
use cosmic::Element;

/// Dedicated branded panel icon: the repo's brand mark simplified for small
/// sizes (`icons/black_simplified.svg`'s drawable content, design-tool export
/// metadata stripped), embedded in the binary rather than looked up by name
/// in the system icon theme, so it renders correctly without any install-time
/// step (dev builds included). The panel renders this at a few dozen pixels
/// at most, where the full detailed brand mark (`FULL_ICON_SVG` below)
/// muddies into an illegible blob — the simplified glyph keeps its shapes
/// distinguishable at that size. A single monochrome source is enough — no
/// separate light/dark variant is needed, since `symbolic(true)` makes
/// libcosmic discard the SVG's own paint and recolor the whole shape to the
/// panel's foreground itself (`icon_button_from_handle` in
/// `libcosmic/src/applet/mod.rs`).
const APPLET_ICON_SVG: &[u8] =
    include_bytes!("../../../resources/icons/cosmic-bwarden-symbolic.svg");

impl CosmicBWardenApp {
    pub fn applet_view(&self) -> Element<'_, Message> {
        let icon_handle = icon::from_svg_bytes(APPLET_ICON_SVG).symbolic(true);
        let btn = self
            .core
            .applet
            .icon_button_from_handle(icon_handle)
            .on_press_with_rectangle(move |offset, bounds| {
                Message::AppletIconClicked(offset, bounds)
            });

        cosmic::Element::from(self.core.applet.applet_tooltip::<Message>(
            btn,
            fl!("app-title"),
            self.applet_popup.is_some(),
            Message::Surface,
            None,
        ))
    }

    pub fn applet_popup_content(&self) -> Element<'_, Message> {
        // If protocol versions don't match, show only the error message and Quit.
        if self.protocol_mismatch {
            let mut content = list_column();
            content =
                content.add(container(text::body(fl!("protocol-version-mismatch"))).padding(10));
            content = content.add(
                button::text(fl!("quit"))
                    .on_press(Message::Exit)
                    .width(Length::Fill),
            );
            return toaster(
                &self.applet_toasts,
                self.core
                    .applet
                    .popup_container(container(content).padding(5)),
            );
        }

        let mut content = list_column();
        content = content.add(menu::header_row(self));

        content = if self.view.is_unlocked() {
            content.add(search::view(self))
        } else if self.view == View::Setup {
            content.add(container(text::body(fl!("not-configured"))).padding(10))
        } else {
            content.add(unlock::view(self))
        };

        if let Some(error) = &self.applet_error {
            content = content.add(container(text::body(error)).padding(5));
        }

        for item in menu::quit_footer(self) {
            content = content.add(item);
        }

        toaster(
            &self.applet_toasts,
            self.core
                .applet
                .popup_container(container(content).padding(5)),
        )
    }
}
