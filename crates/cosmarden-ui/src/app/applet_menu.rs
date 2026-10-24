//! Pure applet-popup listing: the quit footer action.
//!
//! Kept out of the view so the mapping can be unit-tested without widgets.

use crate::fl;
use crate::message::Message;

/// The single footer action in the applet quit section (native applets
/// expose one Quit/Exit row, not a lock/logout/quit cluster).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuitAction {
    Exit,
}

impl QuitAction {
    pub fn message(self) -> Message {
        match self {
            Self::Exit => Message::Exit,
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::Exit => fl!("quit"),
        }
    }
}

/// Ordered quit-section items. Always a single Exit row, locked or not —
/// header Lock/Logout already cover stay-running session actions.
pub fn quit_actions() -> &'static [QuitAction] {
    &[QuitAction::Exit]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quit_listing_is_exit_only() {
        let items = quit_actions();
        assert_eq!(items, &[QuitAction::Exit]);
        assert_eq!(items[0].label(), fl!("quit"));
        assert_eq!(items[0].label(), "Quit");
        assert!(matches!(items[0].message(), Message::Exit));
    }
}
