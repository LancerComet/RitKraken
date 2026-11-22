use gpui_kit::*;

use super::{
    PaletteForm,
    model::{Command, CommandContext, Target},
};
use crate::app::{Client, Message, RepoOperation, changes::StageAction, compare::CompareRef};

impl PaletteForm {
    pub(super) fn execute(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.modal.is_open() {
            return;
        }
        let Some(candidate) = self
            .filtered
            .get(self.selected)
            .and_then(|index| self.candidates.get(*index))
            .cloned()
        else {
            return;
        };
        let id = self.repo_id;
        let modal = self.modal.clone();
        let result = self.client.update(cx, |client, cx| {
            if !modal.is_open() {
                return Ok(());
            }
            match &candidate.target {
                Target::Command(command) => {
                    let context = id
                        .filter(|id| client.session.active == Some(*id))
                        .and_then(|id| client.tab(id))
                        .map(CommandContext::from);
                    if !command.available(context) {
                        return Err(Message::Key("app.error.repo_busy"));
                    }
                }
                Target::Branch(_) | Target::Commit(_) | Target::File(_) => {
                    if !id.and_then(|id| client.tab(id)).is_some_and(|tab| {
                        !tab.busy && tab.rebase.is_none() && client.session.active == id
                    }) {
                        return Err(Message::Key("app.error.repo_busy"));
                    }
                }
            }
            client.close_content_modal(&modal, window, cx);
            // Command handoff replaces the palette immediately, so other modal
            // flows do not reject the outgoing palette's close animation.
            client.closing_modal = None;
            match candidate.target {
                Target::Command(command) => client.run_palette_command(command, id, window, cx),
                Target::Branch(request) => {
                    if let Some(id) = id {
                        client.update_repo(id, Some(RepoOperation::Reference(request)), window, cx);
                    }
                }
                Target::Commit(commit) => {
                    if let Some(id) = id {
                        client.reveal_commit(id, commit, window, cx);
                    }
                }
                Target::File(file) => {
                    if let Some(id) = id {
                        client.open_diff(id, file, window, cx);
                        client.show_source(id, true, window, cx);
                    }
                }
            }
            Ok(())
        });
        match result {
            Ok(Ok(())) => {}
            Ok(Err(message)) => self.error = Some(message),
            Err(error) => self.error = Some(Message::detail("app.error.operation", error)),
        }
        cx.notify();
    }
}

impl Client {
    fn run_palette_command(
        &mut self,
        command: Command,
        id: Option<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match command {
            Command::OpenRepos => self.open_manager(window, cx),
            Command::Preferences => self.open_preferences(window, cx),
            Command::Refresh => self.refresh(window, cx),
            Command::SearchCommits => self.open_commit_search(window, cx),
            Command::Recovery => self.open_recovery(window, cx),
            Command::Worktrees => self.open_worktrees(window, cx),
            Command::CreateBranch => {
                if let Some(id) = id {
                    self.start_toolbar_branch(id, window, cx);
                }
            }
            Command::Workspace => {
                if let Some(id) = id {
                    self.select(id, None, Some(ScrollStrategy::Nearest), window, cx);
                    if let Some(tab) = self.tab(id) {
                        tab.changes
                            .summary
                            .read(cx)
                            .focus_handle(cx)
                            .focus(window, cx);
                    }
                }
            }
            Command::Compare => {
                let Some(id) = id else { return };
                let Some((base, compare)) = self.tab(id).map(|tab| {
                    let snapshot = &tab.active_history().snapshot;
                    let base = CompareRef::head(snapshot);
                    // Prefer the current branch's upstream tracking ref.
                    let compare = snapshot
                        .upstream
                        .as_ref()
                        .and_then(|upstream| {
                            upstream
                                .reference
                                .strip_prefix("refs/heads/")
                                .map(|branch| format!("refs/remotes/{}/{branch}", upstream.remote))
                        })
                        .and_then(|revision| {
                            snapshot
                                .refs
                                .iter()
                                .find(|reference| reference.full_name() == revision)
                                .map(CompareRef::reference)
                        })
                        .unwrap_or_else(|| CompareRef::head(snapshot));
                    (base, compare)
                }) else {
                    return;
                };
                self.open_compare(id, base, compare, window, cx);
            }
            Command::StageAll | Command::UnstageAll => {
                let Some(id) = id else { return };
                let Some(tab) = self.tab(id) else { return };
                let (changes, action) = match command {
                    Command::StageAll => (
                        &tab.history.data.snapshot.status.unstaged,
                        StageAction::Stage,
                    ),
                    Command::UnstageAll => (
                        &tab.history.data.snapshot.status.staged,
                        StageAction::Unstage,
                    ),
                    Command::OpenRepos
                    | Command::Preferences
                    | Command::Refresh
                    | Command::SearchCommits
                    | Command::Recovery
                    | Command::Worktrees
                    | Command::CreateBranch
                    | Command::Workspace
                    | Command::Fetch
                    | Command::Push
                    | Command::Stash
                    | Command::Pop
                    | Command::Compare => return,
                };
                let edit = RepoOperation::StageFiles(action, Some(changes.clone()));
                self.update_repo(id, Some(edit), window, cx);
            }
            Command::Fetch | Command::Push | Command::Stash | Command::Pop => {
                let Some(id) = id else { return };
                let edit = match command {
                    Command::Fetch => RepoOperation::Fetch(None),
                    Command::Stash => RepoOperation::Stash,
                    Command::Pop => RepoOperation::Pop,
                    Command::Push => {
                        let Some(snapshot) = self.tab(id).map(|tab| &tab.history.data.snapshot)
                        else {
                            return;
                        };
                        let (Some(branch), Some(upstream)) = (&snapshot.branch, &snapshot.upstream)
                        else {
                            return;
                        };
                        RepoOperation::Push {
                            branch: branch.clone(),
                            upstream: upstream.clone(),
                            set_upstream: false,
                        }
                    }
                    Command::OpenRepos
                    | Command::Preferences
                    | Command::Refresh
                    | Command::SearchCommits
                    | Command::Recovery
                    | Command::Worktrees
                    | Command::CreateBranch
                    | Command::StageAll
                    | Command::UnstageAll
                    | Command::Workspace
                    | Command::Compare => return,
                };
                self.update_repo(id, Some(edit), window, cx);
            }
        }
    }
}
