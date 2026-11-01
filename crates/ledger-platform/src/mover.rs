//! The one place that moves files.
//!
//! It calls `MoveFileExW` with no flags, and that is the whole safety design:
//! - Without `MOVEFILE_REPLACE_EXISTING`, Windows refuses a name that is taken, even
//!   if another program took it a moment ago.
//! - Without `MOVEFILE_COPY_ALLOWED`, Windows refuses another drive, so the file is
//!   never copied and then deleted.
//!
//! The move is a single rename, so a crash leaves the file in one place or the other.

use crate::journal::{Action, Journal};
use ledger_core::name::name_candidates;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
}

const NO_FLAGS: u32 = 0;

/// Why a move did not happen. In every case the file stays where it was.
#[derive(Debug)]
pub enum MoveError {
    /// The file is no longer there.
    Gone,
    /// Another program has the file open.
    InUse,
    /// Windows denied access.
    Denied,
    /// The destination is on another drive, and the app never copies across drives.
    OtherDrive,
    /// The drive is full.
    DiskFull,
    /// Links are never moved, because they can point anywhere.
    IsLink,
    /// The name is not valid Unicode.
    UnreadableName,
    /// Every candidate name was taken.
    NoFreeName,
    /// This move was already put back.
    AlreadyUndone,
    /// The journal has no finished move with this number.
    UnknownMove,
    /// The journal could not be written, so the move was not attempted.
    Journal(io::Error),
    /// Windows reported something else.
    Other(io::Error),
}

/// A finished move.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Moved {
    /// Its number in the journal, used to put it back.
    pub seq: u64,
    /// The full path of the file now.
    pub to: PathBuf,
    /// True when the name was taken and the file got a new one.
    pub renamed: bool,
}

/// Moves an item into a folder, creating the folder if needed.
///
/// The name changes only if it is taken, like `report (1).pdf`.
pub fn move_to(src: &Path, dest_dir: &Path, journal: &mut Journal) -> Result<Moved, MoveError> {
    let meta = fs::symlink_metadata(src).map_err(classify)?;
    if meta.file_type().is_symlink() {
        return Err(MoveError::IsLink);
    }
    let name = src
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or(MoveError::UnreadableName)?;
    let from = fs::canonicalize(src).map_err(classify)?;
    fs::create_dir_all(dest_dir).map_err(classify)?;
    let dest_dir = fs::canonicalize(dest_dir).map_err(classify)?;
    rename_into_free_name(&from, &dest_dir, name, Action::Move, journal)
}

/// Puts a finished move back in its original folder.
///
/// If another file now has the original name, this file comes back as `name (1)`.
pub fn put_back(seq: u64, journal: &mut Journal) -> Result<Moved, MoveError> {
    let record = journal.get(seq).ok_or(MoveError::UnknownMove)?;
    if journal.is_undone(seq) {
        return Err(MoveError::AlreadyUndone);
    }
    if record.action != Action::Move {
        return Err(MoveError::UnknownMove);
    }
    let (now, original) = (record.to.clone(), record.from.clone());
    let name = original
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or(MoveError::UnreadableName)?;
    let folder = original.parent().ok_or(MoveError::UnknownMove)?;
    fs::symlink_metadata(&now).map_err(classify)?;
    fs::create_dir_all(folder).map_err(classify)?;
    rename_into_free_name(&now, folder, name, Action::Undo(seq), journal)
}

fn rename_into_free_name(
    from: &Path,
    dir: &Path,
    name: &str,
    action: Action,
    journal: &mut Journal,
) -> Result<Moved, MoveError> {
    for candidate in name_candidates(name) {
        let to = dir.join(&candidate);
        let seq = journal
            .intend(action, from, &to)
            .map_err(MoveError::Journal)?;
        match rename_no_replace(from, &to) {
            Ok(()) => {
                // The file has moved. If this write fails, recovery finds the file at
                // its target on the next start, so the failure is reported, not hidden.
                journal.done(seq).map_err(MoveError::Journal)?;
                return Ok(Moved {
                    seq,
                    to,
                    renamed: candidate != name,
                });
            }
            Err(error) => {
                journal.abort(seq).map_err(MoveError::Journal)?;
                if !is_name_taken(&error) {
                    return Err(classify(error));
                }
            }
        }
    }
    Err(MoveError::NoFreeName)
}

/// Renames on the same drive, and fails if the target exists.
fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    let from = wide(from);
    let to = wide(to);
    // SAFETY: both pointers are valid, NUL-terminated UTF-16 buffers that outlive the call.
    let ok = unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), NO_FLAGS) };
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn is_name_taken(error: &io::Error) -> bool {
    // ERROR_FILE_EXISTS and ERROR_ALREADY_EXISTS.
    matches!(error.raw_os_error(), Some(80 | 183))
}

fn classify(error: io::Error) -> MoveError {
    match error.raw_os_error() {
        Some(2 | 3) => MoveError::Gone,
        Some(5) => MoveError::Denied,
        Some(17) => MoveError::OtherDrive,
        Some(32 | 33) => MoveError::InUse,
        Some(39 | 112) => MoveError::DiskFull,
        _ => MoveError::Other(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_errors_map_to_clear_reasons() {
        let check = |code, want: fn(&MoveError) -> bool| {
            assert!(
                want(&classify(io::Error::from_raw_os_error(code))),
                "{code}"
            );
        };
        check(2, |e| matches!(e, MoveError::Gone));
        check(3, |e| matches!(e, MoveError::Gone));
        check(5, |e| matches!(e, MoveError::Denied));
        check(17, |e| matches!(e, MoveError::OtherDrive));
        check(32, |e| matches!(e, MoveError::InUse));
        check(112, |e| matches!(e, MoveError::DiskFull));
        check(1234, |e| matches!(e, MoveError::Other(_)));
    }
}
