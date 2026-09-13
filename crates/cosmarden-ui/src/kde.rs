//! KDE/Plasma extras. Compiled only with `--features kde`.
//!
//! Owns: theme-connection log demotion, StatusNotifierItem tray, and the
//! compact popup that reuses `view/applet/` (search, copy, PIN/password
//! `secure_input`). Default COSMIC builds must not include this module.

mod icon;
pub(crate) mod menu;
mod theme;
mod tray;

use crate::app::applet_search;
use crate::app::state::CosmardenApp;
use crate::app::tasks::applet_rows_task;
use crate::fl;
use crate::message::{Message, View, WindowState};
use cosmic::app::Task;
use cosmic::iced::window::{self, Position};
use cosmic::iced::{Size, Subscription};
use cosmic::Action;
use cosmic::ApplicationExt;
use menu::TrayCommand;
use tracing_subscriber::EnvFilter;
use tray::TraySnapshot;

pub use menu::TrayCommand as KdeTrayCommand;

/// Tracing directives that demote libcosmic's missing-session theme
/// errors. They must be `=off`: those crates log at `ERROR`, so `=warn`
/// would still print them.
pub const THEME_NOISE_DIRECTIVES: &[&str] = &[
    "cosmic::theme=off",
    "cosmic::app::cosmic=off",
    "cosmic_config=off",
];

pub fn apply_theme_noise_filter(mut filter: EnvFilter) -> EnvFilter {
    for directive in THEME_NOISE_DIRECTIVES {
        filter = filter.add_directive(directive.parse().expect("static directive"));
    }
    filter
}

pub fn subscription() -> Subscription<Message> {
    Subscription::run(|| {
        async_stream::stream! {
            let mut rx = tray::command_receiver();
            while let Some(cmd) = rx.recv().await {
                yield Message::KdeTray(cmd);
            }
        }
    })
}

pub fn publish_snapshot(app: &CosmardenApp) {
    tray::publish_snapshot(TraySnapshot {
        unlocked: app.view.is_unlocked(),
        setup: app.view == View::Setup,
        protocol_mismatch: app.protocol_mismatch,
        rows: applet_search::build_applet_rows(&app.applet_search_results),
    });
}

pub fn handle_tray_command(app: &mut CosmardenApp, cmd: TrayCommand) -> Task<Message> {
    match cmd {
        TrayCommand::TogglePopup => toggle_popup(app),
        other => Task::done(Action::App(menu::command_to_message(other))),
    }
}

fn toggle_popup(app: &mut CosmardenApp) -> Task<Message> {
    if let Some(id) = app.applet_popup.take() {
        app.windows.remove(&id);
        return window::close(id);
    }
    // Same reset and focus as the COSMIC popup: a typed master password must
    // not outlive the window, and the unlock field has to be ready to type in.
    app.reset_applet_popup_state();
    let focus = app.applet_popup_focus_task();
    let rows = applet_rows_task(app);
    let settings = window::Settings {
        size: Size::new(372.0, 480.0),
        position: Position::Centered,
        resizable: true,
        decorations: true,
        exit_on_close_request: true,
        min_size: Some(Size::new(360.0, 200.0)),
        level: window::Level::AlwaysOnTop,
        ..Default::default()
    };
    let (id, open) = window::open(settings);
    app.applet_popup = Some(id);
    app.windows.insert(id, WindowState::Popup);
    let title = fl!("app-title");
    Task::batch([
        open.map(|_| Action::None),
        app.set_window_title(title, id),
        crate::app::tasks::check_protocol_version(),
        focus,
        rows,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_noise_directives_parse_as_env_filter_layers() {
        let mut filter = EnvFilter::new("warn");
        for directive in THEME_NOISE_DIRECTIVES {
            filter = filter.add_directive(
                directive
                    .parse()
                    .unwrap_or_else(|e| panic!("{directive:?} must parse: {e}")),
            );
        }
        let _ = apply_theme_noise_filter(EnvFilter::new("warn"));
        assert_eq!(THEME_NOISE_DIRECTIVES.len(), 3);
        for directive in THEME_NOISE_DIRECTIVES {
            assert!(
                directive.ends_with("=off"),
                "{directive} must be =off: libcosmic logs these at ERROR"
            );
        }
    }
}
