use std::{ops::RangeInclusive, time::Duration};

use gpui_kit::{
    component::input::{Input, InputState},
    prelude::*,
    *,
};

use super::{
    PreferenceDraft, Preferences,
    view::{preference_action_row, preference_row, unit_suffix},
};
use crate::{
    app::{
        Client, Message,
        system::open_text_file,
        theme::palette,
        widgets::{Tone, button},
    },
    i18n::tr,
};

pub(super) const DEFAULT_AUTO_FETCH_MINUTES: u64 = 1;
const AUTO_FETCH_RANGE: RangeInclusive<u64> = 0..=60;

pub(super) fn valid_auto_fetch_minutes(minutes: u64) -> Option<u64> {
    AUTO_FETCH_RANGE.contains(&minutes).then_some(minutes)
}

pub(super) fn valid_auto_fetch_timeout_seconds(seconds: u64) -> Option<u64> {
    (seconds > 0 && u32::try_from(seconds).is_ok()).then_some(seconds)
}

impl Preferences {
    pub(in crate::app) fn auto_fetch_interval(&self) -> Option<Duration> {
        let minutes = self.auto_fetch_minutes;
        (minutes != 0).then(|| Duration::from_secs(minutes * 60))
    }

    pub(in crate::app) fn auto_fetch_timeout(&self) -> Duration {
        Duration::from_secs(self.auto_fetch_timeout_seconds)
    }
}

pub(super) fn git_preferences(draft: &PreferenceDraft, cx: &mut Context<Client>) -> Div {
    let p = palette(cx);
    div()
        .flex()
        .flex_col()
        .gap(px(18.))
        .child(preference_row(
            "app.preferences.auto_fetch_interval",
            duration_input(
                &draft.auto_fetch,
                "app.preferences.auto_fetch_interval",
                "app.preferences.minutes",
                cx,
            ),
            Some("app.preferences.auto_fetch_interval_hint"),
            p,
        ))
        .child(preference_row(
            "app.preferences.auto_fetch_timeout",
            duration_input(
                &draft.auto_fetch_timeout,
                "app.preferences.auto_fetch_timeout",
                "app.preferences.seconds",
                cx,
            ),
            Some("app.preferences.auto_fetch_timeout_hint"),
            p,
        ))
        .child(preference_action_row(
            "app.preferences.global_config",
            button(
                "open-global-git-config",
                tr!("app.common.open"),
                Tone::Outline,
                true,
                cx,
                cx.listener(|this, _, window, cx| this.open_global_git_config(window, cx)),
            )
            .rounded_full()
            .px(px(14.))
            .self_start(),
            p,
        ))
}

fn duration_input(
    state: &Entity<InputState>,
    label_key: &'static str,
    unit_key: &'static str,
    cx: &App,
) -> Input {
    Input::new(state)
        .w(px(132.))
        .max_w_full()
        .min_w_0()
        .suffix(unit_suffix(tr!(unit_key), cx))
        .aria_label(tr!(label_key))
}

impl Client {
    fn open_global_git_config(&mut self, window: &Window, cx: &mut Context<Self>) {
        Self::spawn_background(
            window,
            cx,
            || {
                let path = ritkraken_git::global_config_path()?
                    .ok_or(Message::Key("app.error.global_config_missing"))?;
                open_text_file(&path)
                    .map_err(|error| Message::detail("app.error.open_global_config", error))
            },
            |this, result, _, cx| {
                if let Some(draft) = this.preferences.as_mut() {
                    draft.error = result.err();
                    cx.notify();
                }
            },
        );
    }
}
