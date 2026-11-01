//! Folder guard: refuses places where moving files breaks games, apps or Windows.

/// Why a folder is blocked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blocked {
    /// A games folder, or a folder holding game files.
    Games,
    /// A whole drive or user profile, or a path that is not a full path.
    Root,
    /// Windows or installed apps need these files in place.
    System,
}

/// Why a folder is risky.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Risk {
    /// Other people may use it, and it can disconnect.
    Network,
    /// Other apps may expect its files and shortcuts to stay there.
    Personal,
}

/// The guard's answer for a folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Guard {
    /// Safe to watch.
    Ok,
    /// Allowed only after the user accepts the risk.
    Warn(Risk),
    /// Never allowed.
    Block(Blocked),
}

const GAME_FOLDERS: [&str; 9] = [
    "steamapps",
    "steam",
    "steamlibrary",
    "epic games",
    "riot games",
    "ea games",
    "origin games",
    "gog galaxy",
    "xboxgames",
];
const SYSTEM_FOLDERS: [&str; 6] = [
    "windows",
    "program files",
    "program files (x86)",
    "programdata",
    "$recycle.bin",
    "system volume information",
];
const PERSONAL_FOLDERS: [&str; 5] = ["desktop", "documents", "pictures", "videos", "music"];

/// Judges a folder. `has_game_files` comes from looking inside it.
pub fn guard_folder(path: &str, has_game_files: bool) -> Guard {
    let (network, parts) = split_path(path);
    let has = |list: &[&str]| parts.iter().any(|p| list.contains(&p.as_str()));

    if has_game_files || has(&GAME_FOLDERS) || parts.iter().any(|p| p == "games") {
        return Guard::Block(Blocked::Games);
    }
    if network {
        return Guard::Warn(Risk::Network);
    }
    let is_drive = parts
        .first()
        .is_some_and(|d| d.len() == 2 && d.as_bytes()[0].is_ascii_alphabetic() && d.ends_with(':'));
    if !is_drive || parts.len() == 1 {
        return Guard::Block(Blocked::Root);
    }
    let in_users = parts[1] == "users";
    if in_users && parts.len() <= 3 {
        return Guard::Block(Blocked::Root);
    }
    if SYSTEM_FOLDERS.contains(&parts[1].as_str()) || parts.iter().any(|p| p == "appdata") {
        return Guard::Block(Blocked::System);
    }
    if (in_users && PERSONAL_FOLDERS.contains(&parts[3].as_str()))
        || parts.iter().any(|p| p == "onedrive")
    {
        return Guard::Warn(Risk::Personal);
    }
    Guard::Ok
}

/// Lowercases a path and splits it, resolving `.` and `..`.
///
/// Windows reports canonical paths as `\\?\C:\Windows`, which must be judged as
/// `C:\Windows` and not as a network folder. Returns whether it is a network path.
fn split_path(path: &str) -> (bool, Vec<String>) {
    let lower = path.replace('/', "\\").to_lowercase();
    let (network, rest) = if let Some(unc) = lower.strip_prefix("\\\\?\\unc\\") {
        (true, unc)
    } else if let Some(local) = lower
        .strip_prefix("\\\\?\\")
        .or_else(|| lower.strip_prefix("\\\\.\\"))
    {
        (false, local)
    } else if let Some(unc) = lower.strip_prefix("\\\\") {
        (true, unc)
    } else {
        (false, lower.as_str())
    };
    let mut parts: Vec<String> = Vec::new();
    for part in rest.split('\\') {
        match part.trim_end_matches([' ', '.']) {
            "" if part != ".." => {}
            _ if part == "." => {}
            _ if part == ".." => {
                if parts.len() > 1 {
                    parts.pop();
                }
            }
            trimmed => parts.push(trimmed.to_string()),
        }
    }
    (network, parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_from_the_approved_list() {
        use Guard::*;
        let cases = [
            (r"C:\Users\you\Downloads", Ok),
            ("c:/users/you/downloads/", Ok),
            (r"C:\Users\you\Downloads\Work", Ok),
            (r"D:\Sorted", Ok),
            (r"C:\Users\you\Desktop", Warn(Risk::Personal)),
            (r"C:\Users\you\Documents", Warn(Risk::Personal)),
            (r"\\server\share", Warn(Risk::Network)),
            (r"C:\", Block(Blocked::Root)),
            (r"D:\", Block(Blocked::Root)),
            ("C:", Block(Blocked::Root)),
            (r"C:\Users", Block(Blocked::Root)),
            (r"C:\Users\you", Block(Blocked::Root)),
            (r"C:\Windows\System32", Block(Blocked::System)),
            (
                r"C:\Program Files (x86)\Steam\steamapps\common",
                Block(Blocked::Games),
            ),
            (r"D:\SteamLibrary", Block(Blocked::Games)),
            (r"D:\Games", Block(Blocked::Games)),
            (r"C:\Program Files\Epic Games", Block(Blocked::Games)),
            (r"C:\Users\you\AppData\Roaming", Block(Blocked::System)),
        ];
        for (path, want) in cases {
            assert_eq!(guard_folder(path, false), want, "{path}");
        }
    }

    #[test]
    fn game_files_block_an_unknown_folder() {
        assert_eq!(
            guard_folder(r"E:\MyStuff", true),
            Guard::Block(Blocked::Games)
        );
    }

    #[test]
    fn canonical_windows_paths_are_not_mistaken_for_network_folders() {
        assert_eq!(
            guard_folder(r"\\?\C:\Windows", false),
            Guard::Block(Blocked::System)
        );
        assert_eq!(guard_folder(r"\\?\D:\", false), Guard::Block(Blocked::Root));
        assert_eq!(
            guard_folder(r"\\.\C:\Windows", false),
            Guard::Block(Blocked::System)
        );
        assert_eq!(guard_folder(r"\\?\D:\Sorted", false), Guard::Ok);
        assert_eq!(
            guard_folder(r"\\?\UNC\server\share", false),
            Guard::Warn(Risk::Network)
        );
    }

    #[test]
    fn path_tricks_do_not_escape_the_guard() {
        assert_eq!(
            guard_folder(r"C:\Users\you\Downloads\..\..\..\Windows", false),
            Guard::Block(Blocked::System)
        );
        assert_eq!(
            guard_folder(r"C:\Users\you\Downloads\..", false),
            Guard::Block(Blocked::Root)
        );
        assert_eq!(
            guard_folder(r"C:\Windows.\System32", false),
            Guard::Block(Blocked::System)
        );
        assert_eq!(
            guard_folder(r"C:\.\Windows", false),
            Guard::Block(Blocked::System)
        );
        assert_eq!(
            guard_folder(r"C:\\\Windows", false),
            Guard::Block(Blocked::System)
        );
    }

    #[test]
    fn incomplete_paths_are_refused() {
        for path in ["", "Downloads", r"C:Downloads", r"..\Windows"] {
            assert_eq!(
                guard_folder(path, false),
                Guard::Block(Blocked::Root),
                "{path}"
            );
        }
    }
}
