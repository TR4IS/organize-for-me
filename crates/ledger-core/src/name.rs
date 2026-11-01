//! File name rules: extensions, hidden direction characters and free names.

/// NTFS allows 255 UTF-16 units in one file name.
const MAX_NAME_UNITS: usize = 255;
/// Stops the search for a free name, so a broken folder cannot loop forever.
const MAX_COPIES: u32 = 9999;
/// A longer "extension" is not really one, so it is not kept whole when renaming.
const MAX_KEPT_EXT_UNITS: usize = 32;

/// Returns the lowercase extension, or an empty string.
///
/// A leading dot (`.env`) or a trailing dot (`notes.`) does not make an extension.
pub fn ext_of(name: &str) -> String {
    match name.rfind('.') {
        Some(i) if i > 0 && i + 1 < name.len() => name[i + 1..].to_lowercase(),
        _ => String::new(),
    }
}

/// True when the name holds characters that can make it display in a false order.
///
/// `photo\u{202e}gpj.exe` displays as `photoexe.jpg`, which hides a program.
pub fn hides_direction(name: &str) -> bool {
    name.chars().any(|c| {
        matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{200e}' | '\u{200f}')
    })
}

/// True when Windows treats the name as a device, whatever its extension.
pub fn is_reserved(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or("").to_lowercase();
    match stem.as_str() {
        "con" | "prn" | "aux" | "nul" => true,
        _ => {
            let bytes = stem.as_bytes();
            bytes.len() == 4
                && (stem.starts_with("com") || stem.starts_with("lpt"))
                && bytes[3].is_ascii_digit()
        }
    }
}

/// Yields `name`, then `name (1)`, `name (2)` and so on, each within the NTFS limit.
///
/// These are only candidates. The real protection is the no-replace move in
/// `ledger-platform`, which refuses a taken name even if another program just took it.
pub fn name_candidates(name: &str) -> impl Iterator<Item = String> + '_ {
    let ext = ext_of(name);
    let keep_ext = !ext.is_empty() && ext.encode_utf16().count() <= MAX_KEPT_EXT_UNITS;
    let (stem, dotted_ext) = if keep_ext {
        let split = name.len() - ext.len() - 1;
        (&name[..split], &name[split..])
    } else {
        (name, "")
    };
    std::iter::once(name.to_string()).chain((1..=MAX_COPIES).map(move |i| {
        let tag = format!(" ({i}){dotted_ext}");
        let room = MAX_NAME_UNITS.saturating_sub(tag.encode_utf16().count());
        format!("{}{tag}", clip_units(stem, room))
    }))
}

/// Cuts text to a number of UTF-16 units without splitting a character.
fn clip_units(text: &str, limit: usize) -> &str {
    let mut used = 0;
    for (at, c) in text.char_indices() {
        used += c.len_utf16();
        if used > limit {
            return &text[..at];
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nth(name: &str, n: usize) -> String {
        name_candidates(name).nth(n).unwrap()
    }

    #[test]
    fn extensions() {
        assert_eq!(ext_of("SCAN.PDF"), "pdf");
        assert_eq!(ext_of("backup.tar.gz"), "gz");
        assert_eq!(ext_of(".env"), "");
        assert_eq!(ext_of("notes."), "");
        assert_eq!(ext_of("README"), "");
    }

    #[test]
    fn reserved_names() {
        for name in ["CON.pdf", "con", "nul.txt", "COM1", "lpt9.log"] {
            assert!(is_reserved(name), "{name}");
        }
        for name in ["console.pdf", "community", "com.txt", "lpt10", "COMA"] {
            assert!(!is_reserved(name), "{name}");
        }
    }

    #[test]
    fn hidden_direction_is_detected() {
        assert!(hides_direction("photo\u{202e}gpj.exe"));
        assert!(!hides_direction("تقرير المبيعات.pdf"));
    }

    #[test]
    fn copies_keep_the_extension() {
        assert_eq!(nth("report.pdf", 0), "report.pdf");
        assert_eq!(nth("report.pdf", 1), "report (1).pdf");
        assert_eq!(nth("a.pdf", 3), "a (3).pdf");
        assert_eq!(nth("REPORT.PDF", 1), "REPORT (1).PDF");
        assert_eq!(nth(".env", 1), ".env (1)");
        assert_eq!(nth("README", 2), "README (2)");
    }

    #[test]
    fn a_full_length_name_stays_within_the_limit() {
        let long = format!("{}.pdf", "y".repeat(251));
        let copy = nth(&long, 1);
        assert_eq!(copy.encode_utf16().count(), 255);
        assert!(copy.ends_with(" (1).pdf"));
    }

    #[test]
    fn cutting_never_splits_arabic_or_emoji() {
        for long in [
            format!("{}.pdf", "ت".repeat(251)),
            format!("{}.pdf", "🎉".repeat(125)),
        ] {
            let copy = nth(&long, 1);
            assert!(copy.encode_utf16().count() <= 255);
            assert!(copy.ends_with(" (1).pdf"));
        }
    }

    #[test]
    fn a_huge_fake_extension_stays_within_the_limit() {
        let long = format!("a.{}", "b".repeat(253));
        assert!(nth(&long, 1).encode_utf16().count() <= 255);
    }

    #[test]
    fn the_search_for_a_free_name_ends() {
        assert_eq!(name_candidates("a.pdf").count(), 10_000);
    }
}
