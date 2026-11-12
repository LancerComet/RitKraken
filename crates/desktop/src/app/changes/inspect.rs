mod history;

use std::sync::Arc;

use gpui_kit::{component::scroll::ScrollableElement, prelude::*, *};
use ritkraken_diff::{DiffState, DiffTypography};
use ritkraken_git::{self as git, BlameLine, FileHistory, FileTarget, Repo, Scope};

use crate::{
    app::{
        Client, Load,
        modals::{ContentModal, ModalHandle},
        review::parse_patch,
        widgets::{MUTED, error_text, note, table::ResizableColumns},
    },
    i18n::tr,
};

#[derive(Clone, Copy)]
pub(super) enum InspectionKind {
    History,
    Blame,
}

impl InspectionKind {
    fn title(self) -> String {
        tr!(match self {
            Self::History => "app.file_menu.history",
            Self::Blame => "app.file_menu.blame",
        })
    }
}

enum InspectionData {
    History(Arc<FileHistory>),
    Blame(Arc<[BlameLine]>),
}

pub(super) struct FileInspection {
    repo: Repo,
    file: FileTarget,
    kind: InspectionKind,
    data: Load<InspectionData>,
    typography: DiffTypography,
    scroll: UniformListScrollHandle,
    history_columns: ResizableColumns,
    limit: usize,
    loading: bool,
    selected: Option<String>,
    preview: Option<Load<Entity<DiffState>>>,
    preview_request: u64,
}

impl FileInspection {
    pub(super) fn open(
        repo: Repo,
        file: FileTarget,
        kind: InspectionKind,
        typography: DiffTypography,
        window: &mut Window,
        cx: &mut Context<Client>,
    ) {
        let path = file.change.path.to_string_lossy().into_owned();
        let inspection = cx.new(|cx| {
            let mut state = Self {
                repo,
                file,
                kind,
                data: Load::Pending,
                typography,
                scroll: UniformListScrollHandle::default(),
                history_columns: ResizableColumns::new(cx),
                limit: 500,
                loading: false,
                selected: None,
                preview: None,
                preview_request: 0,
            };
            state.load(window, cx);
            state
        });
        Client::present_content_modal(
            ContentModal::new(
                ModalHandle::new(cx),
                format!("{} · {}", kind.title(), path),
                inspection,
                size(px(1250.), px(817.)),
                false,
            ),
            window,
            cx,
        );
    }

    fn load(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.loading {
            return;
        }
        self.loading = true;
        let (repo, file, kind, limit) =
            (self.repo.clone(), self.file.clone(), self.kind, self.limit);
        Client::spawn_background(
            window,
            cx,
            move || match kind {
                InspectionKind::History => repo
                    .file_history_at(&file, limit)
                    .map(|history| InspectionData::History(Arc::new(history))),
                InspectionKind::Blame => repo
                    .blame_file(&file)
                    .map(|lines| InspectionData::Blame(lines.into())),
            },
            |this, result, window, cx| {
                this.loading = false;
                this.data = result.into();
                if this.selected.is_none()
                    && matches!(&this.data, Load::Ready(InspectionData::History(history)) if !history.commits.is_empty())
                {
                    this.select_revision(0, window, cx);
                }
                cx.notify();
            },
        );
    }

    fn select_revision(&mut self, index: usize, window: &Window, cx: &mut Context<Self>) {
        let Load::Ready(InspectionData::History(history)) = &self.data else {
            return;
        };
        let Some(revision) = history.commits.get(index) else {
            return;
        };
        self.selected = Some(revision.commit.id.clone());
        self.preview = Some(Load::Pending);
        self.preview_request += 1;
        let request = self.preview_request;
        let file = FileTarget {
            scope: Scope::Commit {
                id: revision.commit.id.clone(),
                parent: revision.commit.parents.first().cloned(),
            },
            change: revision.change.clone(),
        };
        let repo = self.repo.clone();
        Client::spawn_background(
            window,
            cx,
            move || repo.patch(&file).map(|patch| patch.bytes().to_vec()),
            move |this, result, _, cx| {
                if this.preview_request != request {
                    return;
                }
                let result = result
                    .map_err(crate::app::Message::from)
                    .and_then(|bytes| parse_patch(&bytes, false))
                    .map(|files| {
                        cx.new(|cx| DiffState::new(files, cx).with_context_lines(Some(3)))
                    });
                this.preview = Some(result.into());
                cx.notify();
            },
        );
        cx.notify();
    }

    fn blame_view(&self, lines: &Arc<[BlameLine]>, window: &Window, cx: &mut Context<Self>) -> Div {
        if lines.is_empty() {
            return div().child(note(tr!("app.file_menu.empty_file")));
        }
        let lines = lines.clone();
        let (family, font_size, row_height) = self.typography.resolve(window, cx);
        let list = uniform_list("file-blame", lines.len(), move |range, _, _| {
            range
                .map(|index| {
                    let line = &lines[index];
                    let uncommitted = line.commit.bytes().all(|byte| byte == b'0');
                    div()
                        .id(("blame-line", index))
                        .h(row_height)
                        .flex()
                        .items_center()
                        .bg(rgb(if uncommitted {
                            0xfff6dc
                        } else if index % 2 == 0 {
                            0xffffff
                        } else {
                            0xf6f7f8
                        }))
                        .child(
                            div()
                                .w(px(160.))
                                .px(px(8.))
                                .text_size(px(11.))
                                .truncate()
                                .child(if uncommitted {
                                    tr!("app.file_menu.uncommitted")
                                } else {
                                    line.author.clone()
                                }),
                        )
                        .child(
                            div()
                                .w(px(72.))
                                .text_size(px(11.))
                                .text_color(rgb(MUTED))
                                .child(if uncommitted {
                                    String::new()
                                } else {
                                    git::short_id(&line.commit).to_owned()
                                }),
                        )
                        .child(
                            div()
                                .w(px(55.))
                                .pr(px(10.))
                                .text_right()
                                .text_size(font_size)
                                .text_color(rgb(MUTED))
                                .child(line.number.to_string()),
                        )
                        .child(
                            div()
                                .font_family(family.clone())
                                .text_size(font_size)
                                .line_height(row_height)
                                .whitespace_nowrap()
                                .pr(px(20.))
                                .child(line.text.clone()),
                        )
                })
                .collect::<Vec<_>>()
        })
        .track_scroll(&self.scroll)
        .size_full()
        .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained);
        div()
            .size_full()
            .relative()
            .child(list)
            .vertical_scrollbar(&self.scroll)
    }
}

impl Render for FileInspection {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match &self.data {
            Load::Pending => div().child(note(tr!("app.file_menu.loading"))),
            Load::Failed(error) => div().child(error_text(error)),
            Load::Ready(InspectionData::History(history)) => self.history_view(history, cx),
            Load::Ready(InspectionData::Blame(lines)) => self.blame_view(lines, window, cx),
        };
        div().size_full().text_size(px(12.)).child(body)
    }
}
