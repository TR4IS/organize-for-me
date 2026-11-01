//! Takes one item from "just appeared" to "accounted for".

use crate::inspect::{InspectError, has_game_files, inspect};
use crate::journal::Journal;
use crate::mover::{MoveError, Moved, move_to};
use ledger_core::guard::{Guard, guard_folder};
use ledger_core::judge::{Decision, Options, Verdict, judge};
use ledger_core::ledger::Status;
use ledger_core::memory::Memory;
use std::path::Path;

/// A folder cannot be sorted because the guard blocks it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FolderBlocked(pub ledger_core::guard::Blocked);

/// Asks the guard about a folder, including a look inside it for game files.
pub fn check_folder(folder: &Path) -> Guard {
    guard_folder(&folder.to_string_lossy(), has_game_files(folder))
}

/// What happened to one item.
#[derive(Debug)]
pub enum Sorted {
    /// It was judged, and any allowed move ran.
    Judged {
        /// The decision and its evidence.
        verdict: Verdict,
        /// The move, if one ran and succeeded.
        moved: Option<Moved>,
        /// Why the move failed, if it did.
        failure: Option<MoveError>,
    },
    /// It could not even be read, so it stayed where it was.
    Unreadable(InspectError),
}

impl Sorted {
    /// The item's status on the receipt.
    pub fn status(&self) -> Status {
        match self {
            Sorted::Unreadable(_) => Status::Fail,
            Sorted::Judged {
                verdict,
                moved,
                failure,
            } => match (verdict.decision, moved, failure) {
                (_, _, Some(_)) => Status::Fail,
                (Decision::Move, Some(_), None) => Status::Moved,
                (Decision::Move, None, None) => Status::Fail,
                (Decision::Hold(_), ..) => Status::Held,
                (Decision::Wait, ..) => Status::Wait,
                (Decision::Skip(_), ..) => Status::Skip,
            },
        }
    }
}

/// Judges one item in a watched folder and carries out the decision.
///
/// The whole folder is checked again first, so a guard rule also protects a
/// folder that was chosen before the rule existed.
pub fn sort_item(
    item: &Path,
    root: &Path,
    memory: &Memory,
    options: Options,
    journal: &mut Journal,
) -> Result<Sorted, FolderBlocked> {
    if let Guard::Block(why) = check_folder(root) {
        return Err(FolderBlocked(why));
    }
    let facts = match inspect(item) {
        Ok(facts) => facts,
        Err(error) => return Ok(Sorted::Unreadable(error)),
    };
    let verdict = judge(&facts, memory, options);
    if verdict.decision != Decision::Move {
        return Ok(Sorted::Judged {
            verdict,
            moved: None,
            failure: None,
        });
    }
    let dest = root.join(verdict.bin.folder());
    Ok(match move_to(item, &dest, journal) {
        Ok(moved) => Sorted::Judged {
            verdict,
            moved: Some(moved),
            failure: None,
        },
        Err(error) => Sorted::Judged {
            verdict,
            moved: None,
            failure: Some(error),
        },
    })
}
