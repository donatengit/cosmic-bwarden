pub mod applet;
pub mod auth;
pub mod settings;
pub mod style;
pub mod vault;

use crate::app::CosmicBWardenApp;
use crate::fl;
use crate::message::{Message, View, WindowState};
use cosmic::iced::{window, Length};
use cosmic::widget::{button, container, text};
use cosmic::Element;

/// Format a duration in seconds as a compact human string (e.g. "2h", "90m").
fn format_secs(secs: u32) -> String {
    if secs == 0 {
        fl!("duration-moment")
    } else if secs.is_multiple_of(3600) {
        format!("{}h", secs / 3600)
    } else if secs >= 3600 {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    } else if secs >= 60 {
        format!("{}m", secs / 60)
    } else {
        format!("{secs}s")
    }
}

/// Convert days since the Unix epoch (1970-01-01) to a proleptic Gregorian
/// (year, month, day). Howard Hinnant's public-domain `civil_from_days`
/// algorithm (http://howardhinnant.github.io/date_algorithms.html) — avoids
/// pulling in `chrono`/`time` just to render a handful of history timestamps.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// The inverse of `civil_from_days` — days since the Unix epoch for a given
/// (year, month, day). Only needed to round-trip-test `civil_from_days`
/// itself; never used to render anything.
#[cfg(test)]
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64; // [0, 399]
    let doy = (153 * u64::from(if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + u64::from(d) - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146097 + doe as i64 - 719468
}

/// Render a Unix timestamp (seconds) as a compact, unambiguous, locale-neutral
/// "YYYY-MM-DD HH:MM" in UTC. Deliberately not converted to local time: doing
/// that correctly needs a timezone database, which would mean pulling in
/// `chrono`/`time` just for this — an accepted trade-off for a "roughly when
/// was this generated" history list, not an audit log.
pub(crate) fn format_unix_timestamp_utc(secs: u64) -> String {
    let secs = secs as i64;
    let days = secs.div_euclid(86400);
    let time_of_day = secs.rem_euclid(86400);
    let (year, month, day) = civil_from_days(days);
    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}")
}

impl CosmicBWardenApp {
    /// One-line summary of the TPM dictionary-attack lockout state for display on
    /// the PIN screens and in Settings. `None` when the TPM is unavailable or the
    /// status hasn't been fetched. Note: the counter is TPM-global (shared by all
    /// DA-protected objects), so it's phrased as a machine-wide budget.
    pub fn tpm_da_line(&self) -> Option<String> {
        let da = self.tpm_da.as_ref()?;
        if !da.available {
            return None;
        }
        if da.in_lockout {
            return Some(match da.recovery_interval_secs {
                Some(s) => fl!("tpm-lockout-wait", time = format_secs(s)),
                None => fl!("tpm-lockout"),
            });
        }
        match (da.remaining, da.max_tries) {
            (Some(rem), Some(max)) => Some(fl!("tpm-attempts-remaining", rem = rem, max = max)),
            (Some(rem), None) => Some(fl!("tpm-attempts-remaining-simple", rem = rem)),
            _ => None,
        }
    }

    /// Caption shown under the PIN input after a failed attempt: the DA
    /// attempts-remaining line when known, else a plain "Incorrect PIN".
    pub fn pin_feedback_line(&self) -> String {
        self.tpm_da_line().unwrap_or_else(|| fl!("pin-incorrect"))
    }

    pub fn view_instance(&self, id: window::Id) -> Element<'_, Message> {
        let state = self.windows.get(&id);

        match state {
            Some(WindowState::Popup) => self.applet_popup_content(),
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
        use cosmic::widget::{button, secure_input};
        if let Some(_id) = &self.show_delete_confirm {
            Some(
                cosmic::widget::dialog()
                    .title(fl!("delete-entry-title"))
                    .body(fl!("confirm-delete"))
                    .primary_action(
                        button::destructive(fl!("delete")).on_press(Message::ConfirmDelete),
                    )
                    .secondary_action(
                        button::standard(fl!("cancel")).on_press(Message::CancelDelete),
                    )
                    .width(Length::Fixed(400.0))
                    .into(),
            )
        } else if self.show_reprompt.is_some() {
            let mut col = cosmic::widget::column::with_capacity(2).spacing(10);

            let password_input = secure_input(
                fl!("master-password"),
                &self.reprompt_password,
                Some(Message::ToggleRepromptPasswordReveal),
                !self.reprompt_password_revealed,
            )
            .on_input(Message::RepromptPasswordChanged)
            .on_submit(|_| Message::SubmitReprompt)
            .width(Length::Fill);
            col = col.push(password_input);

            Some(
                cosmic::widget::dialog()
                    .title(fl!("master-password-required"))
                    .body(fl!("enter-master-password"))
                    .control(col)
                    .primary_action(
                        button::suggested(fl!("verify")).on_press(Message::SubmitReprompt),
                    )
                    .secondary_action(
                        button::standard(fl!("cancel")).on_press(Message::CancelReprompt),
                    )
                    .width(Length::Fixed(400.0))
                    .into(),
            )
        } else if self.generator_history_delete_pending.is_some() {
            Some(
                cosmic::widget::dialog()
                    .title(fl!("delete-history-entry-title"))
                    .body(fl!("confirm-delete-history-entry"))
                    .primary_action(
                        button::destructive(fl!("delete"))
                            .on_press(Message::GeneratorHistoryDeleteConfirmed),
                    )
                    .secondary_action(
                        button::standard(fl!("cancel"))
                            .on_press(Message::GeneratorHistoryDeleteCancelled),
                    )
                    .width(Length::Fixed(400.0))
                    .into(),
            )
        } else {
            None
        }
    }

    pub fn view_content(&self) -> Element<'_, Message> {
        // If protocol versions don't match, show only the error and Quit.
        if self.protocol_mismatch {
            let col = cosmic::widget::column::with_capacity(2)
                .spacing(10)
                .push(text::body(fl!("protocol-version-mismatch")))
                .push(
                    button::text(fl!("quit"))
                        .on_press(Message::Exit)
                        .width(Length::Fill),
                );
            return container(col)
                .class(cosmic::theme::Container::WindowBackground)
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(20)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into();
        }

        match self.view {
            View::Loading => cosmic::widget::text::body(fl!("loading")).into(),
            View::Setup | View::Unlock => self.view_auth(),
            View::Vault | View::Settings | View::PasswordGenerator => self.view_vault(),
        }
    }
}

#[cfg(test)]
mod date_format_tests {
    use super::*;

    #[test]
    fn epoch_is_1970_01_01() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
    }

    // Howard Hinnant's own published test vector for this exact algorithm.
    #[test]
    fn known_reference_date() {
        assert_eq!(days_from_civil(2000, 3, 1), 11017);
        assert_eq!(civil_from_days(11017), (2000, 3, 1));
    }

    #[test]
    fn round_trips_across_a_leap_year_boundary() {
        for day in -400..400 {
            let (y, m, d) = civil_from_days(day);
            assert_eq!(
                days_from_civil(y, m, d),
                day,
                "round-trip failed for day {day}"
            );
        }
    }

    #[test]
    fn leap_day_2000_is_valid_and_next_day_is_march_1st() {
        let leap_day = days_from_civil(2000, 2, 29);
        assert_eq!(civil_from_days(leap_day), (2000, 2, 29));
        assert_eq!(civil_from_days(leap_day + 1), (2000, 3, 1));
    }

    // 1900 is NOT a leap year (divisible by 100 but not 400) — the classic
    // Gregorian edge case this algorithm must get right.
    #[test]
    fn year_1900_is_not_a_leap_year() {
        let feb_28_1900 = days_from_civil(1900, 2, 28);
        assert_eq!(civil_from_days(feb_28_1900 + 1), (1900, 3, 1));
    }

    #[test]
    fn formats_a_known_timestamp() {
        // 2000-03-01 00:00:00 UTC = day 11017 * 86400 seconds.
        let secs = 11017u64 * 86400;
        assert_eq!(format_unix_timestamp_utc(secs), "2000-03-01 00:00");
        assert_eq!(
            format_unix_timestamp_utc(secs + 3600 + 120),
            "2000-03-01 01:02"
        );
    }
}
