//! Answers the browser extension, which shows where each download is now.
//!
//! Chrome starts this exe with the extension's address as its argument and talks over stdin and
//! stdout: each message is a 4-byte length, then JSON. This mode only reads the move journal. It
//! never moves a file and never opens the window, so it can run beside the app.

use ledger_platform::journal::Journal;
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The extension's fixed ID, set by the `key` in extension/manifest.json.
pub const EXTENSION_ORIGIN: &str = "chrome-extension://bafeecmlihhlblhcdpabbnlhmblgocdm/";
/// Chrome refuses replies over 1 MB; a request this size is not from the extension.
const MAX_MESSAGE: usize = 1024 * 1024;
/// Opening one of these runs a program, so the extension may only show it in its folder.
const RUNS_CODE: &[&str] = &[
    "exe", "msi", "bat", "cmd", "com", "ps1", "vbs", "js", "jse", "wsf", "scr", "lnk", "hta",
    "reg", "msix", "appx",
];

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum Request {
    /// Where each of these downloads is now.
    Where { paths: Vec<String> },
    /// Opens a moved download where it is now.
    Open { path: String },
    /// Shows a moved download in Explorer.
    Show { path: String },
}

/// True when Chrome started this exe for the extension.
pub fn started_by_chrome() -> bool {
    std::env::args().skip(1).any(|arg| arg == EXTENSION_ORIGIN)
}

/// Answers messages until Chrome closes the connection.
pub fn serve(records: Option<PathBuf>, language: String) {
    let journal = records.as_deref().map(|dir| dir.join("ledger.journal"));
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    while let Some(message) = read_message(&mut input) {
        // Read fresh each time, so a move the app just made is found.
        let journal = journal.as_deref().and_then(|path| Journal::read(path).ok());
        let reply = match serde_json::from_slice::<Request>(&message) {
            Ok(request) => answer(request, journal.as_ref(), &language),
            Err(_) => json!({ "error": "unknown request" }),
        };
        if write_message(&mut output, &reply).is_err() {
            return;
        }
    }
}

fn answer(request: Request, journal: Option<&Journal>, language: &str) -> Value {
    let now = |path: &str| journal.and_then(|j| j.where_now(Path::new(path)));
    match request {
        Request::Where { paths } => {
            let found: Vec<Value> = paths
                .iter()
                .map(|path| match now(path) {
                    Some(to) => json!({
                        "moved": true,
                        "now": shown(&to),
                        "folder": to.parent().and_then(Path::file_name).map(|f| f.to_string_lossy()),
                        "exists": to.exists(),
                        "canOpen": !runs_code(&to),
                    }),
                    None => json!({ "moved": false }),
                })
                .collect();
            json!({ "ok": true, "lang": language, "paths": found })
        }
        // Only files the journal moved: the extension cannot reach any other path through here.
        Request::Open { path } => match now(&path).filter(|to| to.exists() && !runs_code(to)) {
            Some(to) => json!({ "ok": open(&to) }),
            None => json!({ "ok": false }),
        },
        Request::Show { path } => match now(&path).filter(|to| to.exists()) {
            Some(to) => json!({ "ok": show(&to) }),
            None => json!({ "ok": false }),
        },
    }
}

fn shown(path: &Path) -> String {
    let text = path.to_string_lossy();
    text.strip_prefix(r"\\?\").unwrap_or(&text).to_string()
}

fn runs_code(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| RUNS_CODE.contains(&e.to_ascii_lowercase().as_str()))
}

fn open(path: &Path) -> bool {
    Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", &shown(path)])
        .spawn()
        .is_ok()
}

fn show(path: &Path) -> bool {
    use std::os::windows::process::CommandExt;
    let plain = shown(path).replace('"', "");
    Command::new("explorer")
        .raw_arg(format!("/select,\"{plain}\""))
        .spawn()
        .is_ok()
}

fn read_message(input: &mut impl Read) -> Option<Vec<u8>> {
    let mut size = [0u8; 4];
    input.read_exact(&mut size).ok()?;
    let size = u32::from_le_bytes(size) as usize;
    if size > MAX_MESSAGE {
        return None;
    }
    let mut message = vec![0u8; size];
    input.read_exact(&mut message).ok()?;
    Some(message)
}

fn write_message(output: &mut impl Write, reply: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(reply)?;
    output.write_all(&(body.len() as u32).to_le_bytes())?;
    output.write_all(&body)?;
    output.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip() {
        let mut wire = Vec::new();
        write_message(&mut wire, &json!({ "a": 1 })).unwrap();
        let back = read_message(&mut wire.as_slice()).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&back).unwrap(),
            json!({ "a": 1 })
        );
    }

    #[test]
    fn an_oversized_message_is_refused() {
        let wire = (MAX_MESSAGE as u32 + 1).to_le_bytes();
        assert!(read_message(&mut wire.as_slice()).is_none());
    }

    #[test]
    fn paths_are_shown_without_the_long_prefix() {
        assert_eq!(shown(Path::new(r"\\?\C:\d\a.pdf")), r"C:\d\a.pdf");
        assert_eq!(shown(Path::new(r"C:\d\a.pdf")), r"C:\d\a.pdf");
    }

    #[test]
    fn programs_are_never_opened() {
        assert!(runs_code(Path::new(r"C:\d\exe\setup.EXE")));
        assert!(runs_code(Path::new(r"C:\d\x.ps1")));
        assert!(!runs_code(Path::new(r"C:\d\pdf\a.pdf")));
    }

    #[test]
    fn without_records_nothing_is_moved_or_opened() {
        let reply = answer(
            Request::Where {
                paths: vec![r"C:\d\a.pdf".into()],
            },
            None,
            "en",
        );
        assert_eq!(reply["paths"][0]["moved"], false);
        assert_eq!(
            answer(
                Request::Open {
                    path: r"C:\d\a.pdf".into()
                },
                None,
                "en"
            )["ok"],
            false
        );
        assert_eq!(
            answer(
                Request::Show {
                    path: r"C:\d\a.pdf".into()
                },
                None,
                "en"
            )["ok"],
            false
        );
    }
}
