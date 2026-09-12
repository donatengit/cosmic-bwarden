pub mod menu;
pub mod search;
pub mod unlock;

use crate::app::CosmardenApp;
use crate::fl;
use crate::message::{Message, View};
use cosmic::applet::padded_control;
use cosmic::iced::Alignment;
use cosmic::widget::{column, divider, icon, text, toaster};
use cosmic::Element;

/// Dedicated branded panel icon: the repo's brand mark simplified for small
/// sizes (`icons/black_simplified.svg`'s drawable content, design-tool export
/// metadata stripped), embedded in the binary rather than looked up by name
/// in the system icon theme, so it renders correctly without any install-time
/// step (dev builds included). The panel renders this at a few dozen pixels
/// at most, where the full detailed brand mark (`FULL_ICON_SVG` below)
/// muddies into an illegible blob — the simplified glyph keeps its shapes
/// distinguishable at that size. A single monochrome source is enough — no
/// separate light/dark variant is needed, since `symbolic(true)` makes
/// libcosmic discard the SVG's own paint and recolor the whole shape to the
/// panel's foreground itself (`icon_button_from_handle` in
/// `libcosmic/src/applet/mod.rs`).
const APPLET_ICON_SVG: &[u8] = include_bytes!("../../../resources/icons/cosmarden-symbolic.svg");

/// Locked-state sibling of `APPLET_ICON_SVG`: the same glyph with the three
/// dots removed. State has to be carried by the silhouette, not by color —
/// `symbolic(true)` discards the SVG's paint entirely (see above), so a
/// different fill would render identically. Both files share a `0 0 128 128`
/// viewBox with the same outline geometry, so the panel icon keeps its exact
/// size and baseline across a lock/unlock; a variant that shifted would read
/// as a rendering glitch rather than a state change.
const APPLET_ICON_LOCKED_SVG: &[u8] =
    include_bytes!("../../../resources/icons/cosmarden-locked-symbolic.svg");

/// Size constraints for the applet popup surface, mirroring
/// `cosmic-applet-template`'s popup: a floor that keeps the popup from
/// collapsing and a ceiling that caps it on large panels. The width floor is
/// the libcosmic default (360) rather than the template's 300 — our popup
/// content is entirely `Length::Fill`-based, so a 300 floor would shrink the
/// popup 60 px narrower instead of merely allowing it to adapt.
pub fn applet_popup_limits() -> cosmic::iced::Limits {
    cosmic::iced::Limits::NONE
        .max_width(372.0)
        .min_width(360.0)
        .min_height(200.0)
        .max_height(1080.0)
}

/// Native applet section break: `padded_control(divider)` with
/// `[space_xxs, space_s]` as in cosmic-applet-power / bluetooth / battery.
fn popup_divider<'a>(space_xxs: u16, space_s: u16) -> Element<'a, Message> {
    padded_control(divider::horizontal::default())
        .padding([space_xxs, space_s])
        .into()
}

impl CosmardenApp {
    pub fn applet_view(&self) -> Element<'_, Message> {
        // `is_unlocked()` is false for Loading/Setup/Unlock alike, so the
        // locked glyph also covers startup and the no-account-yet state —
        // both are "no vault available to you right now", which is what the
        // icon is telling the user.
        let icon_bytes = if self.view.is_unlocked() {
            APPLET_ICON_SVG
        } else {
            APPLET_ICON_LOCKED_SVG
        };
        let icon_handle = icon::from_svg_bytes(icon_bytes).symbolic(true);
        let btn = self
            .core
            .applet
            .icon_button_from_handle(icon_handle)
            .on_press_with_rectangle(move |offset, bounds| {
                Message::AppletIconClicked(offset, bounds)
            });

        cosmic::Element::from(self.core.applet.applet_tooltip::<Message>(
            btn,
            fl!("app-title"),
            self.applet_popup.is_some(),
            Message::Surface,
            None,
        ))
    }

    pub fn applet_popup_content(&self) -> Element<'_, Message> {
        let spacing = cosmic::theme::active().cosmic().spacing;
        let space_xxs = spacing.space_xxs;
        let space_s = spacing.space_s;

        // Native applets (power/bluetooth/battery): column `[8, 0]` outer
        // padding, `padded_control` only on non-button content and dividers,
        // `menu_button` rows carrying their own `menu_control_padding`.
        // Wrapping search/header in `padded_control` *and* using `menu_button`
        // padding stacked a second `space_m` inset after the last icon.
        let mut content = column::with_capacity(6)
            .align_x(Alignment::Start)
            .padding([space_xxs, 0]);

        if self.protocol_mismatch {
            content = content.push(padded_control(text::body(fl!("protocol-version-mismatch"))));
            content = content.push(popup_divider(space_xxs, space_s));
            for item in menu::quit_footer() {
                content = content.push(item);
            }
            return toaster(
                &self.applet_toasts,
                self.core.applet.popup_container(content),
            );
        }

        content = content.push(menu::header_row(self));

        content = if self.view.is_unlocked() {
            content.push(search::view(self))
        } else if self.view == View::Setup {
            content.push(padded_control(text::body(fl!("not-configured"))))
        } else {
            content.push(padded_control(unlock::view(self)))
        };

        if let Some(error) = &self.applet_error {
            content = content.push(padded_control(text::body(error)));
        }

        content = content.push(popup_divider(space_xxs, space_s));
        for item in menu::quit_footer() {
            content = content.push(item);
        }

        toaster(
            &self.applet_toasts,
            self.core.applet.popup_container(content),
        )
    }
}
