use cosmic::Element;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{button, text, list_column, settings as cosmic_settings};
use crate::app::CosmicBWardenApp;
use crate::message::Message;

impl CosmicBWardenApp {
    pub fn view_settings(&self) -> Element<'_, Message> {
        let is_editing = self.editing_config.is_some();
        let config = self.editing_config.as_ref().unwrap_or(&self.config);

        let mut col = list_column();
        
        if is_editing {
            col = col.add(cosmic_settings::item("Email", 
                cosmic::widget::text_input::text_input("Email", config.email.as_deref().unwrap_or(""))
                    .on_input(Message::SettingsEmailChanged).width(Length::Fixed(300.0))));
            
            col = col.add(cosmic_settings::item("Server URL", 
                cosmic::widget::text_input::text_input("Server", config.base_url.as_deref().unwrap_or(""))
                    .on_input(Message::SettingsServerChanged).width(Length::Fixed(300.0))));
            
            col = col.add(cosmic_settings::item("Auto-lock (minutes)",
                cosmic::widget::text_input::text_input("Minutes", &self.settings_lock_timeout)
                    .on_input(Message::SettingsLockTimeoutChanged).width(Length::Fixed(300.0))));

            col = col.add(cosmic_settings::item("Top Entries Count",
                cosmic::widget::text_input::text_input("Count", &self.settings_popular_count)
                    .on_input(Message::SettingsPopularCountChanged).width(Length::Fixed(300.0))));

            col = col.add(cosmic_settings::item("Top Entries Period (days)",
                cosmic::widget::text_input::text_input("Days", &self.settings_popular_days)
                    .on_input(Message::SettingsPopularDaysChanged).width(Length::Fixed(300.0))));
        } else {
            col = col.add(cosmic_settings::item("Email", text::body(config.email.as_deref().unwrap_or("Not set"))));
            col = col.add(cosmic_settings::item("Server", text::body(config.base_url.as_deref().unwrap_or("Bitwarden Cloud"))));
            col = col.add(cosmic_settings::item("Auto-lock (minutes)", text::body(format!("{}", config.lock_timeout / 60))));
            col = col.add(cosmic_settings::item("Top Entries Count", text::body(format!("{}", config.top_popular_count))));
            col = col.add(cosmic_settings::item("Top Entries Period (days)", text::body(format!("{}", config.top_popular_days))));
        }

        let header = cosmic::widget::row::with_capacity(2).spacing(10).align_y(Alignment::Center)
            .push(text::title2("Settings").width(Length::Fill))
            .push(if is_editing {
                Element::from(cosmic::widget::row::with_capacity(2).spacing(5)
                    .push(button::suggested("Save").on_press(Message::SettingsSaveClicked))
                    .push(button::standard("Cancel").on_press(Message::SettingsCancelClicked)))
            } else {
                button::suggested("Edit").on_press(Message::SettingsEditClicked).into()
            });

        cosmic::widget::scrollable(
            cosmic::widget::column::with_capacity(2).spacing(20)
                .push(header)
                .push(col)
        ).height(Length::Fill).into()
    }
}
