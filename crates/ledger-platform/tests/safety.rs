//! Tests the promises in SAFETY.md on real files in temporary folders.
//!
//! Only these tests delete anything, and only the temporary folders they made.
#![allow(clippy::disallowed_methods)]
#![allow(missing_docs)]

use ledger_core::guard::{Blocked, Guard};
use ledger_core::judge::{Decision, HoldReason, Options, SkipReason};
use ledger_core::kind::Bin;
use ledger_core::ledger::{Status, Tally};
use ledger_core::memory::Memory;
use ledger_platform::journal::{Action, Journal, Recovered};
use ledger_platform::mover::{MoveError, move_to, put_back};
use ledger_platform::sort::{FolderBlocked, Sorted, check_folder, sort_item};
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::windows::ffi::OsStringExt;
use std::os::windows::fs::{OpenOptionsExt, symlink_dir};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

const PDF: &[u8] = b"%PDF-1.7 test";

static NEXT: AtomicU32 = AtomicU32::new(0);

/// A temporary folder holding a watched folder and a journal, removed at the end.
struct Lab {
    base: PathBuf,
    watched: PathBuf,
}

impl Lab {
    fn new() -> Lab {
        // Not the Windows temp folder: it sits under AppData, which the guard blocks.
        Lab::on(Path::new(env!("CARGO_TARGET_TMPDIR")))
    }

    fn on(parent: &Path) -> Lab {
        let id = NEXT.fetch_add(1, Ordering::SeqCst);
        let base = parent.join(format!("ofm-test-{}-{id}", std::process::id()));
        let watched = base.join("watched");
        fs::create_dir_all(&watched).unwrap();
        Lab { base, watched }
    }

    fn file(&self, name: &str, data: &[u8]) -> PathBuf {
        let path = self.watched.join(name);
        fs::write(&path, data).unwrap();
        path
    }

    fn journal_path(&self) -> PathBuf {
        self.base.join("ledger.journal")
    }

    fn journal(&self) -> Journal {
        Journal::open(&self.journal_path()).unwrap()
    }

    fn sort(&self, path: &Path, journal: &mut Journal) -> Sorted {
        sort_item(
            path,
            &self.watched,
            &Memory::default(),
            Options::default(),
            journal,
        )
        .unwrap()
    }
}

impl Drop for Lab {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn verdict(sorted: &Sorted) -> &ledger_core::judge::Verdict {
    match sorted {
        Sorted::Judged { verdict, .. } => verdict,
        Sorted::Unreadable(error) => panic!("unreadable: {error:?}"),
    }
}

// ---------- The core promises: nothing replaced, nothing lost ----------

#[test]
fn a_file_moves_with_its_contents_intact() {
    let lab = Lab::new();
    let mut journal = lab.journal();
    let src = lab.file("report.pdf", PDF);
    let sorted = lab.sort(&src, &mut journal);
    assert_eq!(sorted.status(), Status::Moved);
    assert!(!src.exists());
    assert_eq!(
        fs::read(lab.watched.join("pdf").join("report.pdf")).unwrap(),
        PDF
    );
}

#[test]
fn a_taken_name_is_never_replaced() {
    let lab = Lab::new();
    let mut journal = lab.journal();
    let dest = lab.watched.join("pdf");
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("report.pdf"), b"%PDF- the original").unwrap();
    fs::write(dest.join("report (1).pdf"), b"%PDF- first copy").unwrap();

    let moved = move_to(&lab.file("report.pdf", b"%PDF- new"), &dest, &mut journal).unwrap();

    assert!(moved.renamed);
    assert_eq!(
        fs::read(dest.join("report.pdf")).unwrap(),
        b"%PDF- the original"
    );
    assert_eq!(
        fs::read(dest.join("report (1).pdf")).unwrap(),
        b"%PDF- first copy"
    );
    assert_eq!(fs::read(dest.join("report (2).pdf")).unwrap(), b"%PDF- new");
}

#[test]
fn a_name_differing_only_by_letter_case_is_not_replaced() {
    let lab = Lab::new();
    let mut journal = lab.journal();
    let dest = lab.watched.join("pdf");
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("report.pdf"), b"original").unwrap();

    let moved = move_to(&lab.file("REPORT.pdf", b"new"), &dest, &mut journal).unwrap();

    assert_eq!(fs::read(dest.join("report.pdf")).unwrap(), b"original");
    assert_eq!(fs::read(&moved.to).unwrap(), b"new");
    assert!(moved.to.ends_with("REPORT (1).pdf"));
}

#[test]
fn exact_duplicates_are_kept() {
    let lab = Lab::new();
    let mut journal = lab.journal();
    let dest = lab.watched.join("pdf");
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("a.pdf"), PDF).unwrap();
    let moved = move_to(&lab.file("a.pdf", PDF), &dest, &mut journal).unwrap();
    assert!(dest.join("a.pdf").exists());
    assert!(moved.to.exists());
}

#[test]
fn many_files_with_one_name_all_survive() {
    let lab = Lab::new();
    let mut journal = lab.journal();
    let dest = lab.watched.join("doc");
    for i in 0..50 {
        let src = lab.file("same.txt", format!("copy {i}").as_bytes());
        move_to(&src, &dest, &mut journal).unwrap();
    }
    let mut seen: Vec<String> = fs::read_dir(&dest)
        .unwrap()
        .map(|e| fs::read_to_string(e.unwrap().path()).unwrap())
        .collect();
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), 50);
}

#[test]
fn another_drive_is_refused_and_the_file_stays() {
    // Runs only where a second fixed drive exists. Reports the skip rather than pretending.
    let Some(other) = other_drive() else {
        eprintln!("SKIPPED: no second drive on this machine");
        return;
    };
    let lab = Lab::new();
    let far = Lab::on(&other);
    let mut journal = lab.journal();
    let src = lab.file("big.pdf", PDF);

    let result = move_to(&src, &far.watched, &mut journal);

    assert!(matches!(result, Err(MoveError::OtherDrive)), "{result:?}");
    assert_eq!(fs::read(&src).unwrap(), PDF);
    assert_eq!(fs::read_dir(&far.watched).unwrap().count(), 0);
    assert_eq!(journal.pending(), 0);
}

fn other_drive() -> Option<PathBuf> {
    let current = env!("CARGO_TARGET_TMPDIR")
        .chars()
        .next()?
        .to_ascii_uppercase();
    ('C'..='H').filter(|d| *d != current).find_map(|d| {
        let root = PathBuf::from(format!("{d}:\\"));
        let probe = root.join(format!("ofm-probe-{}", std::process::id()));
        let writable = fs::create_dir(&probe).is_ok();
        let _ = fs::remove_dir(&probe);
        writable.then_some(root)
    })
}

// ---------- Failures leave the file alone ----------

#[test]
fn a_file_open_in_another_program_waits() {
    let lab = Lab::new();
    let mut journal = lab.journal();
    let src = lab.file("setup.exe", b"MZ program");
    let holder = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&src)
        .unwrap();

    let sorted = lab.sort(&src, &mut journal);
    assert_eq!(sorted.status(), Status::Wait);
    assert!(src.exists());

    drop(holder);
    assert_eq!(lab.sort(&src, &mut journal).status(), Status::Moved);
}

#[test]
fn a_file_being_written_is_not_moved() {
    let lab = Lab::new();
    let mut journal = lab.journal();
    let src = lab.file("download.pdf", PDF);
    let mut writer = OpenOptions::new()
        .append(true)
        .share_mode(1)
        .open(&src)
        .unwrap();
    writer.write_all(b"more").unwrap();
    assert_eq!(lab.sort(&src, &mut journal).status(), Status::Wait);
    assert!(src.exists());
}

#[test]
fn a_vanished_file_fails_cleanly() {
    let lab = Lab::new();
    let mut journal = lab.journal();
    let sorted = lab.sort(&lab.watched.join("never-existed.pdf"), &mut journal);
    assert_eq!(sorted.status(), Status::Fail);
    assert_eq!(journal.pending(), 0);
}

#[test]
fn a_read_only_file_moves_and_stays_read_only() {
    let lab = Lab::new();
    let mut journal = lab.journal();
    let src = lab.file("ro.pdf", PDF);
    let mut perms = fs::metadata(&src).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(&src, perms).unwrap();

    let sorted = lab.sort(&src, &mut journal);
    assert_eq!(sorted.status(), Status::Moved);
    let moved = lab.watched.join("pdf").join("ro.pdf");
    let mut perms = fs::metadata(&moved).unwrap().permissions();
    assert!(perms.readonly());
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    fs::set_permissions(&moved, perms).unwrap();
}

#[test]
fn a_missing_destination_folder_is_recreated() {
    let lab = Lab::new();
    let mut journal = lab.journal();
    let src = lab.file("a.pdf", PDF);
    assert!(!lab.watched.join("pdf").exists());
    assert_eq!(lab.sort(&src, &mut journal).status(), Status::Moved);
    assert!(lab.watched.join("pdf").join("a.pdf").exists());
}

// ---------- Hard names and paths ----------

#[test]
fn arabic_emoji_and_symbol_names_survive() {
    let lab = Lab::new();
    let mut journal = lab.journal();
    for name in [
        "تقرير المبيعات ٢٠٢٦.pdf",
        "🎉 party 🎉.pdf",
        "my file & (draft) #2!.pdf",
        "二零二六.pdf",
    ] {
        let src = lab.file(name, PDF);
        assert_eq!(
            lab.sort(&src, &mut journal).status(),
            Status::Moved,
            "{name}"
        );
        assert_eq!(
            fs::read(lab.watched.join("pdf").join(name)).unwrap(),
            PDF,
            "{name}"
        );
    }
}

#[test]
fn a_full_length_name_that_clashes_still_moves() {
    let lab = Lab::new();
    let mut journal = lab.journal();
    let name = format!("{}.pdf", "y".repeat(251));
    let dest = lab.watched.join("pdf");
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join(&name), b"original").unwrap();

    let moved = move_to(&lab.file(&name, b"new"), &dest, &mut journal).unwrap();

    assert_eq!(fs::read(dest.join(&name)).unwrap(), b"original");
    assert_eq!(fs::read(&moved.to).unwrap(), b"new");
}

#[test]
fn paths_over_260_characters_work() {
    let lab = Lab::new();
    let mut journal = lab.journal();
    let src = lab.file("deep.pdf", PDF);
    let canon = fs::canonicalize(&lab.watched).unwrap();
    let mut deep = canon;
    for _ in 0..8 {
        deep.push("a-rather-long-folder-name-to-pass-the-old-limit");
    }
    assert!(deep.to_string_lossy().len() > 300);

    let moved = move_to(&src, &deep, &mut journal).unwrap();
    assert_eq!(fs::read(&moved.to).unwrap(), PDF);

    let back = put_back(moved.seq, &mut journal).unwrap();
    assert_eq!(fs::read(&back.to).unwrap(), PDF);
}

// ---------- Things that must never move ----------

#[test]
fn shortcuts_and_unfinished_downloads_stay() {
    let lab = Lab::new();
    let mut journal = lab.journal();
    for name in [
        "Game.lnk",
        "Elden Ring.url",
        "movie.mp4.crdownload",
        "movie.mp4.part",
        "~$budget.xlsx",
    ] {
        let src = lab.file(name, b"x");
        assert_eq!(
            lab.sort(&src, &mut journal).status(),
            Status::Skip,
            "{name}"
        );
        assert!(src.exists(), "{name}");
    }
}

#[test]
fn a_disguised_program_is_held_where_it_is() {
    let lab = Lab::new();
    let mut journal = lab.journal();
    let src = lab.file("invoice.pdf", b"MZ this is a program");
    let sorted = lab.sort(&src, &mut journal);
    assert_eq!(
        verdict(&sorted).decision,
        Decision::Hold(HoldReason::DisguisedProgram)
    );
    assert!(src.exists());
}

#[test]
fn hidden_files_stay() {
    let lab = Lab::new();
    let mut journal = lab.journal();
    let src = lab.file("secret.txt", b"x");
    let status = std::process::Command::new("attrib")
        .arg("+h")
        .arg(&src)
        .status()
        .unwrap();
    assert!(status.success());
    let sorted = lab.sort(&src, &mut journal);
    assert_eq!(
        verdict(&sorted).decision,
        Decision::Skip(SkipReason::Hidden)
    );
    assert!(src.exists());
}

#[test]
fn folders_stay_by_default() {
    let lab = Lab::new();
    let mut journal = lab.journal();
    let folder = lab.watched.join("Project");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("keep.txt"), b"x").unwrap();
    assert_eq!(lab.sort(&folder, &mut journal).status(), Status::Skip);
    assert!(folder.join("keep.txt").exists());
}

#[test]
fn a_link_is_never_moved() {
    let lab = Lab::new();
    let mut journal = lab.journal();
    let target = lab.base.join("target");
    fs::create_dir(&target).unwrap();
    let link = lab.watched.join("link");
    // Creating links can need a Windows permission. Use a junction, which does not.
    let made = symlink_dir(&target, &link).is_ok()
        || std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&target)
            .output()
            .is_ok_and(|o| o.status.success());
    assert!(made, "could not create a link or junction");

    let sorted = sort_item(
        &link,
        &lab.watched,
        &Memory::default(),
        Options { move_folders: true },
        &mut journal,
    )
    .unwrap();
    assert_eq!(verdict(&sorted).decision, Decision::Skip(SkipReason::Link));
    assert!(fs::symlink_metadata(&link).is_ok());
    let _ = fs::remove_dir(&link);
}

// ---------- The folder guard on a real path ----------

#[test]
fn a_folder_holding_game_files_is_refused_and_nothing_moves() {
    let lab = Lab::new();
    let mut journal = lab.journal();
    lab.file("steam_api64.dll", b"MZ");
    let src = lab.file("a.pdf", PDF);

    assert_eq!(check_folder(&lab.watched), Guard::Block(Blocked::Games));
    let result = sort_item(
        &src,
        &lab.watched,
        &Memory::default(),
        Options::default(),
        &mut journal,
    );

    assert!(matches!(result, Err(FolderBlocked(Blocked::Games))));
    assert!(src.exists());
}

#[test]
fn a_canonical_path_to_windows_is_blocked() {
    let windows = fs::canonicalize(std::env::var_os("WINDIR").unwrap()).unwrap();
    assert!(windows.to_string_lossy().starts_with(r"\\?\"));
    assert_eq!(check_folder(&windows), Guard::Block(Blocked::System));
}

// ---------- Putting back ----------

#[test]
fn void_puts_the_file_back() {
    let lab = Lab::new();
    let mut journal = lab.journal();
    let src = lab.file("a.pdf", PDF);
    let moved = move_to(&src, &lab.watched.join("pdf"), &mut journal).unwrap();

    put_back(moved.seq, &mut journal).unwrap();

    assert_eq!(fs::read(&src).unwrap(), PDF);
    assert!(!moved.to.exists());
}

#[test]
fn voiding_twice_does_nothing_extra() {
    let lab = Lab::new();
    let mut journal = lab.journal();
    let src = lab.file("a.pdf", PDF);
    let moved = move_to(&src, &lab.watched.join("pdf"), &mut journal).unwrap();
    put_back(moved.seq, &mut journal).unwrap();
    assert!(matches!(
        put_back(moved.seq, &mut journal),
        Err(MoveError::AlreadyUndone)
    ));
    assert_eq!(fs::read(&src).unwrap(), PDF);
}

#[test]
fn void_never_replaces_a_new_file_in_the_original_spot() {
    let lab = Lab::new();
    let mut journal = lab.journal();
    let src = lab.file("a.pdf", b"first");
    let moved = move_to(&src, &lab.watched.join("pdf"), &mut journal).unwrap();
    fs::write(&src, b"second, newer").unwrap();

    let back = put_back(moved.seq, &mut journal).unwrap();

    assert_eq!(fs::read(&src).unwrap(), b"second, newer");
    assert!(back.to.ends_with("a (1).pdf"));
    assert_eq!(fs::read(&back.to).unwrap(), b"first");
}

#[test]
fn void_of_a_deleted_file_is_refused_with_a_reason() {
    let lab = Lab::new();
    let mut journal = lab.journal();
    let moved = move_to(
        &lab.file("a.pdf", PDF),
        &lab.watched.join("pdf"),
        &mut journal,
    )
    .unwrap();
    fs::remove_file(&moved.to).unwrap();
    assert!(matches!(
        put_back(moved.seq, &mut journal),
        Err(MoveError::Gone)
    ));
    assert!(!journal.is_undone(moved.seq));
}

#[test]
fn void_recreates_a_removed_original_folder() {
    let lab = Lab::new();
    let mut journal = lab.journal();
    let sub = lab.watched.join("sub");
    fs::create_dir(&sub).unwrap();
    let src = sub.join("a.pdf");
    fs::write(&src, PDF).unwrap();
    let moved = move_to(&src, &lab.watched.join("pdf"), &mut journal).unwrap();
    fs::remove_dir(&sub).unwrap();

    put_back(moved.seq, &mut journal).unwrap();

    assert_eq!(fs::read(&src).unwrap(), PDF);
}

#[test]
fn a_void_can_be_undone_after_restarting_the_app() {
    let lab = Lab::new();
    let src = lab.file("a.pdf", PDF);
    let seq = {
        let mut journal = lab.journal();
        move_to(&src, &lab.watched.join("pdf"), &mut journal)
            .unwrap()
            .seq
    };
    let mut reopened = lab.journal();
    put_back(seq, &mut reopened).unwrap();
    assert_eq!(fs::read(&src).unwrap(), PDF);

    let mut again = lab.journal();
    assert!(again.is_undone(seq));
    assert!(matches!(
        put_back(seq, &mut again),
        Err(MoveError::AlreadyUndone)
    ));
}

// ---------- Crashes ----------

/// Writes the journal as it would stand if the power died straight after the intent.
fn crash_after_intent(lab: &Lab, from: &Path, to: &Path) {
    let mut journal = lab.journal();
    journal.intend(Action::Move, from, to).unwrap();
}

#[test]
fn crash_before_the_rename_finds_the_file_unmoved() {
    let lab = Lab::new();
    let src = lab.file("a.pdf", PDF);
    let to = lab.watched.join("pdf").join("a.pdf");
    crash_after_intent(&lab, &src, &to);

    let mut journal = lab.journal();
    assert_eq!(journal.pending(), 1);
    let found = journal.recover().unwrap();

    assert_eq!(found[0].1, Recovered::NotMoved);
    assert_eq!(fs::read(&src).unwrap(), PDF);
    assert_eq!(lab.journal().pending(), 0);
}

#[test]
fn crash_after_the_rename_finds_the_file_moved_and_undoable() {
    let lab = Lab::new();
    let src = lab.file("a.pdf", PDF);
    let dir = lab.watched.join("pdf");
    fs::create_dir(&dir).unwrap();
    let to = dir.join("a.pdf");
    crash_after_intent(&lab, &src, &to);
    fs::rename(&src, &to).unwrap();

    let mut journal = lab.journal();
    let found = journal.recover().unwrap();

    assert_eq!(found[0].1, Recovered::Completed);
    put_back(found[0].0.seq, &mut journal).unwrap();
    assert_eq!(fs::read(&src).unwrap(), PDF);
}

#[test]
fn crash_with_both_files_present_keeps_both_and_claims_no_move() {
    let lab = Lab::new();
    let src = lab.file("a.pdf", b"source");
    let dir = lab.watched.join("pdf");
    fs::create_dir(&dir).unwrap();
    let to = dir.join("a.pdf");
    fs::write(&to, b"someone else's file").unwrap();
    crash_after_intent(&lab, &src, &to);

    let found = lab.journal().recover().unwrap();

    assert_eq!(found[0].1, Recovered::NotMoved);
    assert_eq!(fs::read(&src).unwrap(), b"source");
    assert_eq!(fs::read(&to).unwrap(), b"someone else's file");
}

#[test]
fn a_file_missing_from_both_places_is_reported() {
    let lab = Lab::new();
    let src = lab.file("a.pdf", PDF);
    let to = lab.watched.join("pdf").join("a.pdf");
    crash_after_intent(&lab, &src, &to);
    fs::remove_file(&src).unwrap();
    let found = lab.journal().recover().unwrap();
    assert_eq!(found[0].1, Recovered::Missing);
}

#[test]
fn a_half_written_journal_line_is_ignored_and_later_writes_are_safe() {
    let lab = Lab::new();
    let src = lab.file("a.pdf", PDF);
    let seq = {
        let mut journal = lab.journal();
        move_to(&src, &lab.watched.join("pdf"), &mut journal)
            .unwrap()
            .seq
    };
    let mut raw = OpenOptions::new()
        .append(true)
        .open(lab.journal_path())
        .unwrap();
    raw.write_all(b"v1\t9\tmove\tC:\\half").unwrap();
    drop(raw);

    let mut journal = lab.journal();
    assert_eq!(journal.skipped_lines(), 1);
    assert_eq!(journal.pending(), 0);
    let src2 = lab.file("b.pdf", PDF);
    let second = move_to(&src2, &lab.watched.join("pdf"), &mut journal).unwrap();
    drop(journal);

    let mut reopened = lab.journal();
    assert_eq!(reopened.skipped_lines(), 1);
    assert!(reopened.get(seq).is_some());
    assert!(reopened.get(second.seq).is_some());
    put_back(second.seq, &mut reopened).unwrap();
    assert_eq!(fs::read(&src2).unwrap(), PDF);
}

#[test]
fn no_move_starts_if_the_journal_cannot_be_written() {
    let lab = Lab::new();
    let mut journal = lab.journal();
    let src = lab.file("a.pdf", PDF);
    // A folder name that is not valid Unicode makes the journal refuse the intent.
    let bad_folder = lab.watched.join(OsString::from_wide(&[0x62, 0xD800]));
    fs::create_dir(&bad_folder).unwrap();

    let result = move_to(&src, &bad_folder, &mut journal);

    assert!(matches!(result, Err(MoveError::Journal(_))), "{result:?}");
    assert_eq!(fs::read(&src).unwrap(), PDF);
    assert_eq!(fs::read_dir(&bad_folder).unwrap().count(), 0);
}

#[test]
fn a_journal_that_cannot_be_opened_is_an_error() {
    let lab = Lab::new();
    let path = lab.journal_path();
    drop(lab.journal());
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(&path, perms).unwrap();

    assert!(Journal::open(&path).is_err());

    let mut perms = fs::metadata(&path).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    fs::set_permissions(&path, perms).unwrap();
}

#[test]
fn a_file_with_an_unreadable_name_stays() {
    let lab = Lab::new();
    let mut journal = lab.journal();
    let src = lab.watched.join(OsString::from_wide(&[0x62, 0xD800]));
    fs::write(&src, PDF).unwrap();

    assert_eq!(lab.sort(&src, &mut journal).status(), Status::Fail);
    assert!(matches!(
        move_to(&src, &lab.watched.join("pdf"), &mut journal),
        Err(MoveError::UnreadableName)
    ));
    assert_eq!(fs::read(&src).unwrap(), PDF);
}

// ---------- Real download details ----------

#[test]
fn the_source_website_is_read_and_survives_the_move() {
    let lab = Lab::new();
    let mut journal = lab.journal();
    let src = lab.file("a.pdf", PDF);
    let mut stream = src.as_os_str().to_owned();
    stream.push(":Zone.Identifier");
    fs::write(
        &stream,
        "[ZoneTransfer]\r\nZoneId=3\r\nHostUrl=https://www.bank.com/a.pdf\r\n",
    )
    .unwrap();

    let sorted = lab.sort(&src, &mut journal);
    assert!(verdict(&sorted).evidence.iter().any(|e| matches!(
        e,
        ledger_core::judge::Evidence::Site(h) if h == "bank.com"
    )));
    let mut moved_stream = lab.watched.join("pdf").join("a.pdf").into_os_string();
    moved_stream.push(":Zone.Identifier");
    assert!(
        fs::read_to_string(&moved_stream)
            .unwrap()
            .contains("bank.com")
    );
}

#[test]
fn a_void_teaches_and_a_choice_clears_it() {
    let lab = Lab::new();
    let mut journal = lab.journal();
    let mut memory = Memory::default();
    memory.voided("bank.com", Bin::Pdf);
    let src = lab.file("b.pdf", PDF);
    let mut stream = src.as_os_str().to_owned();
    stream.push(":Zone.Identifier");
    fs::write(&stream, "HostUrl=https://bank.com/b.pdf").unwrap();

    let held = sort_item(
        &src,
        &lab.watched,
        &memory,
        Options::default(),
        &mut journal,
    )
    .unwrap();
    assert_eq!(held.status(), Status::Held);
    assert!(src.exists());

    memory.chose("bank.com", Bin::Pdf, Bin::Doc);
    let sorted = sort_item(
        &src,
        &lab.watched,
        &memory,
        Options::default(),
        &mut journal,
    )
    .unwrap();
    assert_eq!(sorted.status(), Status::Moved);
    assert!(lab.watched.join("doc").join("b.pdf").exists());
}

// ---------- The whole flow ----------

#[test]
fn every_item_is_accounted_for_and_no_byte_is_lost() {
    let lab = Lab::new();
    let mut journal = lab.journal();
    let items: &[(&str, &[u8])] = &[
        ("a.pdf", PDF),
        ("a (copy).pdf", PDF),
        ("photo.png", b"\x89PNG\r\n\x1a\n...."),
        ("clip.mp4", b"\0\0\0\x18ftypisom...."),
        ("song.mp3", b"ID3\x04...."),
        ("pack.zip", b"PK\x03\x04...."),
        ("letter.docx", b"PK\x03\x04...."),
        ("setup.exe", b"MZ...."),
        ("notes.txt", b"plain words"),
        ("mystery.dat", b"????"),
        ("empty.txt", b""),
        ("تقرير.pdf", PDF),
        ("invoice.pdf.exe", b"MZ...."),
        ("fake.jpg", b"MZ...."),
        ("Game.lnk", b"x"),
        ("movie.mp4.part", b"x"),
        ("mismatch.pdf", b"\x89PNG\r\n\x1a\n...."),
    ];
    let bytes_before: usize = items.iter().map(|(_, data)| data.len()).sum();
    let mut tally = Tally::default();
    for (name, data) in items {
        let path = lab.file(name, data);
        tally.add(lab.sort(&path, &mut journal).status());
    }
    assert!(tally.balanced());
    assert_eq!(tally.seen, items.len() as u64);
    assert_eq!((tally.moved, tally.held, tally.skip), (12, 3, 2));

    fn walk(dir: &Path, files: &mut u64, bytes: &mut u64) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            if entry.path().is_dir() {
                walk(&entry.path(), files, bytes);
            } else {
                *files += 1;
                *bytes += entry.metadata().unwrap().len();
            }
        }
    }
    let (mut files, mut bytes) = (0, 0);
    walk(&lab.watched, &mut files, &mut bytes);
    assert_eq!(files, items.len() as u64);
    assert_eq!(bytes, bytes_before as u64);
}
