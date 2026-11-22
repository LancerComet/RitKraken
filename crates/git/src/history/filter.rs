use std::collections::BTreeSet;

/// References the history graph hides or restricts itself to, identified by
/// their full ref name (`refs/heads/x`, `refs/remotes/o/x`, `refs/tags/t`).
/// A non-empty `solo` set takes precedence over `hidden`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HistoryFilter {
    pub hidden: BTreeSet<String>,
    pub solo: BTreeSet<String>,
}

impl HistoryFilter {
    /// Any hide or solo entry is set.
    pub fn is_active(&self) -> bool {
        !self.hidden.is_empty() || !self.solo.is_empty()
    }

    /// Whether `full_name` is filtered out of the graph view. While a solo set
    /// exists every ref outside it is excluded; otherwise hidden refs are.
    pub fn is_excluded(&self, full_name: &str) -> bool {
        if self.solo.is_empty() {
            self.hidden.contains(full_name)
        } else {
            !self.solo.contains(full_name)
        }
    }

    /// Hidden entries move out of any solo; solo entries leave the hidden set.
    pub fn hide(&mut self, full_name: &str) {
        self.hidden.insert(full_name.to_owned());
        self.solo.remove(full_name);
    }

    pub fn show(&mut self, full_name: &str) {
        self.hidden.remove(full_name);
    }

    pub fn solo(&mut self, full_name: &str) {
        self.solo.insert(full_name.to_owned());
        self.hidden.remove(full_name);
    }

    pub fn unsolo(&mut self, full_name: &str) {
        self.solo.remove(full_name);
    }

    pub fn clear(&mut self) {
        self.hidden.clear();
        self.solo.clear();
    }
}
