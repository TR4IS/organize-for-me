//! Saves the app's own settings and logs in its data folder.
//!
//! This is the only code outside `mover.rs` allowed to replace a file, and it only
//! ever touches files it made in the app's data folder, never the user's files.
#![allow(clippy::disallowed_methods)]

use crate::engine::{Item, LogLine, Past};
use ledger_core::kind::Bin;
use ledger_core::memory::{Learned, Memory};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

const STATE_FILE: &str = "state.json";
const LOG_FILE: &str = "log.jsonl";
/// Only the newest lines are loaded, so a years-long log cannot slow the app down.
const MAX_LOADED_LOGS: usize = 20_000;
const ASK: &str = "?ask";

/// Everything kept between runs, apart from the journal and the log.
#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Saved {
    pub cfg: Value,
    pub no: u64,
    pub opened: u64,
    pub all_time: u64,
    pub seen: u64,
    pub items: Vec<Item>,
    pub history: Vec<Past>,
    pub next_id: u64,
    pub paused: bool,
    pub memory: Vec<(String, String, String)>,
}

impl Default for Saved {
    fn default() -> Saved {
        Saved {
            cfg: json!({}),
            no: 1,
            opened: 0,
            all_time: 0,
            seen: 0,
            items: Vec::new(),
            history: Vec::new(),
            next_id: 1,
            paused: true,
            memory: Vec::new(),
        }
    }
}

/// Loads saved state. A damaged file is kept aside, and the app starts fresh.
pub fn load_state(dir: &Path) -> Saved {
    let path = dir.join(STATE_FILE);
    let Ok(text) = fs::read_to_string(&path) else {
        return Saved::default();
    };
    match serde_json::from_str(&text) {
        Ok(saved) => saved,
        Err(_) => {
            let _ = fs::rename(&path, dir.join("state.damaged.json"));
            Saved::default()
        }
    }
}

/// Saves state through a temporary file, so a crash or a full disk cannot leave a
/// half-written one. The data reaches the disk before the rename, or a power cut could
/// leave an empty `state.json`.
pub fn save_state(dir: &Path, saved: &Saved) -> io::Result<()> {
    let text = serde_json::to_string(saved)?;
    let temp = dir.join("state.tmp");
    let result = write_synced(&temp, &text).and_then(|()| fs::rename(&temp, dir.join(STATE_FILE)));
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn write_synced(path: &Path, text: &str) -> io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()
}

/// Adds one line to the log file.
pub fn append_log(dir: &Path, line: &LogLine) {
    let Ok(text) = serde_json::to_string(line) else {
        return;
    };
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(LOG_FILE))
    {
        let _ = writeln!(file, "{text}");
    }
}

/// Reads the newest log lines, oldest first. A damaged line is skipped.
pub fn load_logs(dir: &Path) -> Vec<LogLine> {
    let Ok(text) = fs::read_to_string(dir.join(LOG_FILE)) else {
        return Vec::new();
    };
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(MAX_LOADED_LOGS);
    lines[start..]
        .iter()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

/// Turns learned choices into saved text.
pub fn export_memory(memory: &Memory) -> Vec<(String, String, String)> {
    memory
        .entries()
        .map(|(host, from, learned)| {
            let value = match learned {
                Learned::Ask => ASK.to_string(),
                Learned::RouteTo(to) => to.folder().to_string(),
            };
            (host.to_string(), from.folder().to_string(), value)
        })
        .collect()
}

/// Restores one saved choice. Anything unrecognized is ignored.
pub fn restore_memory(memory: &mut Memory, host: &str, from: &str, learned: &str) {
    let Some(from) = Bin::from_folder(from) else {
        return;
    };
    if learned == ASK {
        memory.voided(host, from);
    } else if let Some(to) = Bin::from_folder(learned) {
        memory.chose(host, from, to);
    }
}
