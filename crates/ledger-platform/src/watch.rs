//! Notices new items in a folder by looking at it again and again.
//!
//! Polling is deliberate. Windows change notifications can overflow and drop events
//! during a burst, while a listing cannot miss a file that is still there.

use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Browsers keep these next to a download that is still running. Firefox also leaves an
/// empty file under the final name, which must not be sorted while it downloads.
const IN_PROGRESS_SUFFIXES: [&str; 3] = [".part", ".crdownload", ".download"];

type Signature = (u64, Option<SystemTime>);

/// Tracks one folder. Items present when it starts are never reported.
#[derive(Debug)]
pub struct Watcher {
    root: PathBuf,
    settle_ms: u64,
    known: HashMap<OsString, Signature>,
    arriving: HashMap<OsString, (Signature, u64)>,
}

impl Watcher {
    /// Starts watching. Everything already in the folder is left alone from now on.
    pub fn new(root: &Path, settle_ms: u64) -> io::Result<Watcher> {
        let mut watcher = Watcher {
            root: root.to_path_buf(),
            settle_ms,
            known: HashMap::new(),
            arriving: HashMap::new(),
        };
        watcher.rebaseline()?;
        Ok(watcher)
    }

    /// The folder being watched.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Changes how long a new item must stay unchanged before it is reported.
    pub fn set_settle_ms(&mut self, settle_ms: u64) {
        self.settle_ms = settle_ms;
    }

    /// Treats everything there now as already known, such as after a pause.
    pub fn rebaseline(&mut self) -> io::Result<()> {
        self.known = list(&self.root)?;
        self.arriving.clear();
        Ok(())
    }

    /// Stops an item being reported. A file put back by the user must not be sorted again.
    pub fn accept(&mut self, path: &Path) {
        if let (Some(name), Ok(meta)) = (path.file_name(), fs::symlink_metadata(path)) {
            self.known
                .insert(name.to_owned(), (meta.len(), meta.modified().ok()));
            self.arriving.remove(name);
        }
    }

    /// Looks at the folder and returns items that have stopped changing.
    ///
    /// `now_ms` is any clock that only goes forward. Passing it in lets tests run
    /// without waiting.
    pub fn tick(&mut self, now_ms: u64) -> io::Result<Vec<PathBuf>> {
        let listing = list(&self.root)?;
        self.known.retain(|name, _| listing.contains_key(name));
        self.arriving.retain(|name, _| listing.contains_key(name));

        let mut ready = Vec::new();
        for (name, signature) in &listing {
            if self.known.contains_key(name) {
                continue;
            }
            let entry = self
                .arriving
                .entry(name.clone())
                .or_insert((*signature, now_ms));
            if entry.0 != *signature {
                *entry = (*signature, now_ms);
                continue;
            }
            let settled = now_ms.saturating_sub(entry.1) >= self.settle_ms;
            if settled && !download_in_progress(name, &listing) {
                ready.push(self.root.join(name));
            }
        }
        for path in &ready {
            if let Some(name) = path.file_name() {
                if let Some((signature, _)) = self.arriving.remove(name) {
                    self.known.insert(name.to_owned(), signature);
                }
            }
        }
        ready.sort();
        Ok(ready)
    }
}

fn download_in_progress(name: &OsString, listing: &HashMap<OsString, Signature>) -> bool {
    IN_PROGRESS_SUFFIXES.iter().any(|suffix| {
        let mut partner = name.clone();
        partner.push(suffix);
        listing.contains_key(&partner)
    })
}

fn list(root: &Path) -> io::Result<HashMap<OsString, Signature>> {
    let mut found = HashMap::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        // A file can vanish between the listing and this look. It is simply not there.
        if let Ok(meta) = entry.metadata() {
            found.insert(entry.file_name(), (meta.len(), meta.modified().ok()));
        }
    }
    Ok(found)
}
