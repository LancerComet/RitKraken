mod view;

use std::ops::RangeInclusive;

use gpui_kit::{component::input::InputState, *};

use super::{Client, review::DiffLayout};

const INITIAL_COMMIT_RANGE: RangeInclusive<usize> = 500..=100_000;

#[derive(Clone)]
pub(super) struct Preferences {
    pub(super) default_branch: String,
    pub(super) initial_commits: usize,
    pub(super) show_all_commits: bool,
    pub(super) lazy_history: bool,
    pub(super) remember_tabs: bool,
    pub(super) collapse_review_sidebar: bool,
    pub(super) diff_layout: DiffLayout,
    pub(super) wrap_lines: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            default_branch: "main".into(),
            initial_commits: 2000,
            show_all_commits: false,
            lazy_history: true,
            remember_tabs: true,
            collapse_review_sidebar: true,
            diff_layout: DiffLayout::Split,
            wrap_lines: false,
        }
    }
}

impl Preferences {
    pub(super) fn read(value: &serde_json::Value) -> Self {
        let defaults = Self::default();
        Self {
            default_branch: value["default_branch"]
                .as_str()
                .map_or(defaults.default_branch, str::to_owned),
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
                .find(|layout| value["diff_layout"].as_str() == Some(layout.label()))
                .unwrap_or(defaults.diff_layout),
            wrap_lines: value["wrap_lines"].as_bool().unwrap_or(defaults.wrap_lines),
        }
    }

    pub(super) fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "default_branch": self.default_branch,
            "initial_commits": self.initial_commits,
            "show_all_commits": self.show_all_commits,
            "lazy_history": self.lazy_history,
            "remember_tabs": self.remember_tabs,
            "collapse_review_sidebar": self.collapse_review_sidebar,
            "diff_layout": self.diff_layout.label(),
            "wrap_lines": self.wrap_lines,
        })
    }

    pub(super) fn history_limit(&self) -> Option<usize> {
        (!self.show_all_commits).then_some(self.initial_commits)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PreferencePage {
    General,
    Appearance,
    Editor,
}

pub(super) struct PreferenceDraft {
    pub(super) page: PreferencePage,
    pub(super) options: Preferences,
    pub(super) branch: Entity<InputState>,
    pub(super) count: Entity<InputState>,
    pub(super) error: Option<String>,
}

impl Client {
    pub(super) fn open_preferences(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.session.opening {
            return;
        }
        if self.preferences.is_some() {
            self.save_preferences(window, cx);
            return;
        }
        let options = self.saved.preferences.clone();
        let branch = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Use Git config")
                .default_value(options.default_branch.clone())
        });
        let count = cx.new(|cx| {
            InputState::new(window, cx).default_value(options.initial_commits.to_string())
        });
        self.manager.mode = None;
        self.branch_dialog.open = false;
        self.preferences = Some(PreferenceDraft {
            page: PreferencePage::General,
            options,
            branch,
            count,
            error: None,
        });
        self.focus.focus(window, cx);
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
                draft.error = Some("Enter an initial commit count between 500 and 100,000.".into());
                cx.notify();
                return;
            };
            options.initial_commits = count;
        }
        options.default_branch = draft.branch.read(cx).value().trim().to_owned();
        let previous = &self.saved.preferences;
        let limit = options.history_limit();
        let history_changed = previous.history_limit() != limit;
        let layout_changed = previous.diff_layout != options.diff_layout;
        if history_changed && self.session.tabs.iter().any(|tab| tab.busy) {
            draft.error = Some(
                "Wait for the current Git operation to finish before changing the history limit."
                    .into(),
            );
            cx.notify();
            return;
        }
        if previous.wrap_lines != options.wrap_lines {
            for tab in &mut self.session.tabs {
                tab.review.wrap_lines = options.wrap_lines;
            }
        }
        let tab_ids = if layout_changed || history_changed {
            self.session.tabs.iter().map(|tab| tab.id).collect()
        } else {
            Vec::new()
        };
        if layout_changed {
            for &id in &tab_ids {
                self.set_diff_layout(id, options.diff_layout, cx);
            }
        }
        self.saved.preferences = options;
        self.preferences = None;
        self.persist_session();
        self.focus.focus(window, cx);
        if history_changed {
            for id in tab_ids {
                self.update_repo_with_limit(id, None, limit, window, cx);
            }
        }
        cx.notify();
    }
}
