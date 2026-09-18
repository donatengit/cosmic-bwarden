//! iced/libcosmic `Application` impl and the process entry points shared by
//! the `cosmarden-applet` binary.

use crate::app::{AppFlags, CosmardenApp, APP_ID};
use crate::message::{Message, View};
use crate::{detect_run_mode, RunMode};
use clap::Parser;
use cosmarden_core::agent_client::AgentClient;
use cosmarden_core::protocol::{Action as AgentAction, Response};
use cosmic::app::{Application, Task};
use cosmic::iced::{window, Length, Subscription};
use cosmic::widget::container;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    author,
    version = cosmarden_core::version(),
    about = "Bitwarden-compatible password manager for COSMIC DE",
    after_help = cosmarden_core::help_footer()
)]
struct Cli {
    /// Path to the configuration file. Overrides default and environment.
    #[arg(long, env = "COSMARDEN_CONFIG")]
    config: Option<PathBuf>,

    /// Path to the Unix socket for IPC. Overrides config, default and environment.
    #[arg(long, env = "COSMARDEN_SOCKET")]
    socket: Option<PathBuf>,
}

pub fn parse_cli_flags() -> AppFlags {
    let args = Cli::try_parse().unwrap_or(Cli {
        config: None,
        socket: None,
    });
    AppFlags {
        config: args.config,
        socket: args.socket,
    }
}

impl Application for CosmardenApp {
    type Executor = cosmic::SingleThreadExecutor;
    type Message = Message;
    type Flags = AppFlags;
    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &cosmic::app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::app::Core {
        &mut self.core
    }

    fn init(core: cosmic::app::Core, flags: Self::Flags) -> (Self, Task<Self::Message>) {
        tracing::info!("Initializing CosmardenApp");

        if let Some(config_path) = &flags.config {
            cosmarden_core::dirs::set_config_override(config_path.clone());
        }
        if let Some(socket_path) = &flags.socket {
            cosmarden_core::dirs::set_socket_override(socket_path.clone());
        }

        // Load configuration to check for additional overrides if flags didn't set socket
        if flags.socket.is_none() && std::env::var("COSMARDEN_SOCKET").is_err() {
            if let Ok(config) = cosmarden_core::config::CosmardenConfig::load_legacy() {
                if let Some(path) = config.socket_path {
                    cosmarden_core::dirs::set_socket_override(std::path::PathBuf::from(path));
                }
            }
        }

        let app = CosmardenApp {
            core,
            ..Default::default()
        };

        let tasks = vec![
            crate::app::tasks::check_protocol_version(),
            Task::perform(
                async {
                    tracing::debug!("Connecting to agent...");
                    let agent = AgentClient::new();
                    match agent.send(AgentAction::GetConfig).await {
                        Ok(Response::Config {
                            config,
                            needs_login,
                            has_account,
                            is_locked,
                            sync_failed,
                            session_id,
                            lock_epoch,
                        }) => {
                            tracing::info!(
                                needs_login,
                                has_account,
                                is_locked,
                                sync_failed,
                                "Agent config received"
                            );
                            Ok((
                                config,
                                needs_login,
                                has_account,
                                is_locked,
                                sync_failed,
                                session_id,
                                lock_epoch,
                            ))
                        }
                        Ok(Response::Error { message }) => {
                            tracing::error!("Agent error: {}", message);
                            Err(message)
                        }
                        Ok(_) => {
                            tracing::error!("Unexpected response from agent");
                            Err("unexpected response from agent".to_string())
                        }
                        Err(e) => {
                            tracing::error!("Failed to connect to agent: {}", e);
                            Err(format!("failed to connect to agent: {}", e))
                        }
                    }
                },
                |res| cosmic::Action::App(Message::ConfigReceived(res)),
            ),
        ];

        if std::env::var("COSMIC_PANEL_NAME").is_err() {
            tracing::info!("Not run as applet");
            // In standalone mode, the main window will show the vault UI
        } else {
            tracing::info!(
                "Run as applet in {}",
                std::env::var("COSMIC_PANEL_NAME").unwrap()
            );
        }

        (app, Task::batch(tasks))
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Self::Message> {
        Some(Message::WindowClosed(id))
    }

    fn on_window_resize(&mut self, id: window::Id, width: f32, _height: f32) {
        // Only the standalone vault window drives the sidebar split; ignore
        // applet surfaces and tracked popups (also in `windows`).
        if !self.windows.contains_key(&id) && detect_run_mode() != RunMode::Applet {
            self.vault_window_resized(width);
        }
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        self.update_app(message)
    }

    fn view(&self) -> cosmic::Element<'_, Self::Message> {
        match detect_run_mode() {
            RunMode::Applet => self.applet_view(),
            RunMode::Application => application_view(self),
        }
    }

    fn view_window(&self, id: window::Id) -> cosmic::Element<'_, Self::Message> {
        self.view_instance(id)
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        Subscription::batch(vec![agent_subscription(), token_subscription()])
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        if detect_run_mode() == RunMode::Applet {
            Some(cosmic::applet::style())
        } else {
            None
        }
    }
}

/// The vault window body: `view_content()` plus the dialog overlay, in the
/// window-background container.
fn application_view(app: &CosmardenApp) -> cosmic::Element<'_, Message> {
    let content = app.view_content();
    let is_auth = matches!(app.view, View::Setup | View::Unlock);

    let view: cosmic::Element<Message> = if is_auth {
        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(20)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
    } else {
        // Vault view: no header bar, sidebar/detail handle their own spacing.
        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    };

    let window_content: cosmic::Element<Message> = if let Some(dialog) = app.view_dialogs() {
        let modal = container(dialog)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .class(cosmic::theme::Container::Dialog(true));

        cosmic::iced::widget::stack![view, modal].into()
    } else {
        view
    };

    container(window_content)
        .class(cosmic::theme::Container::WindowBackground)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn agent_subscription() -> Subscription<Message> {
    Subscription::run(|| {
        use cosmarden_core::protocol::{Action, Response};

        async_stream::try_stream! {
            let mut socket = tokio::net::UnixStream::connect(cosmarden_core::dirs::socket_file()).await
                .map_err(|e| anyhow::anyhow!(e))?;

            // Send Subscribe action
            let request = Action::Subscribe;
            let request_bytes = postcard::to_allocvec(&request)
                .map_err(|e| anyhow::anyhow!(e))?;
            use tokio::io::AsyncWriteExt;
            let len = request_bytes.len() as u32;
            socket.write_all(&len.to_le_bytes()).await
                .map_err(|e| anyhow::anyhow!(e))?;
            socket.write_all(&request_bytes).await
                .map_err(|e| anyhow::anyhow!(e))?;

            // Read Response
            use tokio::io::AsyncReadExt;
            loop {
                let mut len_buf = [0u8; 4];
                socket.read_exact(&mut len_buf).await
                    .map_err(|e| anyhow::anyhow!(e))?;
                let len = u32::from_le_bytes(len_buf) as usize;
                let mut buf = vec![0u8; len];
                socket.read_exact(&mut buf).await
                    .map_err(|e| anyhow::anyhow!(e))?;

                let response: Response = postcard::from_bytes(&buf)
                    .map_err(|e| anyhow::anyhow!(e))?;

                match response {
                    Response::Ack => continue,
                    Response::Event { event } => yield event,
                    _ => {}
                }
            }
        }
    })
    .map(
        |res: Result<cosmarden_core::protocol::Event, anyhow::Error>| match res {
            Ok(event) => Message::EventReceived(event),
            Err(e) => {
                tracing::error!("agent subscription error: {}", e);
                Message::RefreshStateInternal // Fallback to refresh if streaming fails
            }
        },
    )
}

/// Activation tokens exist only under a COSMIC panel; the vault window never
/// receives one.
fn token_subscription() -> Subscription<Message> {
    use cosmic::applet::token::subscription::activation_token_subscription;

    if detect_run_mode() == RunMode::Applet {
        activation_token_subscription(0).map(Message::Token)
    } else {
        Subscription::none()
    }
}

pub fn run_applet(flags: AppFlags) -> cosmic::iced::Result {
    tracing::info!("Running in Applet mode");
    cosmic::applet::run::<CosmardenApp>(flags)
}

pub fn run_application(flags: AppFlags) -> cosmic::iced::Result {
    tracing::info!("Running in Application mode");
    let settings = cosmic::app::Settings::default()
        .no_main_window(false)
        .exit_on_close(true)
        .default_mmap_threshold(Some(131072));

    cosmic::app::run_single_instance::<CosmardenApp>(settings, flags)
}
