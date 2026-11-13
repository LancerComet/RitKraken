/// Coalesces background reads and rejects their stale results when an explicit
/// Git operation starts. Background reads never own the visible busy state.
#[derive(Default)]
pub(in crate::app) struct RepoUpdates {
    generation: u64,
    reading: bool,
}

impl RepoUpdates {
    pub(super) fn begin(&mut self, read_only: bool) -> Option<u64> {
        if read_only && self.reading {
            return None;
        }
        self.generation = self.generation.wrapping_add(1);
        self.reading = read_only;
        Some(self.generation)
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
        let first = updates.begin(true).unwrap();
        assert!(updates.begin(true).is_none());
        assert!(updates.finish(first));
        assert!(updates.begin(true).is_some());
    }

    #[test]
    fn an_operation_invalidates_an_older_background_read() {
        let mut updates = RepoUpdates::default();
        let read = updates.begin(true).unwrap();
        let operation = updates.begin(false).unwrap();
        assert!(!updates.finish(read));
        assert!(updates.finish(operation));
    }

    #[test]
    fn a_superseded_read_cannot_clear_the_current_read() {
        let mut updates = RepoUpdates::default();
        let old = updates.begin(true).unwrap();
        let operation = updates.begin(false).unwrap();
        assert!(updates.finish(operation));
        let current = updates.begin(true).unwrap();
        assert!(!updates.finish(old));
        assert!(updates.begin(true).is_none());
        assert!(updates.finish(current));
    }
}
