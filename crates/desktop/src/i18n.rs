use std::ops::Deref;

use gpui_kit::{
    App, AppContext as _, Context, Entity, Window,
    base::input::{InputBaseState, InputMode, InputModeKind},
    component as gpui_component,
};

#[cfg(test)]
mod tests;

macro_rules! tr {
    ($($tokens:tt)*) => {
        ::rust_i18n::t!($($tokens)*).into_owned()
    };
}

pub(crate) use tr;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Language {
    #[default]
    English,
    SimplifiedChinese,
    TraditionalChinese,
    Japanese,
}

impl Language {
    pub(crate) const ALL: [Self; 4] = [
        Self::English,
        Self::SimplifiedChinese,
        Self::TraditionalChinese,
        Self::Japanese,
    ];

    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::SimplifiedChinese => "zh-CN",
            Self::TraditionalChinese => "zh-TW",
            Self::Japanese => "ja",
        }
    }

    pub(crate) fn native_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::SimplifiedChinese => "简体中文",
            Self::TraditionalChinese => "繁體中文",
            Self::Japanese => "日本語",
        }
    }

    pub(crate) fn from_code(code: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|language| language.code() == code)
            .unwrap_or_default()
    }

    pub(crate) fn apply(self) {
        gpui_component::set_locale(self.code());
    }
}

pub(crate) fn init() {
    rust_i18n::extend!(gpui_component);
}

pub(crate) struct LocalizedInput<M: InputModeKind = InputMode> {
    entity: Entity<InputBaseState<M>>,
    key: &'static str,
}

impl<M: InputModeKind> LocalizedInput<M> {
    pub(crate) fn new(
        key: &'static str,
        cx: &mut App,
        build: impl FnOnce(&mut Context<InputBaseState<M>>) -> InputBaseState<M>,
    ) -> Self {
        let entity = cx.new(|cx| build(cx).placeholder(tr!(key)));
        Self { entity, key }
    }

    pub(crate) fn localize(&self, window: &mut Window, cx: &mut App) {
        self.entity.update(cx, |input, cx| {
            input.set_placeholder(tr!(self.key), window, cx);
        });
    }
}

impl<M: InputModeKind> Deref for LocalizedInput<M> {
    type Target = Entity<InputBaseState<M>>;

    fn deref(&self) -> &Self::Target {
        &self.entity
    }
}
