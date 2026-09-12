//! Pure helpers behind the applet popup: the reopen-debounce decision, the
//! anchor-rect clamp, and which unlock field takes focus. Split out of
//! `app/update/applet.rs`, which had grown past the 500-line hard limit; the
//! items are re-exported from `super` so `app/tests/applet.rs` keeps importing
//! them from `crate::app::update::applet::{…}`.

/// Debounce window for the applet popup reopen race, in milliseconds. After an
/// icon click closes the popup, a second click within this window is treated as
/// part of the same physical press (Wayland can deliver a close and then a
/// follow-up event that sees the popup gone and would re-open it) and is
/// suppressed. Mirrors `cosmic-ext-applet-external-monitor-brightness`'s
/// 200 ms `last_quit` gate.
pub(crate) const APPLET_POPUP_REOPEN_DEBOUNCE_MS: u128 = 200;

/// True when a popup reopen should be suppressed because the popup was just
/// closed within [`APPLET_POPUP_REOPEN_DEBOUNCE_MS`]. Pure so the toggle
/// race logic is unit-testable without a live executor.
pub(crate) fn should_suppress_popup_reopen(
    last_closed_at: Option<std::time::Instant>,
    now: std::time::Instant,
) -> bool {
    last_closed_at
        .map(|t| now.duration_since(t).as_millis() < APPLET_POPUP_REOPEN_DEBOUNCE_MS)
        .unwrap_or(false)
}

/// Compute the popup's `anchor_rect` from the icon-click `(offset, bounds)` and
/// clamp every side to at least 1px (as `cosmic-applet-time` does). A 0-width or
/// negative anchor from degenerate icon bounds would produce a zero-sized
/// positioning rect and a misplaced/never-mapped popup. `positioner.anchor_rect`
/// is `Rectangle<i32>`; `bounds`/`offset` are in logical floats. Pure so the
/// clamp is unit-testable without a live executor/popup surface.
pub(crate) fn clamped_anchor_rect(
    offset: cosmic::iced::Vector,
    bounds: cosmic::iced::Rectangle,
) -> cosmic::iced::Rectangle<i32> {
    cosmic::iced::Rectangle {
        x: (bounds.x - offset.x).max(1.0) as i32,
        y: (bounds.y - offset.y).max(1.0) as i32,
        width: bounds.width.max(1.0) as i32,
        height: bounds.height.max(1.0) as i32,
    }
}

/// Which unlock field the applet popup should autofocus: the PIN field when
/// TPM PIN unlock is active, the master password field otherwise. Pulled out
/// as a pure function so the decision is unit-testable independent of the
/// opaque `Task` returned by `text_input::focus`.
pub(crate) fn unlock_focus_id(mode: crate::message::UnlockMode) -> cosmic::widget::Id {
    if mode == crate::message::UnlockMode::Pin {
        crate::view::applet::unlock::pin_input_id()
    } else {
        crate::view::applet::unlock::password_input_id()
    }
}
