use std::collections::BTreeMap;

use serde_json::Value;

use super::{Language, gpui_component, init, tr};

fn messages<'a>(key: &str, value: &'a Value, result: &mut BTreeMap<String, &'a str>) {
    if let Some(children) = value.as_object() {
        for (name, child) in children {
            messages(&format!("{key}.{name}"), child, result);
        }
    } else {
        let text = value
            .as_str()
            .expect("translation messages must be strings");
        assert!(!text.trim().is_empty(), "{key} is empty");
        result.insert(key.into(), text);
    }
}

fn placeholders(text: &str) -> Vec<&str> {
    let mut names = text
        .split("%{")
        .skip(1)
        .map(|part| part.split_once('}').expect("unclosed placeholder").0)
        .collect::<Vec<_>>();
    names.sort_unstable();
    names
}

#[test]
fn app_catalogs_have_matching_keys_and_placeholders() {
    let catalogs = [
        include_str!("../../locales/en.json"),
        include_str!("../../locales/zh-CN.json"),
        include_str!("../../locales/zh-TW.json"),
        include_str!("../../locales/ja.json"),
    ]
    .map(|json| serde_json::from_str::<Value>(json).unwrap());
    let mut english = BTreeMap::new();
    messages("app", &catalogs[0]["app"], &mut english);
    for (language, catalog) in Language::ALL.into_iter().zip(&catalogs) {
        let mut localized = BTreeMap::new();
        messages("app", &catalog["app"], &mut localized);
        assert!(localized.keys().eq(english.keys()), "{}", language.code());
        for (key, text) in localized {
            assert_eq!(placeholders(text), placeholders(english[&key]), "{key}");
            assert_eq!(
                crate::_rust_i18n_try_translate(language.code(), &key).as_deref(),
                Some(text),
                "{key} for {}",
                language.code()
            );
        }
    }
    assert_eq!(
        tr!("app.toolbar.fetch_remote", locale = "ja", remote = "origin"),
        "origin をフェッチ"
    );
}

#[test]
fn registers_missing_toolkit_translations() {
    init();
    for (locale, key, expected) in [
        ("ja", "Input.Copy", "コピー"),
        ("ja", "Diff.NoNewline", "ファイル末尾に改行がありません"),
        ("zh-TW", "Diff.NoNewline", "檔案末尾沒有換行"),
        ("zh-CN", "Input.Copy", "复制"),
    ] {
        assert_eq!(
            gpui_component::_rust_i18n_try_translate(locale, key).as_deref(),
            Some(expected)
        );
    }
}
