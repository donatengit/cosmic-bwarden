use cosmic::Element;
use cosmic::iced::Length;
use cosmic::widget::{button, container, text, list_column, divider};
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
        let mut content = list_column();
        let is_unlocked = matches!(self.view, View::Vault | View::Settings);

        content = content.add(button::text(fl!("app-title")).on_press(Message::SpawnApplication).width(Length::Fill));

        if is_unlocked {
            if self.top_entries.is_empty() {
                content = content.add(container(text::body(fl!("no-pinned-entries"))).padding(10));
            } else {
                content = content.add(container(text::body("Pinned")).padding(5));
                for entry in &self.top_entries {
                    let id = entry.id.clone();
                    content = content.add(button::text(&entry.name).on_press(Message::CopyPassword(id)).width(Length::Fill));
                }
            }
            content = content.add(divider::horizontal::default());
            content = content.add(button::text("Lock").on_press(Message::LockClicked).width(Length::Fill));
        } else {
            content = content.add(button::text("Unlock").on_press(Message::SpawnApplication).width(Length::Fill));
        }

        content = content.add(button::text("Exit").on_press(Message::Exit).width(Length::Fill));

        self.core.applet.popup_container(container(content).padding(5)).into()
    }
}
