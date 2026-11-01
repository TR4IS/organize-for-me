//! The only part of Organize for Me that touches files.
//!
//! `mover` is the one place that can change anything on disk, and it can only
//! rename within a drive without replacing. See `SAFETY.md` for the promises this
//! crate makes and the tests that hold it to them.

#[cfg(not(windows))]
compile_error!(
    "Only Windows is supported so far. A new system needs its own no-replace move, \
     shortcut rules and folder guard before it is safe. See SAFETY.md."
);

pub mod inspect;
pub mod journal;
pub mod mover;
pub mod sort;
