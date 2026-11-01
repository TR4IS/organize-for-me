//! The receipt totals. Every file that comes in must be accounted for.

/// The final state of one item on the receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Moved to a folder.
    Moved,
    /// Moved, then put back by the user.
    Void,
    /// Waiting for the user to decide.
    Held,
    /// In use, and will be retried.
    Wait,
    /// Deliberately left alone.
    Skip,
    /// The move was refused or failed, and the file stayed where it was.
    Fail,
}

/// Running totals for a receipt.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    /// Items that came in.
    pub seen: u64,
    /// Items moved.
    pub moved: u64,
    /// Items voided.
    pub void: u64,
    /// Items held.
    pub held: u64,
    /// Items waiting.
    pub wait: u64,
    /// Items left alone.
    pub skip: u64,
    /// Items that failed.
    pub fail: u64,
}

impl Tally {
    /// Records a new item.
    pub fn add(&mut self, status: Status) {
        self.seen += 1;
        *self.slot(status) += 1;
    }

    /// Moves an existing item to a new status, such as a void or a retry.
    pub fn change(&mut self, from: Status, to: Status) {
        let old = self.slot(from);
        *old = old.saturating_sub(1);
        *self.slot(to) += 1;
    }

    /// Everything accounted for.
    pub fn out(&self) -> u64 {
        self.moved + self.void + self.held + self.wait + self.skip + self.fail
    }

    /// True when in equals out.
    pub fn balanced(&self) -> bool {
        self.seen == self.out()
    }

    fn slot(&mut self, status: Status) -> &mut u64 {
        match status {
            Status::Moved => &mut self.moved,
            Status::Void => &mut self.void,
            Status::Held => &mut self.held,
            Status::Wait => &mut self.wait,
            Status::Skip => &mut self.skip,
            Status::Fail => &mut self.fail,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balances_after_every_kind_of_outcome() {
        let mut tally = Tally::default();
        for status in [
            Status::Moved,
            Status::Moved,
            Status::Held,
            Status::Wait,
            Status::Skip,
            Status::Fail,
        ] {
            tally.add(status);
        }
        tally.change(Status::Moved, Status::Void);
        assert!(tally.balanced());
        assert_eq!(tally.out(), 6);
        assert_eq!((tally.moved, tally.void), (1, 1));
    }
}
