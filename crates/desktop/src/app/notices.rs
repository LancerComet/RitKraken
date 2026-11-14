use gpui_kit::{
    component::{
        WindowExt as _,
        notification::{Notification, NotificationType},
        scroll::ScrollableElement,
    },
    prelude::*,
    *,
};

use super::{Message, RepoTab, theme::palette};
use crate::i18n::tr;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Level {
    Info,
    Warning,
    Error,
}

impl Level {
    fn key(self, repository: u64) -> SharedString {
        format!("{repository}:{self:?}").into()
    }
}

pub(super) struct Notice {
    level: Level,
    pub(super) message: Message,
}

impl Notice {
    pub(super) fn info(message: Message) -> Self {
        Self {
            level: Level::Info,
            message,
        }
    }

    pub(super) fn warning(message: Message) -> Self {
        Self {
            level: Level::Warning,
            message,
        }
    }

    pub(super) fn failure(message: impl Into<Message>) -> Self {
        let message = message.into();
        // A rejected user operation needs attention; a failed Git/I/O operation
        // is an error. Keep that distinction consistent for every caller.
        let level = if matches!(&message, Message::Git(ritkraken_git::Error::Rule(_))) {
            Level::Warning
        } else {
            Level::Error
        };
        Self { level, message }
    }

    pub(super) fn is_error(&self) -> bool {
        self.level == Level::Error
    }

    pub(super) fn clear(repository: u64, window: &mut Window, cx: &mut App) {
        for level in [Level::Info, Level::Warning, Level::Error] {
            window.remove_notification1::<Self>(level.key(repository), cx);
        }
    }

    fn toast(&self, id: u64, repository: &str, cx: &App) -> Notification {
        let p = palette(cx);
        let (kind, title, color, background) = match self.level {
            Level::Info => (
                NotificationType::Info,
                "app.notice.info",
                p.info_fg,
                p.info_bg,
            ),
            Level::Warning => (
                NotificationType::Warning,
                "app.notice.warning",
                p.warning,
                p.warning_bg,
            ),
            Level::Error => (
                NotificationType::Error,
                "app.notice.error",
                p.danger,
                p.danger_bg,
            ),
        };
        let message = SharedString::from(self.message.text());
        let title = SharedString::from(format!("{} · {repository}", tr!(title)));
        Notification::new()
            .id1::<Self>(self.level.key(id))
            .with_type(kind)
            .bg(rgb(background))
            .border_color(rgb(color))
            .border_l_4()
            .rounded(px(8.))
            .text_color(rgb(p.text))
            .autohide(self.level == Level::Info)
            .content(move |_, _, _| {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .pr(px(12.))
                    .text_size(px(12.))
                    .line_height(px(18.))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(color))
                            .child(title.clone()),
                    )
                    .child(
                        div()
                            .id("notice-detail")
                            .max_h(px(180.))
                            .pr(px(8.))
                            .overflow_y_scrollbar()
                            .child(message.clone()),
                    )
                    .into_any_element()
            })
    }
}

impl RepoTab {
    pub(super) fn notify(&mut self, notice: Notice, window: &mut Window, cx: &mut App) {
        window.push_notification(notice.toast(self.id, &self.repo.name(), cx), cx);
        self.notice = Some(notice);
    }
}
