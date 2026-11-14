use gpui_kit::*;
use ritkraken_git::{FileTarget, Scope};

use super::{load_diff, loading::load_source};
use crate::app::{Client, Load, Notice};

impl Client {
    /// Revalidate the mutable preview without replacing the displayed review.
    /// Immutable commit previews are independent of working-directory changes.
    pub(in crate::app) fn refresh_diff(
        &mut self,
        id: u64,
        file: FileTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(file.scope, Scope::Commit { .. }) {
            return;
        }
        let Some(tab) = self.tab_mut(id) else { return };
        let Some(current) = tab.review.active.as_ref() else {
            return;
        };
        let previous = match &current.patch {
            Load::Ready(patch) if current.file == file => Some(patch.clone()),
            _ => None,
        };
        let source_requested = current.source.is_some();
        let source_text = current.source_text.clone();
        let repo = tab.repo.clone();
        let ignore = tab.review.ignore_edge_whitespace;
        let request = tab.review.request;
        tab.review.refresh_request = tab.review.refresh_request.wrapping_add(1);
        let refresh_request = tab.review.refresh_request;
        Self::spawn_background(
            window,
            cx,
            move || {
                let loaded = load_diff(&repo, &file, ignore, previous.as_deref())?;
                let source = (source_requested && loaded.is_some())
                    .then(|| load_source(&repo, &file, source_text.as_deref()));
                Ok::<_, ritkraken_git::Error>((file, loaded, source))
            },
            move |this, result, window, cx| {
                if !this.tab(id).is_some_and(|tab| {
                    tab.review.request == request && tab.review.refresh_request == refresh_request
                }) {
                    return;
                }
                // Validation alone must not invalidate the staging controls
                // already on screen. Advance the review version only when its
                // patch actually changes.
                let request = if matches!(&result, Ok((_, Some(_), _))) {
                    let review = &mut this.tab_mut(id).unwrap().review;
                    review.request += 1;
                    review.request
                } else {
                    request
                };
                let Some((current, layout)) = this.review_for_request(id, request) else {
                    return;
                };
                match result {
                    Ok((file, Some(loaded), source)) => {
                        current.file = file;
                        current.apply_loaded(loaded, layout, None, cx);
                        let reload_source = source.is_none() && current.source.is_some();
                        if let Some(source) = source {
                            current.apply_source(source, cx);
                        }
                        // The user can switch to File View while this read is
                        // running. Keep that view and load its current version.
                        if reload_source {
                            this.load_review_source(id, window, cx);
                        }
                        cx.notify();
                    }
                    Ok((_, None, _)) => {}
                    Err(error) => {
                        if let Some(tab) = this.tab_mut(id) {
                            tab.notify(Notice::failure(error), window, cx);
                        }
                        cx.notify();
                    }
                }
            },
        );
    }
}
