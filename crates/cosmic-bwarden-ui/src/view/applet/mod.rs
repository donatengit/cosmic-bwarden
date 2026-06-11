pub mod menu;
pub mod search;
pub mod unlock;

use cosmic::Element;
use cosmic::widget::{container, list_column, text, toaster};
use crate::app::CosmicBWardenApp;
use crate::message::{Message, View};
use crate::fl;

impl CosmicBWardenApp {
    pub fn applet_view(&self) -> Element<'_, Message> {
        let btn = self.core.applet.icon_button("password-manager-symbolic")
            .on_press_with_rectangle(move |offset, bounds| {
                Message::AppletIconClicked(offset, bounds)
            });

        cosmic::Element::from(self.core.applet.applet_tooltip::<Message>(
            btn,
            fl!("app-title"),
            self.applet_popup.is_some(),
            |a| Message::Surface(a),
            None,
        ))
    }

    pub fn applet_popup_content(&self) -> Element<'_, Message> {
        let is_unlocked = matches!(self.view, View::Vault | View::Settings);

        let mut content = list_column();
        content = content.add(menu::open_vault_button());

        content = if is_unlocked {
            content.add(search::view(self))
        } else {
            content.add(unlock::view(self))
        };

        if let Some(error) = &self.applet_error {
            content = content.add(container(text::body(error)).padding(5));
        }

        for button in menu::footer_buttons(self) {
            content = content.add(button);
        }

        toaster(&self.applet_toasts, self.core.applet.popup_container(container(content).padding(5)))
    }
}
