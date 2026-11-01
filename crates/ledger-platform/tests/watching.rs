//! Tests the folder watcher on real files, with a fake clock so nothing has to wait.
#![allow(clippy::disallowed_methods)]
#![allow(missing_docs)]

use ledger_platform::watch::Watcher;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

const SETTLE: u64 = 2000;
static NEXT: AtomicU32 = AtomicU32::new(0);

struct Lab(PathBuf);

impl Lab {
    fn new() -> Lab {
        let id = NEXT.fetch_add(1, Ordering::SeqCst);
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("ofm-watch-{}-{id}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        Lab(dir)
    }

    fn file(&self, name: &str, data: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, data).unwrap();
        path
    }
}

impl Drop for Lab {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn names(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn files_already_in_the_folder_are_never_reported() {
    let lab = Lab::new();
    lab.file("old.pdf", b"x");
    fs::create_dir(lab.0.join("old folder")).unwrap();
    let mut watcher = Watcher::new(&lab.0, SETTLE).unwrap();
    for now in [0, 5_000, 1_000_000] {
        assert!(watcher.tick(now).unwrap().is_empty());
    }
}

#[test]
fn a_new_file_is_reported_once_after_it_settles() {
    let lab = Lab::new();
    let mut watcher = Watcher::new(&lab.0, SETTLE).unwrap();
    lab.file("new.pdf", b"x");
    assert!(watcher.tick(0).unwrap().is_empty());
    assert!(watcher.tick(1_999).unwrap().is_empty());
    assert_eq!(names(&watcher.tick(2_000).unwrap()), ["new.pdf"]);
    assert!(watcher.tick(3_000).unwrap().is_empty());
}

#[test]
fn a_growing_file_is_not_reported_until_it_stops() {
    let lab = Lab::new();
    let mut watcher = Watcher::new(&lab.0, SETTLE).unwrap();
    lab.file("grow.mp4", b"1");
    watcher.tick(0).unwrap();
    lab.file("grow.mp4", b"12");
    assert!(watcher.tick(2_000).unwrap().is_empty());
    lab.file("grow.mp4", b"123");
    assert!(watcher.tick(3_999).unwrap().is_empty());
    assert!(watcher.tick(5_000).unwrap().is_empty());
    assert_eq!(names(&watcher.tick(5_999).unwrap()), ["grow.mp4"]);
}

#[test]
fn the_placeholder_of_a_running_download_waits_for_it() {
    for suffix in [".part", ".crdownload"] {
        let lab = Lab::new();
        let mut watcher = Watcher::new(&lab.0, SETTLE).unwrap();
        lab.file("movie.mp4", b"");
        let partner = lab.file(&format!("movie.mp4{suffix}"), b"data");
        watcher.tick(0).unwrap();
        let during = names(&watcher.tick(9_000).unwrap());
        assert!(!during.contains(&"movie.mp4".to_string()), "{suffix}");

        fs::remove_file(partner).unwrap();
        lab.file("movie.mp4", b"finished");
        watcher.tick(10_000).unwrap();
        assert_eq!(
            names(&watcher.tick(12_000).unwrap()),
            ["movie.mp4"],
            "{suffix}"
        );
    }
}

#[test]
fn a_file_put_back_is_not_sorted_again() {
    let lab = Lab::new();
    let mut watcher = Watcher::new(&lab.0, SETTLE).unwrap();
    let path = lab.file("a.pdf", b"x");
    watcher.accept(&path);
    assert!(watcher.tick(0).unwrap().is_empty());
    assert!(watcher.tick(9_000).unwrap().is_empty());
}

#[test]
fn a_same_name_download_later_counts_as_new() {
    let lab = Lab::new();
    let mut watcher = Watcher::new(&lab.0, SETTLE).unwrap();
    let path = lab.file("a.pdf", b"x");
    watcher.tick(0).unwrap();
    watcher.tick(2_000).unwrap();
    fs::remove_file(&path).unwrap();
    watcher.tick(3_000).unwrap();
    lab.file("a.pdf", b"y");
    watcher.tick(4_000).unwrap();
    assert_eq!(names(&watcher.tick(6_000).unwrap()), ["a.pdf"]);
}

#[test]
fn rebaseline_forgets_what_arrived_during_a_pause() {
    let lab = Lab::new();
    let mut watcher = Watcher::new(&lab.0, SETTLE).unwrap();
    lab.file("while-paused.pdf", b"x");
    watcher.rebaseline().unwrap();
    assert!(watcher.tick(0).unwrap().is_empty());
    assert!(watcher.tick(9_000).unwrap().is_empty());
}

#[test]
fn a_burst_of_files_is_all_reported() {
    let lab = Lab::new();
    let mut watcher = Watcher::new(&lab.0, SETTLE).unwrap();
    for i in 0..500 {
        lab.file(&format!("f{i}.txt"), b"x");
    }
    watcher.tick(0).unwrap();
    assert_eq!(watcher.tick(2_000).unwrap().len(), 500);
}

#[test]
fn a_missing_folder_is_an_error_and_recovers() {
    let lab = Lab::new();
    let inner = lab.0.join("inner");
    fs::create_dir(&inner).unwrap();
    let mut watcher = Watcher::new(&inner, SETTLE).unwrap();
    fs::remove_dir(&inner).unwrap();
    assert!(watcher.tick(0).is_err());
    fs::create_dir(&inner).unwrap();
    assert!(watcher.tick(1_000).unwrap().is_empty());
}
