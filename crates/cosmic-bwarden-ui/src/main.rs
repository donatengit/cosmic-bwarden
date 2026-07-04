mod app;
mod localize;
mod message;
mod view;

use clap::Parser;
use cosmic::app::{Application, Task};
use cosmic::iced::{window, Subscription};
use std::path::PathBuf;

use crate::app::{AppFlags, CosmicBWardenApp, APP_ID};
use crate::message::{Message, View};
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action as AgentAction, Response};

#[derive(Parser, Debug)]
#[command(author, version = cosmic_bwarden_core::version(), about = "cosmic-bwarden: Secure COSMIC Bitwarden client")]
struct Cli {
    /// Path to the configuration file. Overrides default and environment.
    #[arg(long, env = "COSMIC_BWARDEN_CONFIG")]
    config: Option<PathBuf>,

    /// Path to the Unix socket for IPC. Overrides config, default and environment.
    #[arg(long, env = "COSMIC_BWARDEN_SOCKET")]
    socket: Option<PathBuf>,
}

extern crate tracing;

/// Minimum PIN length for TPM-backed unlock; single source in core. Used in
/// input captions and submit validation; the agent enforces it
/// authoritatively.
pub(crate) const MIN_PIN_LEN: usize = cosmic_bwarden_core::MIN_PIN_LEN;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RunMode {
    Applet,
    Application,
}

pub(crate) fn detect_run_mode() -> RunMode {
    // The COSMIC panel sets this env var when launching an applet.
    // Running the binary directly (outside the panel) falls through to Application mode.
    if std::env::var("COSMIC_PANEL_NAME").is_ok() {
        RunMode::Applet
    } else {
        RunMode::Application
    }
}

fn setup_logs() {
    use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

    let fmt_layer = fmt::layer().with_target(false);
    let filter_layer = EnvFilter::try_from_default_env().unwrap_or(EnvFilter::new(format!(
        "warn,{}=info",
        env!("CARGO_CRATE_NAME")
    )));

    if let Ok(journal_layer) = tracing_journald::layer() {
        tracing_subscriber::registry()
            .with(filter_layer)
            .with(fmt_layer)
            .with(journal_layer)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter_layer)
            .with(fmt_layer)
            .init();
    }
}

impl Application for CosmicBWardenApp {
    type Executor = cosmic::SingleThreadExecutor;
    type Message = Message;
    type Flags = crate::app::AppFlags;
    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &cosmic::app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::app::Core {
        &mut self.core
    }

    fn init(core: cosmic::app::Core, flags: Self::Flags) -> (Self, Task<Self::Message>) {
        tracing::info!("Initializing CosmicBWardenApp");

        if let Some(config_path) = &flags.config {
            cosmic_bwarden_core::dirs::set_config_override(config_path.clone());
        }
        if let Some(socket_path) = &flags.socket {
            cosmic_bwarden_core::dirs::set_socket_override(socket_path.clone());
        }

        // Load configuration to check for additional overrides if flags didn't set socket
        if flags.socket.is_none() && std::env::var("COSMIC_BWARDEN_SOCKET").is_err() {
            if let Ok(config) = cosmic_bwarden_core::config::CosmicBWardenConfig::load_legacy() {
                if let Some(path) = config.socket_path {
                    cosmic_bwarden_core::dirs::set_socket_override(std::path::PathBuf::from(path));
                }
            }
        }

        let app = CosmicBWardenApp {
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
                        }) => {
                            tracing::info!(
                                needs_login,
                                has_account,
                                is_locked,
                                sync_failed,
                                "Agent config received"
                            );
                            Ok((config, needs_login, has_account, is_locked, sync_failed))
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

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        self.update_app(message)
    }

    fn view(&self) -> cosmic::Element<'_, Self::Message> {
        let mode = detect_run_mode();
        if mode == RunMode::Applet {
            self.applet_view()
        } else {
            // Standard window view
            use cosmic::iced::Length;
            use cosmic::widget::container;
            let content = self.view_content();
            let is_auth = matches!(self.view, View::Setup | View::Unlock);

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

            let window_content: cosmic::Element<Message> = if let Some(dialog) = self.view_dialogs()
            {
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
    }

    fn view_window(&self, id: window::Id) -> cosmic::Element<'_, Self::Message> {
        self.view_instance(id)
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        use cosmic::applet::token::subscription::activation_token_subscription;
        use cosmic::iced::Subscription;

        let agent_subscription = Subscription::run(|| {
            use cosmic_bwarden_core::protocol::{Action, Response};

            async_stream::try_stream! {
                let mut socket = tokio::net::UnixStream::connect(cosmic_bwarden_core::dirs::socket_file()).await
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
        }).map(|res: Result<cosmic_bwarden_core::protocol::Event, anyhow::Error>| {
            match res {
                Ok(event) => Message::EventReceived(event),
                Err(e) => {
                    tracing::error!("agent subscription error: {}", e);
                    Message::RefreshStateInternal // Fallback to refresh if streaming fails
                }
            }
        });

        let token_subscription = if detect_run_mode() == RunMode::Applet {
            activation_token_subscription(0).map(Message::Token)
        } else {
            Subscription::none()
        };

        Subscription::batch(vec![agent_subscription, token_subscription])
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        let mode = detect_run_mode();
        if mode == RunMode::Applet {
            Some(cosmic::applet::style())
        } else {
            None
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    setup_logs();

    let args = Cli::try_parse().unwrap_or(Cli {
        config: None,
        socket: None,
    });
    let flags = AppFlags {
        config: args.config,
        socket: args.socket,
    };

    let mode = detect_run_mode();
    tracing::info!(?mode, "Starting CosmicBWarden UI");
    localize::localize();

    match mode {
        RunMode::Applet => run_applet(flags).map_err(|e| e.into()),
        RunMode::Application => run_application(flags).map_err(|e| e.into()),
    }
}

fn run_applet(flags: AppFlags) -> cosmic::iced::Result {
    tracing::info!("Running in Applet mode");
    cosmic::applet::run::<CosmicBWardenApp>(flags)
}

fn run_application(flags: AppFlags) -> cosmic::iced::Result {
    tracing::info!("Running in Application mode");
    let settings = cosmic::app::Settings::default()
        .no_main_window(false)
        .exit_on_close(true)
        .default_mmap_threshold(Some(131072));

    cosmic::app::run_single_instance::<CosmicBWardenApp>(settings, flags)
}
