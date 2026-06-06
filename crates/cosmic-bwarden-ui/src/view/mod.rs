pub mod auth;
pub mod vault;
pub mod settings;
pub mod applet;

use cosmic::Element;
use cosmic::iced::window;
use cosmic::widget::container;
use cosmic::iced::Length;
use crate::app::CosmicBWardenApp;
use crate::message::{Message, View, WindowState};
use crate::fl;

impl CosmicBWardenApp {
    pub fn view_window(&self, id: window::Id) -> Element<'_, Message> {
        let state = self.windows.get(&id);
        
        match state {
            Some(WindowState::Popup) => {
                self.applet_popup_content()
            }
            Some(WindowState::Auth) => {
                use cosmic::widget::{column, header_bar};
                let content = self.view_auth();
                let title = if self.view == View::Setup { "Login" } else { "Unlock Vault" };
                
                container(column![
                    header_bar().title(title),
                    container(content)
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .center_x(Length::Fill)
                        .center_y(Length::Fill)
                ]
                .width(Length::Fill)
                .height(Length::Fill))
                .class(cosmic::theme::Container::WindowBackground)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
            }
            Some(WindowState::Main) => {
                use cosmic::widget::{column, header_bar};
                let content = self.view_content();

                let view = container(content)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .padding(20);

                let window_content: Element<Message> = if let Some(dialog) = self.view_dialogs() {
                    let modal = container(dialog)
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .center_x(Length::Fill)
                        .center_y(Length::Fill)
                        .class(cosmic::theme::Container::Dialog);
                    
                    cosmic::iced::widget::stack![view, modal].into()
                } else {
                    view.into()
                };

                container(column![
                    header_bar().title(fl!("app-title")),
                    window_content
                ])
                .class(cosmic::theme::Container::WindowBackground)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
            }
            None => {
                // Check if this is the panel providing a surface
                if std::env::var("COSMIC_PANEL_NAME").is_ok() {
                    self.applet_view()
                } else {
                    // standalone mode fallback
                    let content = self.view_content();
                    container(content)
                        .class(cosmic::theme::Container::WindowBackground)
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .padding(20)
                        .into()
                }
            }
        }
    }

    pub fn view_dialogs(&self) -> Option<Element<'_, Message>> {
        use cosmic::widget::button;
        if let Some(_id) = &self.show_delete_confirm {
            Some(cosmic::widget::dialog()
                .title("Delete Entry?")
                .body("Are you sure you want to delete this entry? This action cannot be undone.")
                .primary_action(button::destructive("Delete").on_press(Message::ConfirmDelete))
                .secondary_action(button::standard("Cancel").on_press(Message::CancelDelete))
                .width(Length::Fixed(400.0))
                .into())
        } else if let Some(_) = &self.show_reprompt {
            let mut col = cosmic::widget::column::with_capacity(2).spacing(10);
            
            let mut password_input = cosmic::widget::text_input::text_input("Master Password", &self.reprompt_password)
                .on_input(Message::RepromptPasswordChanged)
                .on_submit(|_| Message::SubmitReprompt)
                .width(Length::Fill);
            if !self.master_password_revealed {
                password_input = password_input.password();
            }
            col = col.push(password_input);

            Some(cosmic::widget::dialog()
                .title("Master Password Required")
                .body("Please enter your master password to view this sensitive entry.")
                .control(col)
                .primary_action(button::suggested("Verify").on_press(Message::SubmitReprompt))
                .secondary_action(button::standard("Cancel").on_press(Message::CancelReprompt))
                .width(Length::Fixed(400.0))
                .into())
        } else {
            None
        }
    }

    pub fn view_content(&self) -> Element<'_, Message> {
        match self.view {
            View::Loading => cosmic::widget::text::body(fl!("loading")).into(),
            View::Setup | View::Unlock => {
                self.view_auth()
            }
            View::Vault | View::Settings => self.view_vault(),
        }
    }
}
