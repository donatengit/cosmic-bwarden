//! StatusNotifierItem host (ksni). Menu structure comes from [`super::menu`].

use super::icon;
use super::menu::{self, MenuNode, TrayCommand};
use super::theme;
use crate::app::applet_search::AppletRow;
use crate::app::state::APP_ID;
use crate::fl;
use ksni::menu::{StandardItem, SubMenu};
use ksni::Handle;
use std::sync::OnceLock;
use tokio::sync::mpsc;

#[derive(Clone, Debug, Default)]
pub struct TraySnapshot {
    pub unlocked: bool,
    pub setup: bool,
    pub protocol_mismatch: bool,
    pub rows: Vec<AppletRow>,
}

pub struct CosmardenTray {
    tx: mpsc::UnboundedSender<TrayCommand>,
    snapshot: TraySnapshot,
    /// True when the Plasma panel is dark — the pixmap must then be light.
    dark_panel: bool,
}

static HANDLE: OnceLock<Handle<CosmardenTray>> = OnceLock::new();

pub fn publish_snapshot(snapshot: TraySnapshot) {
    let Some(handle) = HANDLE.get() else {
        return;
    };
    tokio::spawn(async move {
        handle.update(|tray| tray.snapshot = snapshot).await;
    });
}

/// Spawn the SNI item and yield [`TrayCommand`]s into an iced subscription.
pub fn command_receiver() -> mpsc::UnboundedReceiver<TrayCommand> {
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let dark_panel = theme::panel_is_dark().await;
        tracing::info!(dark_panel, "KDE tray panel contrast");
        let tray = CosmardenTray {
            tx: tx.clone(),
            snapshot: TraySnapshot::default(),
            dark_panel,
        };
        use ksni::TrayMethods;
        match tray.spawn().await {
            Ok(handle) => {
                let _ = HANDLE.set(handle.clone());
                theme::watch_panel_dark(move |dark| {
                    let handle = handle.clone();
                    tokio::spawn(async move {
                        handle.update(|t| t.dark_panel = dark).await;
                    });
                })
                .await;
            }
            Err(e) => tracing::error!("StatusNotifierItem failed to start: {e}"),
        }
    });
    rx
}

impl ksni::Tray for CosmardenTray {
    fn id(&self) -> String {
        APP_ID.to_string()
    }

    fn title(&self) -> String {
        fl!("app-title")
    }

    fn icon_name(&self) -> String {
        // Empty: Plasma SNI does not recolor a named symbolic icon. Contrast
        // comes from [`icon_pixmap`].
        String::new()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![icon::sni_icon(self.dark_panel, !self.snapshot.unlocked)]
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::ApplicationStatus
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.tx.send(TrayCommand::TogglePopup);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        menu::build_menu(
            self.snapshot.protocol_mismatch,
            self.snapshot.setup,
            self.snapshot.unlocked,
            &self.snapshot.rows,
        )
        .into_iter()
        .map(node_to_ksni)
        .collect()
    }
}

fn node_to_ksni(node: MenuNode) -> ksni::MenuItem<CosmardenTray> {
    match node {
        MenuNode::Separator => ksni::MenuItem::Separator,
        MenuNode::SubMenu { label, children } => SubMenu {
            label,
            submenu: children.into_iter().map(node_to_ksni).collect(),
            ..Default::default()
        }
        .into(),
        MenuNode::Item {
            label,
            command,
            enabled,
        } => StandardItem {
            label,
            enabled,
            activate: Box::new(move |tray: &mut CosmardenTray| {
                let _ = tray.tx.send(command.clone());
            }),
            ..Default::default()
        }
        .into(),
    }
}
