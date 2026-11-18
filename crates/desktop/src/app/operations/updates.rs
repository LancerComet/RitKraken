/// Coalesces background reads and rejects their stale results when an explicit
/// Git operation starts. Background reads never own the visible busy state.
#[derive(Default)]
pub(in crate::app) struct RepoUpdates {
    generation: u64,
    reading: bool,
}

impl RepoUpdates {
    pub(super) fn begin(&mut self, read_only: bool, explicit: bool) -> Option<u64> {
        if read_only && self.reading && !explicit {
            return None;
        }
        self.generation = self.generation.wrapping_add(1);
        self.reading = read_only;
        Some(self.generation)
    }

    pub(in crate::app) fn in_flight(&self) -> bool {
        self.reading
    }

    pub(super) fn finish(&mut self, generation: u64) -> bool {
        if self.generation != generation {
            return false;
        }
        self.reading = false;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::RepoUpdates;

    #[test]
    fn overlapping_background_reads_are_coalesced() {
        let mut updates = RepoUpdates::default();
        let first = updates.begin(true, false).unwrap();
        assert!(updates.begin(true, false).is_none());
        assert!(updates.finish(first));
        assert!(updates.begin(true, false).is_some());
    }

    #[test]
    fn an_operation_invalidates_an_older_background_read() {
        let mut updates = RepoUpdates::default();
        let read = updates.begin(true, false).unwrap();
        let operation = updates.begin(false, true).unwrap();
        assert!(!updates.finish(read));
        assert!(updates.finish(operation));
    }

    #[test]
    fn a_superseded_read_cannot_clear_the_current_read() {
        let mut updates = RepoUpdates::default();
        let old = updates.begin(true, false).unwrap();
        let operation = updates.begin(false, true).unwrap();
        assert!(updates.finish(operation));
        let current = updates.begin(true, false).unwrap();
        assert!(!updates.finish(old));
        assert!(updates.begin(true, false).is_none());
        assert!(updates.finish(current));
    }

    #[test]
    fn explicit_navigation_supersedes_a_read_without_losing_its_completion() {
        let mut updates = RepoUpdates::default();
        let background = updates.begin(true, false).unwrap();
        let navigation = updates.begin(true, true).unwrap();
        assert!(!updates.finish(background));
        assert!(updates.begin(true, false).is_none());
        assert!(updates.finish(navigation));
    }
}
