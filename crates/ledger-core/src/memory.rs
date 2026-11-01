//! What the app learned from the user's voids and choices, per website.

use crate::kind::Bin;
use std::collections::BTreeMap;

/// What to do with the next file from a website.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Learned {
    /// The user voided a move, so ask them next time.
    Ask,
    /// The user chose a folder, so use it next time.
    RouteTo(Bin),
}

/// Learned choices, keyed by website and the folder the evidence pointed to.
#[derive(Clone, Debug, Default)]
pub struct Memory {
    entries: BTreeMap<(String, Bin), Learned>,
}

impl Memory {
    /// Looks up what was learned for files of this type from this website.
    pub fn get(&self, host: &str, bin: Bin) -> Option<Learned> {
        self.entries.get(&(host.to_lowercase(), bin)).copied()
    }

    /// Records that the user undid a move, which also replaces any earlier choice.
    pub fn voided(&mut self, host: &str, bin: Bin) {
        self.entries
            .insert((host.to_lowercase(), bin), Learned::Ask);
    }

    /// Records the user's choice for a file whose evidence pointed to `from`.
    pub fn chose(&mut self, host: &str, from: Bin, to: Bin) {
        self.entries
            .insert((host.to_lowercase(), from), Learned::RouteTo(to));
    }

    /// Forgets everything learned. Files are not touched.
    pub fn forget(&mut self) {
        self.entries.clear();
    }

    /// Number of saved choices.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when nothing has been learned.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
