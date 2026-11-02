//! Reads what the Ledger needs to know about an item. Nothing here writes.

use ledger_core::judge::{Facts, ItemKind};
use ledger_core::kind::sniff;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

const ATTR_HIDDEN: u32 = 0x2;
const ATTR_SYSTEM: u32 = 0x4;
const ATTR_REPARSE_POINT: u32 = 0x400;
const ERROR_SHARING_VIOLATION: i32 = 32;
const ERROR_LOCK_VIOLATION: i32 = 33;
const HEAD_BYTES: u64 = 512;
/// A real Zone.Identifier is small. Anything larger is ignored.
const MAX_ZONE_BYTES: u64 = 64 * 1024;

/// Files that only games and game launchers keep next to each other.
const GAME_FILES: [&str; 6] = [
    "steam_api.dll",
    "steam_api64.dll",
    "steam_appid.txt",
    "unityplayer.dll",
    "unitycrashhandler64.exe",
    "eossdk-win64-shipping.dll",
];

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetDiskFreeSpaceExW(
        path: *const u16,
        free_to_caller: *mut u64,
        total: *mut u64,
        total_free: *mut u64,
    ) -> i32;
}

/// Free space on the drive holding `path`, or `None` if Windows cannot say.
pub fn free_bytes(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let mut free = 0;
    // SAFETY: `wide` is null terminated and outlives the call, `free` is a valid
    // place to write, and Windows allows the other two pointers to be null.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &raw mut free,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    (ok != 0).then_some(free)
}

/// Why an item could not be read.
#[derive(Debug)]
pub enum InspectError {
    /// The name is not valid Unicode, so it cannot be shown or matched safely.
    UnreadableName,
    /// Windows refused or the item is gone.
    Io(io::Error),
}

/// Gathers the facts about one item.
pub fn inspect(path: &Path) -> Result<Facts, InspectError> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or(InspectError::UnreadableName)?
        .to_string();
    let meta = fs::symlink_metadata(path).map_err(InspectError::Io)?;
    let attrs = meta.file_attributes();
    let hidden = attrs & (ATTR_HIDDEN | ATTR_SYSTEM) != 0;
    let bytes = meta.file_size();

    if attrs & ATTR_REPARSE_POINT != 0 || meta.file_type().is_symlink() {
        return Ok(item(name, bytes, ItemKind::Link, hidden));
    }
    if meta.is_dir() {
        return Ok(item(name, bytes, ItemKind::Folder, hidden));
    }

    let mut facts = item(name, bytes, ItemKind::File, hidden);
    match open_alone(path) {
        Ok(mut file) => {
            let mut head = Vec::with_capacity(HEAD_BYTES as usize);
            (&mut file)
                .take(HEAD_BYTES)
                .read_to_end(&mut head)
                .map_err(InspectError::Io)?;
            facts.contents = sniff(&head);
        }
        Err(error) if is_in_use(&error) => facts.in_use = true,
        Err(error) => return Err(InspectError::Io(error)),
    }
    facts.host = source_host(path);
    Ok(facts)
}

/// True when the folder directly holds files that only games keep.
pub fn has_game_files(folder: &Path) -> bool {
    let Ok(entries) = fs::read_dir(folder) else {
        return false;
    };
    entries.flatten().any(|entry| {
        entry
            .file_name()
            .to_str()
            .is_some_and(|name| GAME_FILES.contains(&name.to_lowercase().as_str()))
    })
}

fn item(name: String, bytes: u64, kind: ItemKind, hidden: bool) -> Facts {
    Facts {
        name,
        bytes,
        kind,
        hidden,
        in_use: false,
        contents: None,
        host: None,
    }
}

/// Opens for reading while refusing to share, which fails if another program has it
/// open. The handle closes straight after the read, so nothing stays locked.
fn open_alone(path: &Path) -> io::Result<File> {
    OpenOptions::new().read(true).share_mode(0).open(path)
}

fn is_in_use(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION)
    )
}

/// Reads the website Windows recorded for a download, from its Zone.Identifier stream.
fn source_host(path: &Path) -> Option<String> {
    let mut stream_path = path.as_os_str().to_owned();
    stream_path.push(":Zone.Identifier");
    let file = File::open(&stream_path).ok()?;
    let mut text = String::new();
    file.take(MAX_ZONE_BYTES).read_to_string(&mut text).ok()?;
    host_from_zone(&text)
}

/// Finds the host in Zone.Identifier text. Anyone can write this stream, so the
/// result is only used as a label and a key, never as a path or a request address.
pub fn host_from_zone(text: &str) -> Option<String> {
    let url = |key: &str| {
        text.lines()
            .find_map(|line| line.trim().strip_prefix(key)?.strip_prefix('='))
    };
    let url = url("HostUrl").or_else(|| url("ReferrerUrl"))?;
    let rest = url.trim().split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority
        .rsplit('@')
        .next()?
        .split(':')
        .next()?
        .to_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    let valid = !host.is_empty()
        && host.len() <= 253
        && host
            .chars()
            .all(|c| c.is_alphanumeric() || c == '.' || c == '-');
    valid.then(|| host.to_string())
}

#[cfg(test)]
mod tests {
    use super::host_from_zone;

    #[test]
    fn finds_the_host_in_zone_text() {
        let zone =
            "[ZoneTransfer]\r\nZoneId=3\r\nHostUrl=https://www.Example.com:8443/a/b.pdf?x=1\r\n";
        assert_eq!(host_from_zone(zone).as_deref(), Some("example.com"));
    }

    #[test]
    fn falls_back_to_the_referrer() {
        assert_eq!(
            host_from_zone("ReferrerUrl=https://mail.google.com/x").as_deref(),
            Some("mail.google.com")
        );
    }

    #[test]
    fn hostile_or_empty_values_give_no_host() {
        for zone in [
            "",
            "ZoneId=3",
            "HostUrl=about:internet",
            "HostUrl=https://",
            "HostUrl=https://exa mple.com/",
            "HostUrl=https://<script>/",
        ] {
            assert_eq!(host_from_zone(zone), None, "{zone}");
        }
    }

    #[test]
    fn login_details_in_a_url_are_dropped() {
        assert_eq!(
            host_from_zone("HostUrl=https://user:secret@files.example.com/a").as_deref(),
            Some("files.example.com")
        );
    }
}
