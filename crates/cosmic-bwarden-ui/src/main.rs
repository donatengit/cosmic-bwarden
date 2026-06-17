mod localize;
mod app;
mod message;
mod view;

use cosmic::app::{Application, Task};
use cosmic::iced::{window, Subscription};
use clap::Parser;
use std::path::PathBuf;

use cosmic_bwarden_core::protocol::{Action as AgentAction, Response};
use cosmic_bwarden_core::agent_client::AgentClient;
use crate::app::{CosmicBWardenApp, APP_ID, AppFlags};
use crate::message::{Message, View};

#[derive(Parser, Debug)]
#[command(author, version, about = "cosmic-bwarden: Secure COSMIC Bitwarden client")]
struct Cli {
    /// Path to the configuration file. Overrides default and environment.
    #[arg(long, env = "COSMIC_BWARDEN_CONFIG")]
    config: Option<PathBuf>,

    /// Path to the Unix socket for IPC. Overrides config, default and environment.
    #[arg(long, env = "COSMIC_BWARDEN_SOCKET")]
    socket: Option<PathBuf>,
}

extern crate tracing;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RunMode {
    Applet,
    Application,
}

pub(crate) fn detect_run_mode() -> RunMode {
    // 1. Check if we are being run by the COSMIC panel (most reliable for Applets)
    if std::env::var("COSMIC_PANEL_NAME").is_ok() {
        return RunMode::Applet;
    }

    // 2. Check environment variable override
    if let Ok(mode) = std::env::var("COSMIC_BWARDEN_MODE") {
        if mode.to_lowercase() == "applet" {
            return RunMode::Applet;
        }
        if mode.to_lowercase() == "application" {
            return RunMode::Application;
        }
    }

    // 3. Check binary name (for symlink support when not run from panel)
    if let Some(exe_path) = std::env::current_exe().ok() {
        if let Some(name) = exe_path.file_name().and_then(|n| n.to_str()) {
            if name.contains("com.system76.CosmicBWarden") {
                return RunMode::Applet;
            }
        }
    }

    // 4. Fallback to Application
    RunMode::Application
}

fn setup_logs() {
    use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

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

        let mut app = CosmicBWardenApp::default();
        app.core = core;

        let tasks = vec![
            Task::perform(async {
                tracing::debug!("Connecting to agent...");
                let agent = AgentClient::new();
                match agent.send(AgentAction::GetConfig).await {
                    Ok(Response::Config { config, needs_login, has_account, is_locked }) => {
                        tracing::info!(needs_login, has_account, is_locked, "Agent config received");
                        Ok((config, needs_login, has_account, is_locked))
                    },
                    Ok(Response::Error { message }) => {
                        tracing::error!("Agent error: {}", message);
                        Err(message)
                    },
                    Ok(_) => {
                        tracing::error!("Unexpected response from agent");
                        Err("unexpected response from agent".to_string())
                    },
                    Err(e) => {
                        tracing::error!("Failed to connect to agent: {}", e);
                        Err(format!("failed to connect to agent: {}", e))
                    },
                }
            }, |res| cosmic::Action::App(Message::ConfigReceived(res))),
        ];

        if std::env::var("COSMIC_PANEL_NAME").is_err() {
            tracing::info!("Not run as applet");
            // In standalone mode, the main window will show the vault UI
        } else {
            tracing::info!("Run as applet in {}", std::env::var("COSMIC_PANEL_NAME").unwrap());
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
            use cosmic::widget::{column, header_bar, container};
            use cosmic::iced::Length;
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
                container(content)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .padding(20)
                    .into()
            };

            let window_content: cosmic::Element<Message> = if let Some(dialog) = self.view_dialogs() {
                let modal = container(dialog)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .center_x(Length::Fill)
                    .center_y(Length::Fill)
                    .class(cosmic::theme::Container::Dialog);

                cosmic::iced::widget::stack![view, modal].into()
            } else {
                view
            };

            container(column![
                header_bar().title(""),
                window_content
            ]
            .width(Length::Fill)
            .height(Length::Fill))
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
        use cosmic::iced::Subscription;
        use cosmic::cosmic_config::Update;
        use cosmic::applet::token::subscription::activation_token_subscription;
        use cosmic_bwarden_core::config::CosmicBWardenConfig;

        let config_watch = self.core
            .watch_config(cosmic_bwarden_core::config::CONFIG_ID)
            .map(|u: Update<CosmicBWardenConfig>| {
                for why in u
                    .errors
                    .into_iter()
                    .filter(cosmic::cosmic_config::Error::is_err)
                {
                    tracing::error!(?why, "config load error");
                }
                Message::ConfigChanged(u.config)
            });

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

        Subscription::batch(vec![
            config_watch,
            agent_subscription,
            token_subscription,
        ])
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

    let args = Cli::try_parse().unwrap_or_else(|_| Cli { config: None, socket: None });
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
