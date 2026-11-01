//! What type a file is, judged from its extension and from its first bytes.

/// The labeled folder a file belongs in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Bin {
    /// PDF documents.
    Pdf,
    /// Text and office documents.
    Doc,
    /// Pictures.
    Image,
    /// Videos.
    Video,
    /// Audio.
    Sound,
    /// Archives.
    Zip,
    /// Programs and installers.
    Exe,
    /// Anything without clear evidence.
    Random,
}

impl Bin {
    /// Every labeled folder.
    pub const ALL: [Bin; 8] = [
        Bin::Pdf,
        Bin::Doc,
        Bin::Image,
        Bin::Video,
        Bin::Sound,
        Bin::Zip,
        Bin::Exe,
        Bin::Random,
    ];

    /// Finds a labeled folder by its name on disk.
    pub fn from_folder(name: &str) -> Option<Bin> {
        Bin::ALL.into_iter().find(|bin| bin.folder() == name)
    }

    /// The folder name on disk.
    pub fn folder(self) -> &'static str {
        match self {
            Bin::Pdf => "pdf",
            Bin::Doc => "doc",
            Bin::Image => "image",
            Bin::Video => "video",
            Bin::Sound => "sound",
            Bin::Zip => "zip",
            Bin::Exe => "exe",
            Bin::Random => "random",
        }
    }
}

/// Returns the folder an extension points to.
pub fn bin_for_ext(ext: &str) -> Option<Bin> {
    Some(match ext {
        "pdf" => Bin::Pdf,
        "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp" | "txt"
        | "rtf" | "csv" | "md" => Bin::Doc,
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "heic" | "avif" | "svg" | "bmp" => Bin::Image,
        "mp4" | "m4v" | "mkv" | "mov" | "webm" | "avi" => Bin::Video,
        "mp3" | "wav" | "flac" | "m4a" | "ogg" | "aac" => Bin::Sound,
        "zip" | "rar" | "7z" | "gz" | "tar" => Bin::Zip,
        "exe" | "msi" | "scr" | "com" | "bat" | "cmd" | "ps1" | "vbs" => Bin::Exe,
        _ => return None,
    })
}

/// True for formats that are really ZIP files inside, so a ZIP signature is expected.
pub fn is_zip_package(ext: &str) -> bool {
    matches!(
        ext,
        "docx" | "xlsx" | "pptx" | "odt" | "ods" | "odp" | "jar" | "apk" | "epub"
    )
}

/// Identifies a file from its first bytes. Read at least 512 bytes when available.
///
/// Returns `None` when the bytes prove nothing. Old Office files and MSI installers
/// share one signature, for example, so that signature is treated as no evidence.
pub fn sniff(head: &[u8]) -> Option<Bin> {
    let starts = |magic: &[u8]| head.starts_with(magic);
    let at = |offset: usize, magic: &[u8]| head.get(offset..).is_some_and(|h| h.starts_with(magic));

    if starts(b"MZ") {
        Some(Bin::Exe)
    } else if starts(b"%PDF-") {
        Some(Bin::Pdf)
    } else if starts(b"\x89PNG\r\n\x1a\n")
        || starts(b"\xff\xd8\xff")
        || starts(b"GIF87a")
        || starts(b"GIF89a")
        || (starts(b"RIFF") && at(8, b"WEBP"))
    {
        Some(Bin::Image)
    } else if at(4, b"ftyp") {
        Some(match head.get(8..12) {
            Some(b"heic" | b"heix" | b"mif1" | b"msf1" | b"avif") => Bin::Image,
            Some(b"M4A " | b"M4B ") => Bin::Sound,
            _ => Bin::Video,
        })
    } else if starts(b"\x1a\x45\xdf\xa3") || (starts(b"RIFF") && at(8, b"AVI ")) {
        Some(Bin::Video)
    } else if starts(b"ID3")
        || starts(b"\xff\xfb")
        || starts(b"\xff\xf3")
        || starts(b"\xff\xf2")
        || starts(b"fLaC")
        || (starts(b"RIFF") && at(8, b"WAVE"))
    {
        Some(Bin::Sound)
    } else if starts(b"PK\x03\x04")
        || starts(b"PK\x05\x06")
        || starts(b"Rar!\x1a\x07")
        || starts(b"7z\xbc\xaf\x27\x1c")
        || starts(b"\x1f\x8b")
        || at(257, b"ustar")
    {
        Some(Bin::Zip)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signatures() {
        let cases: &[(&[u8], Option<Bin>)] = &[
            (b"%PDF-1.7", Some(Bin::Pdf)),
            (b"\x89PNG\r\n\x1a\nxxxx", Some(Bin::Image)),
            (b"\xff\xd8\xff\xe0", Some(Bin::Image)),
            (b"RIFF\0\0\0\0WEBPVP8", Some(Bin::Image)),
            (b"\0\0\0\x18ftypisom", Some(Bin::Video)),
            (b"\0\0\0\x18ftypheic", Some(Bin::Image)),
            (b"\0\0\0\x18ftypM4A ", Some(Bin::Sound)),
            (b"\x1a\x45\xdf\xa3", Some(Bin::Video)),
            (b"ID3\x04", Some(Bin::Sound)),
            (b"RIFF\0\0\0\0WAVEfmt", Some(Bin::Sound)),
            (b"PK\x03\x04", Some(Bin::Zip)),
            (b"\x1f\x8b\x08", Some(Bin::Zip)),
            (b"MZ\x90\0", Some(Bin::Exe)),
            (b"plain words", None),
            (b"", None),
            (b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1", None),
        ];
        for (head, want) in cases {
            assert_eq!(sniff(head), *want, "{head:?}");
        }
    }

    #[test]
    fn tar_signature_sits_deep_in_the_file() {
        let mut head = vec![0u8; 512];
        head[257..262].copy_from_slice(b"ustar");
        assert_eq!(sniff(&head), Some(Bin::Zip));
    }
}
