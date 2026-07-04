pub mod menu;
pub mod search;
pub mod unlock;

use crate::app::CosmicBWardenApp;
use crate::fl;
use crate::message::{Message, View};
use cosmic::iced::Length;
use cosmic::widget::{button, container, list_column, text, toaster};
use cosmic::Element;

impl CosmicBWardenApp {
    pub fn applet_view(&self) -> Element<'_, Message> {
        let btn = self
            .core
            .applet
            // TODO: replace with our own branded icons (icons/black*.png / white*.png at repo root).
            // Requires either installing them into the hicolor XDG theme via the justfile `install`
            // recipe, or switching to icon::from_path() with a theme-aware path (dark/light variant).
            .icon_button("password-manager-symbolic")
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

        content = match self.view {
            View::Vault | View::Settings => content.add(search::view(self)),
            View::Setup => content.add(
                container(text::body(fl!("not-configured")))
                    .padding(10),
            ),
            _ => content.add(unlock::view(self)),
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
