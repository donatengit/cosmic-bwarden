//! The StatusNotifierItem: its menu, its icon, and the process entry point.
//!
//! The loop that reads the agent, runs menu commands and follows the
//! subscription lives in `supervisor.rs`.

use crate::icon;
use crate::menu::{self, MenuCommand, MenuNode};
use crate::state::{self, TrayState};
use crate::supervisor::{self, Signal, Supervisor};
use crate::theme;
use cosmarden_core::agent_client::AgentClient;
use ksni::menu::{MenuItem, StandardItem, SubMenu};
use ksni::TrayMethods as _;
use tokio::sync::mpsc;

/// SNI item id: what the panel keys this item's position and pin state on, so
/// it is a stable literal rather than anything derived.
const ITEM_ID: &str = "cosmarden-plasma-tray";

/// Display name (the SNI tooltip), per the naming table: `Cosmarden`.
const ITEM_TITLE: &str = "Cosmarden";

pub struct PlasmaTray {
    commands: mpsc::UnboundedSender<MenuCommand>,
    state: TrayState,
    /// Dark panel ⇒ light pixmap. Plasma does not recolor a named symbolic
    /// icon, so the contrast has to be baked into the pixmap.
    dark_panel: bool,
}

impl ksni::Tray for PlasmaTray {
    /// Left click opens the menu instead of activating the item: this binary
    /// has no popup, so there is nothing else a click could do.
    ///
    /// `ItemIsMenu` makes Plasma ≥ 6.4 skip `Activate` entirely. ksni answers
    /// `UnknownMethod` to `Activate` for the hosts that still call it (GNOME
    /// shell's appindicator extension, Plasma < 6.4), which makes those show
    /// the menu as well. Verified in ksni 0.3.6 `dbus_interface.rs`.
    const MENU_ON_ACTIVATE: bool = true;

    fn id(&self) -> String {
        ITEM_ID.to_string()
    }

    fn title(&self) -> String {
        ITEM_TITLE.to_string()
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::ApplicationStatus
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![icon::sni_icon(self.dark_panel, self.state.locked)]
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        menu::build_menu(&self.state)
            .into_iter()
            .map(node_to_ksni)
            .collect()
    }
}

fn node_to_ksni(node: MenuNode) -> MenuItem<PlasmaTray> {
    match node {
        MenuNode::Separator => MenuItem::Separator,
        MenuNode::SubMenu { label, children } => SubMenu {
            label,
            submenu: children.into_iter().map(node_to_ksni).collect(),
            ..Default::default()
        }
        .into(),
        MenuNode::Item { label, command } => StandardItem {
            label,
            activate: Box::new(move |tray: &mut PlasmaTray| {
                // The host calls this on its own thread; hand the command to
                // the supervisor instead of doing agent work here.
                let _ = tray.commands.send(command.clone());
            }),
            ..Default::default()
        }
        .into(),
    }
}
/// Register the tray and run until `Exit` is chosen.
pub async fn run() -> anyhow::Result<()> {
    let (commands, command_rx) = mpsc::unbounded_channel();
    let dark_panel = theme::panel_is_dark().await;
    // Logged before registration: this is a startup decision (portal, then
    // kdeglobals, then GTK, then dark), and it is also the only way to see it
    // when registration itself fails.
    tracing::info!(dark_panel, "panel contrast resolved");
    let tray = PlasmaTray {
        commands,
        state: TrayState::default(),
        dark_panel,
    };
    let handle = tray.spawn().await?;
    tracing::info!("StatusNotifierItem registered");

    // Follow the KDE theme: a light/dark switch redraws the icon instead of
    // waiting for a tray restart. The watcher ends by itself when the portal
    // goes away, leaving the startup glyph in place.
    let theme_handle = handle.clone();
    tokio::spawn(async move {
        theme::watch_panel_dark(move |dark| {
            let handle = theme_handle.clone();
            tokio::spawn(async move {
                handle.update(|tray| tray.dark_panel = dark).await;
            });
        })
        .await;
    });

    let client = AgentClient::new();
    let mut supervisor = Supervisor::new(command_rx, cosmarden_core::dirs::socket_file());
    // `None` until the first probe: the opening state is logged even when
    // nothing changes afterwards, so a tray that starts with no agent says so.
    let mut state: Option<TrayState> = None;

    loop {
        let next = state::probe(&client).await;
        if state.as_ref() != Some(&next) {
            supervisor::log_state(state.as_ref(), &next);
            let snapshot = next.clone();
            state = Some(next);
            handle.update(move |tray| tray.state = snapshot).await;
        }

        match supervisor.next_signal().await {
            Signal::Exit => break,
            Signal::Command(command) => {
                supervisor::run_command(command, &client, state.as_ref()).await
            }
            Signal::Event(event) => tracing::debug!(?event, "agent event"),
            Signal::Tick => {}
        }
    }

    tracing::info!("exiting");
    // Dropping the handle unregisters the item, so the icon leaves the panel.
    drop(handle);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ksni::Tray as _;

    /// The icon follows the lock state: a locked vault draws the locked glyph
    /// (`ksni::Icon` has no `PartialEq`, so compare the decoded pixels and
    /// dimensions).
    #[test]
    fn icon_state_tracks_the_vault() {
        let tray = PlasmaTray {
            commands: mpsc::unbounded_channel().0,
            state: TrayState {
                agent_ready: true,
                has_account: true,
                locked: true,
                rows: vec![],
                protocol_mismatch: false,
            },
            dark_panel: true,
        };
        let pixmap = tray.icon_pixmap();
        assert_eq!(pixmap.len(), 1, "one icon");
        let locked_icon = icon::sni_icon(true, true);
        assert_eq!(pixmap[0].width, locked_icon.width);
        assert_eq!(pixmap[0].height, locked_icon.height);
        assert_eq!(
            pixmap[0].data, locked_icon.data,
            "a locked vault must use the locked pixmap"
        );

        // Unlocked swaps to the open glyph: the difference is the padlock.
        let mut tray = tray;
        tray.state.locked = false;
        assert_ne!(tray.icon_pixmap()[0].data, locked_icon.data);

        assert_eq!(tray.menu().len(), menu::build_menu(&tray.state).len());
    }
}
