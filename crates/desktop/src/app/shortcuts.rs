use gpui_kit::*;

use super::history::rebase::{
    RebaseDrop, RebaseEdit, RebaseFixup, RebaseMoveDown, RebaseMoveUp, RebasePick, RebaseReword,
    RebaseSquash,
};
use super::{
    Escape, NextRow, OpenPalette, OpenPreferences, OpenRepos, PreviousRow, Refresh, SearchCommits,
    Submit,
};

const GRAPH_KEYS: &str = "CommitGraph && !Input";
const REBASE_KEYS: &str = "CommitGraph && RebasePlan && !Input";

pub(super) fn graph_context(editing_rebase: bool) -> &'static str {
    if editing_rebase {
        "CommitGraph RebasePlan"
    } else {
        "CommitGraph"
    }
}

pub(super) fn init(cx: &mut App) {
    cx.bind_keys(bindings(cfg!(target_os = "macos")));
}

fn bindings(mac: bool) -> Vec<KeyBinding> {
    let (open, submit, preferences) = if mac {
        ("cmd-o", "cmd-enter", "cmd-,")
    } else {
        ("ctrl-o", "ctrl-enter", "ctrl-,")
    };
    vec![
        KeyBinding::new(open, OpenRepos, Some("RitKraken")),
        KeyBinding::new(preferences, OpenPreferences, Some("RitKraken")),
        KeyBinding::new(
            if mac { "cmd-shift-p" } else { "ctrl-shift-p" },
            OpenPalette,
            Some("RitKraken"),
        ),
        KeyBinding::new(
            if mac { "cmd-f" } else { "ctrl-f" },
            SearchCommits,
            Some("CommitGraph && !Input"),
        ),
        KeyBinding::new("f5", Refresh, Some("RitKraken")),
        KeyBinding::new("escape", Escape, Some("RitKraken")),
        KeyBinding::new("down", NextRow, Some(GRAPH_KEYS)),
        KeyBinding::new("up", PreviousRow, Some(GRAPH_KEYS)),
        KeyBinding::new("p", RebasePick, Some(REBASE_KEYS)),
        KeyBinding::new("r", RebaseReword, Some(REBASE_KEYS)),
        KeyBinding::new("e", RebaseEdit, Some(REBASE_KEYS)),
        KeyBinding::new("s", RebaseSquash, Some(REBASE_KEYS)),
        KeyBinding::new("f", RebaseFixup, Some(REBASE_KEYS)),
        KeyBinding::new("d", RebaseDrop, Some(REBASE_KEYS)),
        KeyBinding::new("alt-up", RebaseMoveUp, Some(REBASE_KEYS)),
        KeyBinding::new("alt-down", RebaseMoveDown, Some(REBASE_KEYS)),
        KeyBinding::new(submit, Submit, Some("RitKraken")),
        KeyBinding::new(submit, Submit, Some("CommitOperation")),
    ]
}

#[cfg(test)]
mod tests {
    use super::{
        NextRow, PreviousRow, RebaseDrop, RebaseEdit, RebaseFixup, RebaseMoveDown, RebaseMoveUp,
        RebasePick, RebaseReword, RebaseSquash, bindings, graph_context,
    };
    use gpui_kit::{KeyBinding, KeyContext, Keymap, Keystroke, OperatingSystem};
    use std::any::TypeId;

    fn resolve(key: &str, rebase: bool, input: bool) -> Vec<KeyBinding> {
        let mut contexts = vec![
            KeyContext::parse("RitKraken").unwrap(),
            KeyContext::parse(graph_context(rebase)).unwrap(),
        ];
        if input {
            contexts.push(KeyContext::parse("Input").unwrap());
        }
        Keymap::new(bindings(false))
            .bindings_for_input(
                &[Keystroke::parse_with_os(key, OperatingSystem::Windows).unwrap()],
                &contexts,
            )
            .0
            .into_vec()
    }

    #[test]
    fn palette_shortcut_resolves_on_windows_and_macos_while_typing() {
        for (mac, key, os) in [
            (false, "ctrl-shift-p", OperatingSystem::Windows),
            (true, "cmd-shift-p", OperatingSystem::Mac),
        ] {
            let contexts = [
                KeyContext::parse("RitKraken").unwrap(),
                KeyContext::parse("Input").unwrap(),
            ];
            let matched = Keymap::new(bindings(mac))
                .bindings_for_input(&[Keystroke::parse_with_os(key, os).unwrap()], &contexts)
                .0;
            assert_eq!(matched.len(), 1, "{key}");
            assert_eq!(
                matched[0].action().as_any().type_id(),
                TypeId::of::<super::OpenPalette>()
            );
        }
    }

    #[test]
    fn graph_inputs_keep_all_letters_and_navigation_keys() {
        for rebase in [false, true] {
            for letter in 'a'..='z' {
                assert!(
                    resolve(&letter.to_string(), rebase, true).is_empty(),
                    "{letter}"
                );
            }
            for key in ["up", "down", "alt-up", "alt-down"] {
                assert!(resolve(key, rebase, true).is_empty(), "{key}");
            }
        }
    }

    #[test]
    fn rebase_actions_are_available_only_in_the_rebase_graph() {
        for (key, action) in [
            ("p", TypeId::of::<RebasePick>()),
            ("r", TypeId::of::<RebaseReword>()),
            ("e", TypeId::of::<RebaseEdit>()),
            ("s", TypeId::of::<RebaseSquash>()),
            ("f", TypeId::of::<RebaseFixup>()),
            ("d", TypeId::of::<RebaseDrop>()),
            ("alt-up", TypeId::of::<RebaseMoveUp>()),
            ("alt-down", TypeId::of::<RebaseMoveDown>()),
        ] {
            assert!(resolve(key, false, false).is_empty(), "{key}");
            let matched = resolve(key, true, false);
            assert_eq!(matched.len(), 1, "{key}");
            assert_eq!(matched[0].action().as_any().type_id(), action, "{key}");
        }
        assert!(
            resolve("down", false, false)[0]
                .action()
                .as_any()
                .is::<NextRow>()
        );
        assert!(
            resolve("up", false, false)[0]
                .action()
                .as_any()
                .is::<PreviousRow>()
        );
    }
}
