mod editor;
mod git;
mod view;

use std::ops::RangeInclusive;

use gpui_kit::{component::input::InputState, *};

use crate::i18n::{Language, LocalizedInput};
use editor::{EditorDraft, EditorTypography};

use super::{
    Client, Message, Tab,
    review::DiffLayout,
    theme::{ThemeId, apply_theme},
};

const INITIAL_COMMIT_RANGE: RangeInclusive<usize> = 500..=100_000;

#[derive(Clone)]
pub(super) struct Preferences {
    pub(super) language: Language,
    pub(super) theme: ThemeId,
    pub(super) default_branch: String,
    auto_fetch_minutes: u64,
    pub(super) initial_commits: usize,
    pub(super) show_all_commits: bool,
    pub(super) lazy_history: bool,
    pub(super) remember_tabs: bool,
    pub(super) collapse_review_sidebar: bool,
    pub(super) diff_layout: DiffLayout,
    pub(super) wrap_lines: bool,
    pub(super) editor: EditorTypography,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            language: Language::default(),
            theme: ThemeId::default(),
            default_branch: "main".into(),
            auto_fetch_minutes: git::DEFAULT_AUTO_FETCH_MINUTES,
            initial_commits: 2000,
            show_all_commits: false,
            lazy_history: true,
            remember_tabs: true,
            collapse_review_sidebar: true,
            diff_layout: DiffLayout::Split,
            wrap_lines: false,
            editor: EditorTypography::default(),
        }
    }
}

impl Preferences {
    pub(super) fn read(value: &serde_json::Value) -> Self {
        let defaults = Self::default();
        Self {
            language: value["language"]
                .as_str()
                .map(Language::from_code)
                .unwrap_or_default(),
            theme: value["theme"]
                .as_str()
                .map(ThemeId::from_id)
                .unwrap_or_default(),
            default_branch: value["default_branch"]
                .as_str()
                .map_or(defaults.default_branch, str::to_owned),
            auto_fetch_minutes: value["auto_fetch_minutes"]
                .as_u64()
                .and_then(git::valid_auto_fetch_minutes)
                .unwrap_or(defaults.auto_fetch_minutes),
            initial_commits: value["initial_commits"]
                .as_u64()
                .and_then(|count| usize::try_from(count).ok())
                .filter(|count| INITIAL_COMMIT_RANGE.contains(count))
                .unwrap_or(defaults.initial_commits),
            show_all_commits: value["show_all_commits"]
                .as_bool()
                .unwrap_or(defaults.show_all_commits),
            lazy_history: value["lazy_history"]
                .as_bool()
                .unwrap_or(defaults.lazy_history),
            remember_tabs: value["remember_tabs"]
                .as_bool()
                .unwrap_or(defaults.remember_tabs),
            collapse_review_sidebar: value["collapse_review_sidebar"]
                .as_bool()
                .unwrap_or(defaults.collapse_review_sidebar),
            diff_layout: DiffLayout::ALL
                .into_iter()
                .find(|layout| value["diff_layout"].as_str() == Some(layout.id()))
                .unwrap_or(defaults.diff_layout),
            wrap_lines: value["wrap_lines"].as_bool().unwrap_or(defaults.wrap_lines),
            editor: EditorTypography::read(&value["editor_typography"]),
        }
    }

    pub(super) fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "language": self.language.code(),
            "theme": self.theme.id(),
            "default_branch": self.default_branch,
            "auto_fetch_minutes": self.auto_fetch_minutes,
            "initial_commits": self.initial_commits,
            "show_all_commits": self.show_all_commits,
            "lazy_history": self.lazy_history,
            "remember_tabs": self.remember_tabs,
            "collapse_review_sidebar": self.collapse_review_sidebar,
            "diff_layout": self.diff_layout.id(),
            "wrap_lines": self.wrap_lines,
            "editor_typography": self.editor.json(),
        })
    }

    pub(super) fn history_limit(&self) -> Option<usize> {
        (!self.show_all_commits).then_some(self.initial_commits)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PreferencePage {
    General,
    Git,
    Appearance,
    Editor,
}

impl PreferencePage {
    fn label_key(self) -> &'static str {
        match self {
            Self::General => "app.preferences.general",
            Self::Git => "app.preferences.git",
            Self::Appearance => "app.preferences.appearance",
            Self::Editor => "app.preferences.editor",
        }
    }
}

pub(super) struct PreferenceDraft {
    pub(super) page: PreferencePage,
    pub(super) options: Preferences,
    pub(super) branch: LocalizedInput,
    pub(super) count: Entity<InputState>,
    auto_fetch: Entity<InputState>,
    editor: EditorDraft,
    pub(super) error: Option<Message>,
}

impl Client {
    pub(super) fn open_preferences(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.session.opening || self.content_modal.is_some() {
            return;
        }
        if self.preferences.is_some() {
            self.save_preferences(window, cx);
            return;
        }
        let options = self.saved.preferences.clone();
        let branch = LocalizedInput::new("app.preferences.branch_placeholder", cx, |cx| {
            InputState::new(window, cx).default_value(options.default_branch.clone())
        });
        let count = cx.new(|cx| {
            InputState::new(window, cx).default_value(options.initial_commits.to_string())
        });
        let auto_fetch = cx.new(|cx| unsigned_input(Some(options.auto_fetch_minutes), window, cx));
        let editor = EditorDraft::new(&options, window, cx);
        self.manager.mode = None;
        self.branch_editor.clear();
        self.preferences = Some(PreferenceDraft {
            page: PreferencePage::General,
            options,
            branch,
            count,
            auto_fetch,
            editor,
            error: None,
        });
        self.modal_focus.focus(window, cx);
        cx.notify();
    }

    fn choose_language(&mut self, language: Language, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self.preferences.as_mut() else {
            return;
        };
        if self.saved.preferences.language == language {
            return;
        }
        draft.options.language = language;
        self.saved.preferences.language = language;
        self.localize(window, cx);
        self.saved.save();
        cx.notify();
    }

    fn choose_theme(&mut self, theme: ThemeId, cx: &mut Context<Self>) {
        let Some(draft) = self.preferences.as_mut() else {
            return;
        };
        if self.saved.preferences.theme == theme {
            return;
        }
        draft.options.theme = theme;
        self.saved.preferences.theme = theme;
        apply_theme(theme, cx);
        self.saved.save();
        cx.notify();
    }

    pub(super) fn save_preferences(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self.preferences.as_mut() else {
            return;
        };
        let mut options = draft.options.clone();
        if !options.show_all_commits {
            let parsed = draft
                .count
                .read(cx)
                .value()
                .trim()
                .parse::<usize>()
                .ok()
                .filter(|count| INITIAL_COMMIT_RANGE.contains(count));
            let Some(count) = parsed else {
                draft.page = PreferencePage::General;
                draft.error = Some(Message::Key("app.error.initial_commit_count"));
                cx.notify();
                return;
            };
            options.initial_commits = count;
        }
        let Some(auto_fetch_minutes) = draft
            .auto_fetch
            .read(cx)
            .value()
            .trim()
            .parse::<u64>()
            .ok()
            .and_then(git::valid_auto_fetch_minutes)
        else {
            draft.page = PreferencePage::Git;
            draft.error = Some(Message::Key("app.error.auto_fetch_minutes"));
            cx.notify();
            return;
        };
        options.auto_fetch_minutes = auto_fetch_minutes;
        options.default_branch = draft.branch.read(cx).value().trim().to_owned();
        match draft.editor.typography(cx) {
            Ok(typography) => options.editor = typography,
            Err(message) => {
                draft.page = PreferencePage::Editor;
                draft.error = Some(Message::Key(message));
                cx.notify();
                return;
            }
        }
        let previous = &self.saved.preferences;
        let limit = options.history_limit();
        let history_changed = previous.history_limit() != limit;
        let layout_changed = previous.diff_layout != options.diff_layout;
        let auto_fetch_changed = previous.auto_fetch_minutes != options.auto_fetch_minutes;
        if history_changed && self.session.tabs.iter().any(Tab::is_busy) {
            draft.error = Some(Message::Key("app.error.history_busy"));
            cx.notify();
            return;
        }
        if previous.wrap_lines != options.wrap_lines {
            for tab in self.session.tabs.iter_mut().filter_map(Tab::ready_mut) {
                tab.review.wrap_lines = options.wrap_lines;
            }
        }
        let tab_ids = if layout_changed || history_changed {
            self.session
                .tabs
                .iter()
                .filter_map(Tab::ready)
                .map(|tab| tab.id)
                .collect()
        } else {
            Vec::new()
        };
        if layout_changed {
            for &id in &tab_ids {
                self.set_diff_layout(id, options.diff_layout, cx);
            }
        }
        self.saved.preferences = options;
        self.dismiss_modal(window, cx);
        self.persist_session();
        if auto_fetch_changed {
            self.reset_auto_fetch_timer(window, cx);
        }
        if history_changed {
            for id in tab_ids {
                self.update_repo_with_limit(id, None, limit, window, cx);
            }
        }
        cx.notify();
    }

    fn localize(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.saved.preferences.language.apply();
        if let Some(draft) = &self.preferences {
            draft.branch.localize(window, cx);
            draft.editor.localize(window, cx);
        }
        self.manager.localize(window, cx);
        self.branch_editor.localize(window, cx);
        for tab in self.session.tabs.iter().filter_map(Tab::ready) {
            tab.refs.filter.localize(window, cx);
            tab.changes.summary.localize(window, cx);
            tab.changes.description.localize(window, cx);
        }
    }
}

fn unsigned_input(
    value: Option<impl ToString>,
    window: &mut Window,
    cx: &mut Context<InputState>,
) -> InputState {
    InputState::new(window, cx)
        .default_value(value.map(|number| number.to_string()).unwrap_or_default())
        .validate(|text, _| text.bytes().all(|byte| byte.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{DiffLayout, Language, Preferences, ThemeId};

    #[test]
    fn theme_preference_round_trips_and_handles_legacy_settings() {
        let legacy = serde_json::json!({"initial_commits": 750});
        assert_eq!(Preferences::read(&legacy).theme, ThemeId::default());

        let unknown = serde_json::json!({"theme": "nonexistent"});
        assert_eq!(Preferences::read(&unknown).theme, ThemeId::default());

        for theme in ThemeId::ALL {
            let options = Preferences::read(&serde_json::json!({"theme": theme.id()}));
            assert_eq!(options.theme, theme);
            assert_eq!(options.json()["theme"], theme.id());
        }
    }

    #[test]
    fn language_preference_preserves_legacy_settings() {
        let mut saved = serde_json::json!({"diff_layout": "Inline", "initial_commits": 750});
        assert_eq!(Preferences::read(&saved).language, Language::English);
        saved["language"] = "unknown".into();
        assert_eq!(Preferences::read(&saved).language, Language::English);

        for language in Language::ALL {
            saved["language"] = language.code().into();
            let options = Preferences::read(&saved);
            assert_eq!(options.language, language);
            assert_eq!(options.diff_layout, DiffLayout::Inline);
            assert_eq!(options.initial_commits, 750);
            assert_eq!(options.json()["language"], language.code());
            assert_eq!(options.json()["diff_layout"], "Inline");
        }
    }

    #[test]
    fn auto_fetch_preference_defaults_and_preserves_disabled_intervals() {
        let legacy = serde_json::json!({"initial_commits": 750});
        assert_eq!(
            Preferences::read(&legacy).auto_fetch_interval(),
            Some(Duration::from_secs(60))
        );

        for minutes in [0, 5, 60] {
            let options = Preferences::read(&serde_json::json!({"auto_fetch_minutes": minutes}));
            let saved = options.json();
            assert_eq!(saved["auto_fetch_minutes"], minutes);
            assert_eq!(
                Preferences::read(&saved).auto_fetch_interval(),
                (minutes != 0).then(|| Duration::from_secs(minutes * 60))
            );
        }

        for invalid in [
            serde_json::json!(-1),
            serde_json::json!(61),
            serde_json::json!("5"),
        ] {
            let options = Preferences::read(&serde_json::json!({"auto_fetch_minutes": invalid}));
            assert_eq!(options.auto_fetch_interval(), Some(Duration::from_secs(60)));
        }
    }
}
