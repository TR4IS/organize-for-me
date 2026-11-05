//! The running app without its window: the receipt, the log and the watching loop.
//!
//! Text reasons here are the English keys the screens translate.

use crate::store::{self, Saved};
use ledger_core::judge::{
    Decision, Evidence, Facts, HoldReason, ItemKind, Note, Options, SkipReason, Verdict, judge,
};
use ledger_core::kind::Bin;
use ledger_core::memory::Memory;
use ledger_platform::inspect::{InspectError, free_bytes, inspect};
use ledger_platform::journal::{Action, Journal, Recovered};
use ledger_platform::mover::{MoveError, move_to, put_back};
use ledger_platform::sort::{FolderBlocked, Sorted, sort_item};
use ledger_platform::watch::Watcher;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MS_PER_SECOND: u64 = 1000;
const DEFAULT_DELAY_SECONDS: u64 = 2;
const WAIT_RETRY_MS: u64 = 3000;
/// How many items a first-run preview lists.
const PREVIEW_LIMIT: usize = 8;
/// How many items a first-run preview counts.
const PREVIEW_SCAN_LIMIT: usize = 5000;
/// A slow drive must not keep someone waiting: the preview shows what it found in this time.
const PREVIEW_TIME_LIMIT: Duration = Duration::from_millis(1200);
/// Below this much space on the app's own drive, sorting waits. A move itself needs
/// almost no space, but the records of it do, and no move goes unrecorded.
const MIN_FREE_BYTES: u64 = 5 * 1024 * 1024;
const HOLD_DISK_FULL: &str = "disk is almost full, sorting waits";
const HOLD_CANNOT_SAVE: &str = "cannot save records, sorting waits";

/// One line on the receipt.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: u64,
    pub status: String,
    pub bin: String,
    pub evidence_bin: String,
    pub shown: String,
    pub saved: Option<String>,
    pub bytes: u64,
    pub count: u64,
    pub is_dir: bool,
    pub host: Option<String>,
    pub ev: Vec<(String, i32)>,
    pub conf: u8,
    pub reason: String,
    pub notes: Vec<String>,
    pub at: u64,
    pub undo_fail: Option<String>,
    pub back_as: Option<String>,
    pub seq: Option<u64>,
    pub path: PathBuf,
}

/// One line in the log.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub id: u64,
    pub level: String,
    pub name: String,
    pub msg: String,
    pub at: u64,
    pub seq: Option<u64>,
    #[serde(default)]
    pub undone: bool,
}

/// A finished receipt.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Past {
    pub no: u64,
    pub moved: u64,
    pub bytes: u64,
    pub day: u64,
}

/// What the guard said about a folder, for the screens.
#[derive(Clone, Debug, Serialize)]
pub struct FolderChoice {
    pub path: String,
    pub level: &'static str,
    pub why: &'static str,
}

/// Everything the screens draw from.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub items: Vec<Item>,
    pub seen: u64,
    pub no: u64,
    pub opened: u64,
    pub all_time: u64,
    pub history: Vec<Past>,
    pub paused: bool,
    pub learned: usize,
    pub problem: Option<String>,
    pub cfg: Value,
}

/// A first-run preview: judged but untouched.
#[derive(Clone, Debug, Serialize, Default)]
pub struct Preview {
    pub rows: Vec<Item>,
    pub moved: u64,
    pub skip: u64,
    pub held: u64,
    /// True when the folder was too big to check fully in time.
    pub partial: bool,
}

/// The running app.
pub struct Engine {
    dir: PathBuf,
    journal: Journal,
    memory: Memory,
    cfg: Value,
    no: u64,
    opened: u64,
    all_time: u64,
    seen: u64,
    items: Vec<Item>,
    history: Vec<Past>,
    logs: Vec<LogLine>,
    next_id: u64,
    next_log: u64,
    paused: bool,
    watcher: Option<Watcher>,
    problem: Option<String>,
    folder_lost: bool,
    last_wait_retry: u64,
    save_failed: bool,
    last_save_retry: u64,
    hold: Option<&'static str>,
}

impl Engine {
    /// Loads saved data and settles any move a crash interrupted.
    pub fn open(dir: &Path) -> Result<Engine, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let journal = Journal::open(&dir.join("ledger.journal")).map_err(|e| e.to_string())?;
        let saved = store::load_state(dir);
        let logs = store::load_logs(dir);
        let mut memory = Memory::default();
        for (host, from, learned) in &saved.memory {
            store::restore_memory(&mut memory, host, from, learned);
        }
        let next_log = logs.iter().map(|l| l.id).max().unwrap_or(0) + 1;
        let mut engine = Engine {
            dir: dir.to_path_buf(),
            journal,
            memory,
            cfg: saved.cfg,
            no: saved.no.max(1),
            opened: if saved.opened == 0 {
                now()
            } else {
                saved.opened
            },
            all_time: saved.all_time,
            seen: saved.seen,
            items: saved.items,
            history: saved.history,
            logs,
            next_id: saved.next_id.max(1),
            next_log,
            paused: saved.paused,
            watcher: None,
            problem: None,
            folder_lost: false,
            last_wait_retry: 0,
            save_failed: false,
            last_save_retry: 0,
            hold: None,
        };
        engine.recover()?;
        engine.settle_voids();
        engine.log("SYS", "", "started", None);
        if engine.welcomed() && !engine.paused {
            engine.start_watcher();
        }
        engine.save();
        Ok(engine)
    }

    /// The state the screens need.
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            items: self.items.clone(),
            seen: self.seen,
            no: self.no,
            opened: self.opened,
            all_time: self.all_time,
            history: self.history.clone(),
            paused: self.paused,
            learned: self.memory.len(),
            problem: self.hold.map(String::from).or_else(|| self.problem.clone()),
            cfg: self.cfg.clone(),
        }
    }

    /// Log lines, newest first, with undo state read from the journal.
    pub fn logs(&self) -> Vec<LogLine> {
        self.logs
            .iter()
            .rev()
            .map(|line| {
                let mut line = line.clone();
                line.undone = line.seq.is_some_and(|seq| self.journal.is_undone(seq));
                line
            })
            .collect()
    }

    /// Saves settings from the screens. The folder only changes through `choose_folder`.
    pub fn save_config(&mut self, mut incoming: Value) {
        incoming["folder"] = self.cfg["folder"].clone();
        incoming["welcomed"] = self.cfg["welcomed"].clone();
        self.cfg = incoming;
        let settle = self.settle_ms();
        if let Some(watcher) = &mut self.watcher {
            watcher.set_settle_ms(settle);
        }
        self.save();
    }

    /// Says in Logs that the app closed unexpectedly last time, and where the details are.
    pub fn note_crash(&mut self, file: &str) {
        self.log(
            "FAIL",
            "",
            &format!("closed unexpectedly last time, details in {file}"),
            None,
        );
    }

    /// Current setting for the window's close button.
    pub fn closes_to_tray(&self) -> bool {
        self.cfg["closeTo"].as_str() != Some("quit")
    }

    /// Current language.
    pub fn language(&self) -> String {
        self.cfg["lang"].as_str().unwrap_or("en").to_string()
    }

    /// Records whether the app starts with Windows.
    pub fn set_startup_setting(&mut self, on: bool) {
        self.cfg["startup"] = json!(on);
        self.save();
    }

    /// Finishes the first-run guide and starts on the chosen folder.
    pub fn start_watching(
        &mut self,
        path: &str,
        risk_accepted: bool,
        paused: bool,
    ) -> Result<(), String> {
        self.choose_folder(path, risk_accepted)?;
        self.cfg["welcomed"] = json!(true);
        self.set_paused(paused);
        Ok(())
    }

    /// Changes the watched folder, if the guard allows it.
    pub fn choose_folder(&mut self, path: &str, risk_accepted: bool) -> Result<(), String> {
        let choice = folder_choice(path);
        if choice.level == "block" {
            return Err(choice.why.to_string());
        }
        if choice.level == "warn" && !risk_accepted {
            return Err(choice.why.to_string());
        }
        if !Path::new(path).is_dir() {
            return Err("watched folder is missing".to_string());
        }
        self.cfg["folder"] = json!(path);
        self.watcher = None;
        self.problem = None;
        self.folder_lost = false;
        if self.welcomed() && !self.paused {
            self.start_watcher();
        }
        self.save();
        Ok(())
    }

    /// Pauses or resumes. Files that arrive during a pause stay where they are.
    pub fn set_paused(&mut self, paused: bool) {
        let changed = self.paused != paused;
        self.paused = paused;
        self.watcher = None;
        if !paused {
            self.start_watcher();
        }
        if changed {
            self.log("SYS", "", if paused { "paused" } else { "resumed" }, None);
        }
        self.save();
    }

    /// Closes this receipt and opens a new one. Waiting and held items carry over.
    pub fn new_receipt(&mut self) {
        if self.items.iter().all(|i| is_carried(&i.status)) {
            return;
        }
        let moved: Vec<&Item> = self.items.iter().filter(|i| i.status == "moved").collect();
        let count = moved.len() as u64;
        let bytes = moved.iter().map(|i| i.bytes).sum();
        self.history.insert(
            0,
            Past {
                no: self.no,
                moved: count,
                bytes,
                day: now(),
            },
        );
        self.all_time += count;
        self.no += 1;
        self.opened = now();
        self.items.retain(|i| is_carried(&i.status));
        self.seen = self.items.len() as u64;
        self.log("SYS", "", "newr", None);
        self.save();
    }

    /// Erases what was learned. Files are not touched.
    pub fn forget(&mut self) {
        self.memory.forget();
        self.save();
    }

    /// Puts a moved item back where it was.
    pub fn void_item(&mut self, id: u64) {
        let Some(index) = self.find(id) else { return };
        if self.items[index].status != "moved" {
            return;
        }
        let Some(seq) = self.items[index].seq else {
            return;
        };
        match put_back(seq, &mut self.journal) {
            Ok(back) => {
                self.accept_in_watcher(&back.to);
                let learn = self.learns();
                let item = &mut self.items[index];
                item.status = "void".into();
                item.undo_fail = None;
                item.back_as = back.renamed.then(|| file_name(&back.to));
                item.path = back.to.clone();
                if learn
                    && let (Some(host), Some(bin)) =
                        (&item.host, Bin::from_folder(&item.evidence_bin))
                {
                    self.memory.voided(host, bin);
                }
                let name = self.items[index].shown.clone();
                self.log("VOID", &name, "back", None);
            }
            Err(error) => {
                self.items[index].undo_fail = Some(undo_failure(&error));
            }
        }
        self.save();
    }

    /// Moves a voided item into its folder again.
    pub fn redo_item(&mut self, id: u64) {
        let Some(index) = self.find(id) else { return };
        if self.items[index].status != "void" {
            return;
        }
        let bin = self.items[index].bin.clone();
        self.move_item(index, &bin, "again");
        self.save();
    }

    /// Carries out the user's choice for a held item, and remembers it for that website.
    pub fn pick_item(&mut self, id: u64, to: &str) {
        let Some(index) = self.find(id) else { return };
        let Some(to_bin) = Bin::from_folder(to) else {
            return;
        };
        if self.items[index].status != "held" {
            return;
        }
        if self.move_item(index, to_bin.folder(), "") && self.learns() {
            let item = &mut self.items[index];
            item.ev
                .push((format!("You chose {}/", to_bin.folder()), 100));
            item.conf = 100;
            if let (Some(host), Some(from)) = (&item.host, Bin::from_folder(&item.evidence_bin)) {
                self.memory.chose(host, from, to_bin);
            }
        }
        self.save();
    }

    /// Leaves a held item where it is.
    pub fn leave_item(&mut self, id: u64) {
        let Some(index) = self.find(id) else { return };
        if self.items[index].status != "held" {
            return;
        }
        self.items[index].status = "skip".into();
        self.items[index].reason = "you chose to leave it".into();
        let name = self.items[index].shown.clone();
        self.log("SKIP", &name, "left", None);
        self.save();
    }

    /// Tries a failed item again.
    pub fn retry_item(&mut self, id: u64) {
        let Some(index) = self.find(id) else { return };
        if self.items[index].status != "fail" {
            return;
        }
        let path = self.items[index].path.clone();
        self.resort(index, &path);
        if self.items[index].status == "fail" {
            let name = self.items[index].shown.clone();
            self.log("FAIL", &name, "retry", None);
        }
        self.save();
    }

    /// Puts back a move from the log, which may belong to an older receipt.
    pub fn undo_log(&mut self, log_id: u64) -> bool {
        let Some(line) = self.logs.iter().find(|l| l.id == log_id).cloned() else {
            return false;
        };
        let Some(seq) = line.seq else { return false };
        if let Some(item) = self.items.iter().find(|i| i.seq == Some(seq)) {
            let id = item.id;
            self.void_item(id);
            return self.journal.is_undone(seq);
        }
        match put_back(seq, &mut self.journal) {
            Ok(back) => {
                self.accept_in_watcher(&back.to);
                self.log("VOID", &line.name, "back", None);
                self.save();
                true
            }
            Err(_) => false,
        }
    }

    /// The current path of an item, to show it in Explorer.
    pub fn item_path(&self, id: u64) -> Option<PathBuf> {
        self.find(id).map(|index| self.items[index].path.clone())
    }

    /// Runs one watching step. Returns true when the screens need to redraw.
    pub fn tick(&mut self, now_ms: u64) -> bool {
        if self.watcher.is_none() {
            return false;
        }
        let mut changed = self.update_hold(now_ms);
        if self.hold.is_some() {
            // The files stay where they are and are sorted once records can be kept again.
            return changed;
        }
        let Some(mut watcher) = self.watcher.take() else {
            return changed;
        };
        if self.folder_lost {
            // Whatever is there now may be a different disk, so nothing counts as new.
            if watcher.rebaseline().is_ok() {
                self.folder_lost = false;
                self.problem = None;
                changed = true;
            } else {
                self.watcher = Some(watcher);
                return false;
            }
        }
        let root = watcher.root().to_path_buf();
        match watcher.tick(now_ms) {
            Ok(ready) => {
                for path in ready {
                    changed |= self.sort_new(&path, &root);
                }
            }
            Err(_) => {
                self.folder_lost = true;
                self.problem = Some("watched folder is missing".into());
                self.log("FAIL", "", "watched folder is missing", None);
                changed = true;
            }
        }
        self.watcher = Some(watcher);
        if now_ms.saturating_sub(self.last_wait_retry) >= WAIT_RETRY_MS {
            self.last_wait_retry = now_ms;
            changed |= self.retry_waiting();
        }
        if changed {
            self.save();
        }
        changed
    }

    fn welcomed(&self) -> bool {
        self.cfg["welcomed"].as_bool().unwrap_or(false)
    }

    fn learns(&self) -> bool {
        self.cfg["learn"].as_bool().unwrap_or(true)
    }

    fn options(&self) -> Options {
        Options {
            move_folders: self.cfg["folders"].as_bool().unwrap_or(false),
        }
    }

    fn settle_ms(&self) -> u64 {
        self.cfg["delay"].as_u64().unwrap_or(DEFAULT_DELAY_SECONDS) * MS_PER_SECOND
    }

    fn folder(&self) -> Option<PathBuf> {
        self.cfg["folder"]
            .as_str()
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
    }

    fn start_watcher(&mut self) {
        let Some(folder) = self.folder() else { return };
        match Watcher::new(&folder, self.settle_ms()) {
            Ok(watcher) => {
                self.watcher = Some(watcher);
                self.problem = None;
                self.folder_lost = false;
            }
            Err(_) => {
                self.problem = Some("watched folder is missing".into());
                self.log("FAIL", "", "watched folder is missing", None);
            }
        }
    }

    fn accept_in_watcher(&mut self, path: &Path) {
        if let Some(watcher) = &mut self.watcher {
            watcher.accept(path);
        }
    }

    fn find(&self, id: u64) -> Option<usize> {
        self.items.iter().position(|i| i.id == id)
    }

    /// Sorts a newly noticed item. Returns true when it adds a line to the receipt.
    fn sort_new(&mut self, path: &Path, root: &Path) -> bool {
        let options = self.options();
        let sorted = match sort_item(path, root, &self.memory, options, &mut self.journal) {
            Ok(sorted) => sorted,
            Err(FolderBlocked(_)) => {
                self.problem = Some("this folder is blocked".into());
                self.log("FAIL", "", "this folder is blocked", None);
                self.paused = true;
                self.watcher = None;
                return true;
            }
        };
        if is_silent(&sorted) {
            return false;
        }
        let id = self.next_id;
        self.next_id += 1;
        let item = item_from(id, path, &sorted);
        self.seen += 1;
        self.log_item(&item);
        self.items.insert(0, item);
        true
    }

    fn retry_waiting(&mut self) -> bool {
        let waiting: Vec<(usize, PathBuf)> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, i)| i.status == "wait")
            .map(|(index, i)| (index, i.path.clone()))
            .collect();
        let mut changed = false;
        for (index, path) in waiting {
            self.resort(index, &path);
            changed |= self.items[index].status != "wait";
        }
        changed
    }

    /// Judges an existing line again, keeping its place on the receipt.
    fn resort(&mut self, index: usize, path: &Path) {
        let Some(root) = self.folder() else { return };
        let options = self.options();
        let Ok(sorted) = sort_item(path, &root, &self.memory, options, &mut self.journal) else {
            return;
        };
        let was = self.items[index].status.clone();
        let mut item = item_from(self.items[index].id, path, &sorted);
        item.at = self.items[index].at;
        if item.status != was {
            self.log_item(&item);
        }
        self.items[index] = item;
    }

    /// Moves an item to a folder. Returns true when the move happened.
    fn move_item(&mut self, index: usize, bin: &str, log_msg: &str) -> bool {
        let Some(root) = self.folder() else {
            return false;
        };
        let path = self.items[index].path.clone();
        match move_to(&path, &root.join(bin), &mut self.journal) {
            Ok(moved) => {
                let item = &mut self.items[index];
                item.status = "moved".into();
                item.bin = bin.to_string();
                item.saved = moved.renamed.then(|| file_name(&moved.to));
                item.reason.clear();
                item.path = moved.to;
                item.seq = Some(moved.seq);
                item.back_as = None;
                let (name, seq) = (item.shown.clone(), item.seq);
                let msg = if log_msg.is_empty() {
                    format!("{bin}/")
                } else {
                    log_msg.to_string()
                };
                self.log("MOVE", &name, &msg, seq);
                true
            }
            Err(error) => {
                let item = &mut self.items[index];
                item.status = "fail".into();
                item.reason = failure_reason(&error);
                let (name, reason) = (item.shown.clone(), item.reason.clone());
                self.log("FAIL", &name, &reason, None);
                false
            }
        }
    }

    fn log_item(&mut self, item: &Item) {
        let (level, msg) = match item.status.as_str() {
            "moved" => ("MOVE", format!("{}/", item.bin)),
            "held" => ("HOLD", item.reason.clone()),
            "wait" => ("WAIT", item.reason.clone()),
            "skip" => ("SKIP", item.reason.clone()),
            _ => ("FAIL", item.reason.clone()),
        };
        self.log(level, &item.shown, &msg, item.seq);
    }

    fn log(&mut self, level: &str, name: &str, msg: &str, seq: Option<u64>) {
        let line = LogLine {
            id: self.next_log,
            level: level.into(),
            name: name.into(),
            msg: msg.into(),
            at: now(),
            seq,
            undone: false,
        };
        self.next_log += 1;
        store::append_log(&self.dir, &line);
        self.logs.push(line);
    }

    fn recover(&mut self) -> Result<(), String> {
        let found = self.journal.recover().map_err(|e| e.to_string())?;
        for (record, result) in found {
            let name = file_name(&record.from);
            match result {
                Recovered::Completed => {
                    self.log("MOVE", &name, "recovered after a crash", Some(record.seq));
                }
                Recovered::NotMoved => {
                    self.log("SKIP", &name, "not moved, stayed in place", None);
                }
                Recovered::Missing => {
                    self.log("FAIL", &name, "not found after a crash", None);
                }
            }
        }
        Ok(())
    }

    /// The journal outranks the saved receipt. A void that happened just before a crash,
    /// or while the disk was full, is in the journal even if the receipt never saved it.
    fn settle_voids(&mut self) {
        for record in self.journal.completed() {
            let Action::Undo(of) = record.action else {
                continue;
            };
            if let Some(item) = self
                .items
                .iter_mut()
                .find(|i| i.status == "moved" && i.seq == Some(of))
            {
                item.status = "void".into();
                item.path = record.to.clone();
            }
        }
    }

    fn save(&mut self) {
        let memory = store::export_memory(&self.memory);
        let saved = Saved {
            cfg: self.cfg.clone(),
            no: self.no,
            opened: self.opened,
            all_time: self.all_time,
            seen: self.seen,
            items: self.items.clone(),
            history: self.history.clone(),
            next_id: self.next_id,
            paused: self.paused,
            memory,
        };
        self.save_failed = store::save_state(&self.dir, &saved).is_err();
    }

    /// Decides if sorting must wait because the app could not record what it does.
    /// Returns true when that answer changed.
    fn update_hold(&mut self, now_ms: u64) -> bool {
        if self.save_failed && now_ms.saturating_sub(self.last_save_retry) >= WAIT_RETRY_MS {
            self.last_save_retry = now_ms;
            self.save();
        }
        let hold = if self.save_failed {
            Some(HOLD_CANNOT_SAVE)
        } else if free_bytes(&self.dir).is_some_and(|free| free < MIN_FREE_BYTES) {
            Some(HOLD_DISK_FULL)
        } else {
            None
        };
        if hold == self.hold {
            return false;
        }
        self.hold = hold;
        if let Some(reason) = hold {
            self.log("FAIL", "", reason, None);
        }
        true
    }
}

/// Asks the guard about a folder.
pub fn folder_choice(path: &str) -> FolderChoice {
    use ledger_core::guard::{Blocked, Guard, Risk};
    use ledger_platform::sort::check_folder;
    let (level, why) = match check_folder(Path::new(path)) {
        Guard::Ok => ("ok", ""),
        Guard::Warn(Risk::Network) => ("warn", "network"),
        Guard::Warn(Risk::Personal) => ("warn", "personal"),
        Guard::Block(Blocked::Games) => ("block", "games"),
        Guard::Block(Blocked::Root) => ("block", "root"),
        Guard::Block(Blocked::System) => ("block", "system"),
    };
    FolderChoice {
        path: path.to_string(),
        level,
        why,
    }
}

/// Judges what is in a folder without touching anything.
pub fn preview(path: &str, memory_free: Options) -> Preview {
    let mut result = Preview::default();
    let started = Instant::now();
    let root = Path::new(path);
    let Ok(entries) = std::fs::read_dir(root) else {
        return result;
    };
    let memory = Memory::default();
    for (index, entry) in entries.flatten().enumerate() {
        if index >= PREVIEW_SCAN_LIMIT || started.elapsed() > PREVIEW_TIME_LIMIT {
            result.partial = true;
            break;
        }
        let item_path = entry.path();
        let Ok(facts) = inspect(&item_path) else {
            continue;
        };
        let verdict = judge(&facts, &memory, memory_free);
        let status = match verdict.decision {
            Decision::Move => {
                result.moved += 1;
                "moved"
            }
            Decision::Hold(_) => {
                result.held += 1;
                "held"
            }
            Decision::Wait => "wait",
            Decision::Skip(_) => {
                result.skip += 1;
                "skip"
            }
        };
        if result.rows.len() < PREVIEW_LIMIT {
            let mut item = base_item(index as u64, &item_path, &facts, &verdict);
            item.status = status.to_string();
            result.rows.push(item);
        }
    }
    result
}

fn is_carried(status: &str) -> bool {
    status == "wait" || status == "held"
}

/// Items that are not worth a line, because they are about to change or are the app's own.
fn is_silent(sorted: &Sorted) -> bool {
    matches!(
        sorted,
        Sorted::Judged {
            verdict: Verdict {
                decision: Decision::Skip(SkipReason::Unfinished | SkipReason::DestinationFolder),
                ..
            },
            ..
        }
    )
}

fn item_from(id: u64, path: &Path, sorted: &Sorted) -> Item {
    let status = sorted.status();
    match sorted {
        Sorted::Unreadable(error) => {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
            Item {
                id,
                status: "fail".into(),
                bin: Bin::Random.folder().into(),
                evidence_bin: Bin::Random.folder().into(),
                shown: clean(&name.unwrap_or_else(|| "(no name)".into())),
                saved: None,
                bytes: 0,
                count: 1,
                is_dir: false,
                host: None,
                ev: Vec::new(),
                conf: 0,
                reason: inspect_reason(error),
                notes: Vec::new(),
                at: now(),
                undo_fail: None,
                back_as: None,
                seq: None,
                path: path.to_path_buf(),
            }
        }
        Sorted::Judged {
            facts,
            verdict,
            moved,
            failure,
        } => {
            let mut item = base_item(id, path, facts, verdict);
            item.status = status_key(status).into();
            if let Some(moved) = moved {
                item.seq = Some(moved.seq);
                item.path = moved.to.clone();
                if moved.renamed {
                    item.saved = Some(file_name(&moved.to));
                    item.notes
                        .push(format!("{}/ already had this name", item.bin));
                }
            }
            if let Some(error) = failure {
                item.reason = failure_reason(error);
                if matches!(error, MoveError::InUse) {
                    item.status = "wait".into();
                    item.reason = "in use, will retry".into();
                }
            }
            item
        }
    }
}

fn base_item(id: u64, path: &Path, facts: &Facts, verdict: &Verdict) -> Item {
    let mut ev: Vec<(String, i32)> = verdict.evidence.iter().map(evidence_line).collect();
    if verdict.decision == Decision::Hold(HoldReason::DisguisedProgram) {
        ev.push(("Looks like a program in disguise".into(), 0));
    }
    Item {
        id,
        status: String::new(),
        bin: verdict.bin.folder().into(),
        evidence_bin: verdict.evidence_bin.folder().into(),
        shown: clean(&facts.name),
        saved: None,
        bytes: facts.bytes,
        count: 1,
        is_dir: facts.kind == ItemKind::Folder,
        host: facts.host.clone(),
        ev,
        conf: verdict.confidence,
        reason: decision_reason(verdict.decision).into(),
        notes: verdict.notes.iter().map(note_line).collect(),
        at: now(),
        undo_fail: None,
        back_as: None,
        seq: None,
        path: path.to_path_buf(),
    }
}

fn status_key(status: ledger_core::ledger::Status) -> &'static str {
    use ledger_core::ledger::Status::*;
    match status {
        Moved => "moved",
        Void => "void",
        Held => "held",
        Wait => "wait",
        Skip => "skip",
        Fail => "fail",
    }
}

fn evidence_line(evidence: &Evidence) -> (String, i32) {
    let points = evidence.points();
    let text = match evidence {
        Evidence::Contents(bin) => format!("Contents are {}", bin.folder()),
        Evidence::Extension(ext) => format!("Extension .{ext}"),
        Evidence::Package => "Office or app package".into(),
        Evidence::Site(host) => format!("Came from {host}"),
        Evidence::LearnedChoice(host) => format!("You chose this for {host}"),
        Evidence::LearnedVoid(host) => format!("You voided this from {host}"),
        Evidence::FolderSetting => "Folder move is on".into(),
        Evidence::Nothing => "No type evidence".into(),
    };
    (text, points)
}

fn note_line(note: &Note) -> String {
    match note {
        Note::HiddenDirection => "Name hides its true direction".into(),
        Note::Empty => "Empty file, 0 bytes".into(),
    }
}

fn decision_reason(decision: Decision) -> &'static str {
    match decision {
        Decision::Move => "",
        Decision::Wait => "in use, will retry",
        Decision::Hold(HoldReason::DisguisedProgram) => "may be a disguised program",
        Decision::Hold(HoldReason::NameContentsDisagree) => "name and contents disagree",
        Decision::Hold(HoldReason::VoidedSource) => "you undid this source before",
        Decision::Skip(reason) => match reason {
            SkipReason::DestinationFolder => "this is a destination folder",
            SkipReason::FoldersOff => "folders stay unless enabled",
            SkipReason::Link => "link, moving could break it",
            SkipReason::Unfinished => "unfinished download",
            SkipReason::SystemFile => "Windows or Office system file",
            SkipReason::Hidden => "hidden file",
            SkipReason::Shortcut => "shortcut, moving could break it",
            SkipReason::ReservedName => "reserved Windows name",
        },
    }
}

fn failure_reason(error: &MoveError) -> String {
    match error {
        MoveError::Gone => "file vanished before moving",
        MoveError::InUse => "in use, will retry",
        MoveError::Denied => "Windows denied access",
        MoveError::OtherDrive => "destination is on another drive",
        MoveError::DiskFull => "not enough space on target drive",
        MoveError::IsLink => "link, moving could break it",
        MoveError::UnreadableName => "file name cannot be read",
        _ => "could not move, file stayed in place",
    }
    .to_string()
}

fn undo_failure(error: &MoveError) -> String {
    match error {
        MoveError::Gone => "File was deleted after moving".to_string(),
        other => failure_reason(other),
    }
}

fn inspect_reason(error: &InspectError) -> String {
    match error {
        InspectError::UnreadableName => "file name cannot be read",
        InspectError::Io(io) => match io.raw_os_error() {
            Some(2 | 3) => "file vanished before moving",
            Some(5) => "Windows denied access",
            _ => "could not move, file stayed in place",
        },
    }
    .to_string()
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Removes characters that could fake how a name displays. Only for showing it.
fn clean(name: &str) -> String {
    name.chars()
        .filter(|c| !matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{200e}' | '\u{200f}'))
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

/// Milliseconds since 1970, for labels only.
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::disallowed_methods)]
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static NEXT: AtomicU32 = AtomicU32::new(0);

    struct Lab {
        base: PathBuf,
        watched: PathBuf,
    }

    impl Lab {
        fn new() -> Lab {
            let id = NEXT.fetch_add(1, Ordering::SeqCst);
            let base = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../target/tmp")
                .join(format!("ofm-engine-{}-{id}", std::process::id()));
            let watched = base.join("watched");
            std::fs::create_dir_all(&watched).unwrap();
            Lab { base, watched }
        }

        fn engine(&self) -> Engine {
            let mut engine = Engine::open(&self.base.join("data")).unwrap();
            engine.cfg["delay"] = json!(0);
            engine
                .start_watching(self.watched.to_str().unwrap(), true, false)
                .unwrap();
            engine
        }

        fn file(&self, name: &str, data: &[u8]) -> PathBuf {
            let path = self.watched.join(name);
            std::fs::write(&path, data).unwrap();
            path
        }
    }

    impl Drop for Lab {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.base);
        }
    }

    /// Two ticks: one to notice, one to confirm it stopped changing.
    fn run(engine: &mut Engine, clock: &mut u64) {
        for _ in 0..2 {
            *clock += 10;
            engine.tick(*clock);
        }
    }

    #[test]
    fn files_already_there_stay_and_new_ones_move() {
        let lab = Lab::new();
        let old = lab.file("old.pdf", b"%PDF-");
        let mut engine = lab.engine();
        let mut clock = 0;
        run(&mut engine, &mut clock);
        assert!(old.exists());
        assert!(engine.items.is_empty());

        let new = lab.file("new.pdf", b"%PDF-");
        run(&mut engine, &mut clock);
        assert!(!new.exists());
        assert!(lab.watched.join("pdf").join("new.pdf").exists());
        assert_eq!(engine.items[0].status, "moved");
        assert!(old.exists());
    }

    #[test]
    fn nothing_moves_while_records_cannot_be_saved() {
        let lab = Lab::new();
        let mut engine = lab.engine();
        let mut clock = 0;
        run(&mut engine, &mut clock);

        // A folder in the temporary file's place makes every save fail, as a full disk would.
        let block = lab.base.join("data").join("state.tmp");
        std::fs::create_dir(&block).unwrap();
        engine.save();
        let file = lab.file("waits.pdf", b"%PDF-");
        clock += 10_000;
        run(&mut engine, &mut clock);
        assert!(file.exists(), "no move may happen without a record");
        assert_eq!(engine.snapshot().problem.as_deref(), Some(HOLD_CANNOT_SAVE));

        std::fs::remove_dir(&block).unwrap();
        clock += 10_000;
        run(&mut engine, &mut clock);
        run(&mut engine, &mut clock);
        assert!(lab.watched.join("pdf").join("waits.pdf").exists());
        assert_eq!(engine.snapshot().problem, None);
        let saved = store::load_state(&lab.base.join("data"));
        assert_eq!(saved.items[0].shown, "waits.pdf");
    }

    #[test]
    fn a_void_the_receipt_never_saved_is_still_shown_after_a_restart() {
        let lab = Lab::new();
        let mut engine = lab.engine();
        let mut clock = 0;
        run(&mut engine, &mut clock);
        let file = lab.file("back.pdf", b"%PDF-");
        run(&mut engine, &mut clock);
        let id = engine.items[0].id;

        let block = lab.base.join("data").join("state.tmp");
        std::fs::create_dir(&block).unwrap();
        engine.void_item(id);
        assert!(file.exists());
        drop(engine);
        std::fs::remove_dir(&block).unwrap();

        let engine = Engine::open(&lab.base.join("data")).unwrap();
        assert_eq!(engine.items[0].status, "void");
        assert_eq!(file_name(&engine.items[0].path), "back.pdf");
    }

    #[test]
    fn a_voided_file_is_not_sorted_again() {
        let lab = Lab::new();
        let mut engine = lab.engine();
        let mut clock = 0;
        let src = lab.file("a.pdf", b"%PDF-");
        run(&mut engine, &mut clock);
        let id = engine.items[0].id;

        engine.void_item(id);
        assert!(src.exists());
        for _ in 0..5 {
            run(&mut engine, &mut clock);
        }
        assert!(src.exists());
        assert_eq!(engine.items.len(), 1);
        assert_eq!(engine.items[0].status, "void");

        engine.redo_item(id);
        assert!(!src.exists());
        assert_eq!(engine.items[0].status, "moved");
    }

    #[test]
    fn the_apps_own_folders_and_partial_downloads_add_no_lines() {
        let lab = Lab::new();
        let mut engine = lab.engine();
        let mut clock = 0;
        lab.file("a.pdf", b"%PDF-");
        lab.file("movie.mp4.crdownload", b"x");
        for _ in 0..4 {
            run(&mut engine, &mut clock);
        }
        assert_eq!(engine.items.len(), 1);
        assert!(lab.watched.join("movie.mp4.crdownload").exists());
    }

    #[test]
    fn a_held_file_stays_until_the_user_chooses() {
        let lab = Lab::new();
        let mut engine = lab.engine();
        let mut clock = 0;
        let src = lab.file("invoice.pdf", b"MZ program");
        run(&mut engine, &mut clock);
        assert_eq!(engine.items[0].status, "held");
        assert!(src.exists());
        for _ in 0..3 {
            run(&mut engine, &mut clock);
        }
        assert!(src.exists());

        engine.pick_item(engine.items[0].id, "exe");
        assert!(!src.exists());
        assert!(lab.watched.join("exe").join("invoice.pdf").exists());
    }

    #[test]
    fn a_pause_leaves_new_files_even_after_resuming() {
        let lab = Lab::new();
        let mut engine = lab.engine();
        let mut clock = 0;
        engine.set_paused(true);
        let during = lab.file("during.pdf", b"%PDF-");
        run(&mut engine, &mut clock);
        engine.set_paused(false);
        run(&mut engine, &mut clock);
        assert!(during.exists());

        let after = lab.file("after.pdf", b"%PDF-");
        run(&mut engine, &mut clock);
        assert!(!after.exists());
    }

    #[test]
    fn a_blocked_folder_cannot_be_chosen() {
        let lab = Lab::new();
        let mut engine = lab.engine();
        let windows = std::env::var("WINDIR").unwrap();
        assert!(engine.choose_folder(&windows, true).is_err());
        assert_eq!(engine.cfg["folder"], lab.watched.to_str().unwrap());
        assert!(engine.choose_folder(r"C:\", true).is_err());
    }

    #[test]
    fn a_risky_folder_needs_the_risk_accepted() {
        let lab = Lab::new();
        let mut engine = lab.engine();
        // The guard treats anything under OneDrive as risky, wherever the code is checked out.
        let risky = lab.base.join("OneDrive");
        std::fs::create_dir_all(&risky).unwrap();
        let path = risky.to_str().unwrap().to_string();
        assert_eq!(folder_choice(&path).level, "warn");
        assert!(engine.choose_folder(&path, false).is_err());
        assert!(engine.choose_folder(&path, true).is_ok());
    }

    #[test]
    fn a_receipt_and_undo_survive_a_restart() {
        let lab = Lab::new();
        let mut clock = 0;
        let src = lab.watched.join("a.pdf");
        let id = {
            let mut engine = lab.engine();
            lab.file("a.pdf", b"%PDF-");
            run(&mut engine, &mut clock);
            engine.items[0].id
        };
        let mut engine = Engine::open(&lab.base.join("data")).unwrap();
        assert_eq!(engine.items.len(), 1);
        engine.void_item(id);
        assert!(src.exists());
    }

    #[test]
    fn undo_from_the_log_works_for_an_older_receipt() {
        let lab = Lab::new();
        let mut engine = lab.engine();
        let mut clock = 0;
        let src = lab.file("a.pdf", b"%PDF-");
        run(&mut engine, &mut clock);
        engine.new_receipt();
        assert!(engine.items.is_empty());
        let line = engine
            .logs()
            .into_iter()
            .find(|l| l.level == "MOVE")
            .unwrap();

        assert!(engine.undo_log(line.id));

        assert!(src.exists());
        let again = engine.logs().into_iter().find(|l| l.id == line.id).unwrap();
        assert!(again.undone);
    }

    #[test]
    fn a_new_receipt_carries_held_files_and_counts_moves() {
        let lab = Lab::new();
        let mut engine = lab.engine();
        let mut clock = 0;
        lab.file("a.pdf", b"%PDF-");
        lab.file("fake.pdf", b"MZ");
        run(&mut engine, &mut clock);
        engine.new_receipt();
        assert_eq!(engine.items.len(), 1);
        assert_eq!(engine.items[0].status, "held");
        assert_eq!(engine.all_time, 1);
        assert_eq!(engine.seen, 1);
    }

    #[test]
    fn a_lost_folder_is_reported_and_nothing_counts_as_new_after_it_returns() {
        let lab = Lab::new();
        let mut engine = lab.engine();
        let mut clock = 0;
        run(&mut engine, &mut clock);
        std::fs::remove_dir(&lab.watched).unwrap();
        run(&mut engine, &mut clock);
        assert!(engine.problem.is_some());

        std::fs::create_dir(&lab.watched).unwrap();
        let back = lab.file("was-on-the-disk.pdf", b"%PDF-");
        run(&mut engine, &mut clock);
        assert!(engine.problem.is_none());
        run(&mut engine, &mut clock);
        assert!(back.exists());
    }

    #[test]
    fn preview_touches_nothing() {
        let lab = Lab::new();
        let a = lab.file("a.pdf", b"%PDF-");
        let b = lab.file("Game.lnk", b"x");
        let c = lab.file("fake.pdf", b"MZ");
        let result = preview(lab.watched.to_str().unwrap(), Options::default());
        assert_eq!((result.moved, result.skip, result.held), (1, 1, 1));
        assert!(a.exists() && b.exists() && c.exists());
    }
}
