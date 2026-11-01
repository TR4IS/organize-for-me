//! An append-only record of every move, written to disk before the move happens.
//!
//! If the power dies mid-move, the journal holds an intent with no result. `recover`
//! then looks at the disk to find out what really happened, so no file is ever lost
//! track of. Each line stands alone, and a half-written last line is ignored.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

const VERSION: &str = "v1";

/// What a journal entry is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Sorting a file into a folder.
    Move,
    /// Putting back the move with this sequence number.
    Undo(u64),
}

/// A finished or pending move.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    /// Its number in the journal.
    pub seq: u64,
    /// A move or an undo.
    pub action: Action,
    /// Where the file was.
    pub from: PathBuf,
    /// Where the file went.
    pub to: PathBuf,
}

/// What recovery found for an interrupted move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recovered {
    /// The file reached its destination.
    Completed,
    /// The file is still in its original place.
    NotMoved,
    /// The file is in neither place, so something else moved or deleted it.
    Missing,
}

/// The move journal.
#[derive(Debug)]
pub struct Journal {
    file: File,
    next_seq: u64,
    pending: BTreeMap<u64, Record>,
    completed: BTreeMap<u64, Record>,
    undone: BTreeSet<u64>,
    /// Damaged lines found when opening, kept out of the record.
    skipped_lines: usize,
}

impl Journal {
    /// Opens or creates the journal and reads everything already in it.
    pub fn open(path: &Path) -> io::Result<Journal> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(path)?;
        let mut raw = Vec::new();
        file.read_to_end(&mut raw)?;

        let mut journal = Journal {
            file,
            next_seq: 1,
            pending: BTreeMap::new(),
            completed: BTreeMap::new(),
            undone: BTreeSet::new(),
            skipped_lines: 0,
        };
        for line in raw.split(|b| *b == b'\n') {
            if !line.is_empty() && !journal.replay(line) {
                journal.skipped_lines += 1;
            }
        }
        if raw.last().is_some_and(|b| *b != b'\n') {
            journal.file.write_all(b"\n")?;
            journal.file.sync_data()?;
        }
        Ok(journal)
    }

    /// Records an intent and forces it to disk. No move may start without this.
    pub fn intend(&mut self, action: Action, from: &Path, to: &Path) -> io::Result<u64> {
        let (Some(from_text), Some(to_text)) = (from.to_str(), to.to_str()) else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path is not valid Unicode",
            ));
        };
        let seq = self.next_seq;
        let kind = match action {
            Action::Move => "move".to_string(),
            Action::Undo(of) => format!("undo:{of}"),
        };
        self.append(&format!(
            "{seq}\t{kind}\t{}\t{}",
            escape(from_text),
            escape(to_text)
        ))?;
        self.next_seq += 1;
        self.pending.insert(
            seq,
            Record {
                seq,
                action,
                from: from.to_path_buf(),
                to: to.to_path_buf(),
            },
        );
        Ok(seq)
    }

    /// Records that the intent succeeded.
    pub fn done(&mut self, seq: u64) -> io::Result<()> {
        self.append(&format!("{seq}\tdone"))?;
        self.finish(seq, true);
        Ok(())
    }

    /// Records that the intent did not happen.
    pub fn abort(&mut self, seq: u64) -> io::Result<()> {
        self.append(&format!("{seq}\tabort"))?;
        self.finish(seq, false);
        Ok(())
    }

    /// Settles every interrupted move by checking the disk, and reports what it found.
    pub fn recover(&mut self) -> io::Result<Vec<(Record, Recovered)>> {
        let pending: Vec<Record> = self.pending.values().cloned().collect();
        let mut found = Vec::with_capacity(pending.len());
        for record in pending {
            let at_source = fs::symlink_metadata(&record.from).is_ok();
            let at_target = fs::symlink_metadata(&record.to).is_ok();
            // With both present the target is some other file that was already there,
            // because a no-replace move can never leave two copies.
            let result = match (at_source, at_target) {
                (false, true) => Recovered::Completed,
                (true, _) => Recovered::NotMoved,
                (false, false) => Recovered::Missing,
            };
            if result == Recovered::Completed {
                self.done(record.seq)?;
            } else {
                self.abort(record.seq)?;
            }
            found.push((record, result));
        }
        Ok(found)
    }

    /// Finds a finished move.
    pub fn get(&self, seq: u64) -> Option<&Record> {
        self.completed.get(&seq)
    }

    /// True when this move was already put back.
    pub fn is_undone(&self, seq: u64) -> bool {
        self.undone.contains(&seq)
    }

    /// Every finished move and undo, oldest first.
    pub fn completed(&self) -> impl Iterator<Item = &Record> {
        self.completed.values()
    }

    /// Moves with no recorded result.
    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    /// Damaged lines found when the journal was opened.
    pub fn skipped_lines(&self) -> usize {
        self.skipped_lines
    }

    fn append(&mut self, body: &str) -> io::Result<()> {
        let line = format!("{VERSION}\t{body}\t.\n");
        self.file.write_all(line.as_bytes())?;
        self.file.sync_data()
    }

    fn finish(&mut self, seq: u64, succeeded: bool) {
        if let Some(record) = self.pending.remove(&seq)
            && succeeded
        {
            if let Action::Undo(of) = record.action {
                self.undone.insert(of);
            }
            self.completed.insert(seq, record);
        }
    }

    /// Applies one saved line. Returns false when the line is damaged.
    fn replay(&mut self, line: &[u8]) -> bool {
        let Ok(text) = std::str::from_utf8(line) else {
            return false;
        };
        let fields: Vec<&str> = text.split('\t').collect();
        // The final "." proves the whole line reached the disk.
        if fields.first() != Some(&VERSION) || fields.last() != Some(&".") {
            return false;
        }
        let Some(seq) = fields.get(1).and_then(|s| s.parse::<u64>().ok()) else {
            return false;
        };
        match fields[2..fields.len() - 1] {
            ["done"] if self.pending.contains_key(&seq) => self.finish(seq, true),
            ["abort"] if self.pending.contains_key(&seq) => self.finish(seq, false),
            [kind, from, to] => {
                let action = match kind.strip_prefix("undo:") {
                    Some(of) => match of.parse() {
                        Ok(of) => Action::Undo(of),
                        Err(_) => return false,
                    },
                    None if kind == "move" => Action::Move,
                    None => return false,
                };
                let (Some(from), Some(to)) = (unescape(from), unescape(to)) else {
                    return false;
                };
                self.pending.insert(
                    seq,
                    Record {
                        seq,
                        action,
                        from: from.into(),
                        to: to.into(),
                    },
                );
                self.next_seq = self.next_seq.max(seq + 1);
            }
            _ => return false,
        }
        true
    }
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn unescape(text: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        out.push(match chars.next()? {
            '\\' => '\\',
            't' => '\t',
            'n' => '\n',
            'r' => '\r',
            _ => return None,
        });
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaping_round_trips() {
        for text in [
            r"C:\a\b.pdf",
            "tab\there",
            "line\nbreak",
            "ملف عربي.pdf",
            r"end\",
        ] {
            assert_eq!(unescape(&escape(text)).as_deref(), Some(text));
        }
        assert_eq!(unescape(r"bad\q"), None);
    }
}
