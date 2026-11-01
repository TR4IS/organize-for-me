//! The Ledger's decision: given what is known about an item, what should happen?

use crate::kind::{Bin, bin_for_ext, is_zip_package};
use crate::memory::{Learned, Memory};
use crate::name::{ext_of, hides_direction, is_reserved};

const POINTS_CONTENTS: i32 = 60;
const POINTS_EXTENSION: i32 = 40;
const POINTS_CHOICE: i32 = 50;
const POINTS_SITE: i32 = 10;
const POINTS_FOLDER_SETTING: i32 = 50;

const SHORTCUT_EXTS: [&str; 5] = ["lnk", "url", "pif", "appref-ms", "website"];
const PARTIAL_EXTS: [&str; 6] = ["crdownload", "part", "tmp", "temp", "download", "partial"];
const SYSTEM_FILES: [&str; 3] = ["desktop.ini", "thumbs.db", ".ds_store"];

/// What sort of item this is on disk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    /// A normal file.
    File,
    /// A folder.
    Folder,
    /// A symbolic link or junction. Never moved, because it can point anywhere.
    Link,
}

/// Everything known about an item, gathered by `ledger-platform`.
#[derive(Clone, Debug)]
pub struct Facts {
    /// The file name, without its folder.
    pub name: String,
    /// Size in bytes.
    pub bytes: u64,
    /// File, folder or link.
    pub kind: ItemKind,
    /// The hidden or system attribute is set.
    pub hidden: bool,
    /// Another program has it open.
    pub in_use: bool,
    /// The type its first bytes prove, if any.
    pub contents: Option<Bin>,
    /// The website it was downloaded from, if Windows recorded one.
    pub host: Option<String>,
}

/// User settings that change decisions.
#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    /// Also move new folders. Off by default because it can break projects.
    pub move_folders: bool,
}

/// Why an item is left alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkipReason {
    /// One of the app's own labeled folders.
    DestinationFolder,
    /// A folder, while moving folders is off.
    FoldersOff,
    /// A symbolic link or junction.
    Link,
    /// A download that has not finished.
    Unfinished,
    /// A Windows or Office system file.
    SystemFile,
    /// A hidden file.
    Hidden,
    /// A shortcut, which moving could break.
    Shortcut,
    /// A name Windows reserves.
    ReservedName,
}

/// Why an item waits for the user.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoldReason {
    /// It looks like a program pretending to be something else.
    DisguisedProgram,
    /// Its name and its contents point to different types.
    NameContentsDisagree,
    /// The user undid a move from this website before.
    VoidedSource,
}

/// The decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Move it to the verdict's folder.
    Move,
    /// Keep it where it is and ask the user.
    Hold(HoldReason),
    /// In use, so try again later.
    Wait,
    /// Leave it alone.
    Skip(SkipReason),
}

/// One piece of evidence shown on the receipt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Evidence {
    /// The bytes inside the file.
    Contents(Bin),
    /// The file extension.
    Extension(String),
    /// A package that is a ZIP inside, as expected.
    Package,
    /// The website it came from.
    Site(String),
    /// A folder choice the user made for this website.
    LearnedChoice(String),
    /// A void the user made for this website.
    LearnedVoid(String),
    /// The move-folders setting is on.
    FolderSetting,
    /// Nothing points to a type.
    Nothing,
}

/// A note shown with the receipt item. It does not change the decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Note {
    /// The name holds characters that hide its real order.
    HiddenDirection,
    /// The file has no contents.
    Empty,
}

/// The result of judging one item.
#[derive(Clone, Debug)]
pub struct Verdict {
    /// What should happen.
    pub decision: Decision,
    /// The folder the evidence points to.
    pub bin: Bin,
    /// The folder the evidence pointed to before any learned choice.
    pub evidence_bin: Bin,
    /// Why, in order.
    pub evidence: Vec<Evidence>,
    /// Extra notes.
    pub notes: Vec<Note>,
    /// How sure the evidence is, from 0 to 100.
    pub confidence: u8,
}

/// Decides what should happen to an item. This never touches the disk.
pub fn judge(facts: &Facts, memory: &Memory, options: Options) -> Verdict {
    let mut verdict = Verdict {
        decision: Decision::Move,
        bin: Bin::Random,
        evidence_bin: Bin::Random,
        evidence: Vec::new(),
        notes: Vec::new(),
        confidence: 0,
    };
    let skip = |mut v: Verdict, reason| {
        v.decision = Decision::Skip(reason);
        v
    };
    let lower = facts.name.to_lowercase();
    let ext = ext_of(&facts.name);

    match facts.kind {
        ItemKind::Link => return skip(verdict, SkipReason::Link),
        ItemKind::Folder => {
            if Bin::ALL.iter().any(|b| b.folder() == lower) {
                return skip(verdict, SkipReason::DestinationFolder);
            }
            if !options.move_folders {
                return skip(verdict, SkipReason::FoldersOff);
            }
            verdict.evidence.push(Evidence::FolderSetting);
            verdict.confidence = POINTS_FOLDER_SETTING as u8;
            return verdict;
        }
        ItemKind::File => {}
    }

    if PARTIAL_EXTS.contains(&ext.as_str()) {
        return skip(verdict, SkipReason::Unfinished);
    }
    if SYSTEM_FILES.contains(&lower.as_str()) || lower.starts_with("~$") {
        return skip(verdict, SkipReason::SystemFile);
    }
    if facts.hidden {
        return skip(verdict, SkipReason::Hidden);
    }
    if SHORTCUT_EXTS.contains(&ext.as_str()) {
        return skip(verdict, SkipReason::Shortcut);
    }
    if is_reserved(&facts.name) {
        return skip(verdict, SkipReason::ReservedName);
    }
    if facts.in_use {
        verdict.decision = Decision::Wait;
        return verdict;
    }

    let by_ext = bin_for_ext(&ext);
    let package = facts.contents == Some(Bin::Zip) && is_zip_package(&ext);
    let by_contents = if package { None } else { facts.contents };

    let spoofed = hides_direction(&facts.name);
    let disguised = by_contents == Some(Bin::Exe) && by_ext != Some(Bin::Exe);
    let double_ext = by_ext == Some(Bin::Exe) && inner_ext_is_not_program(&facts.name);
    if spoofed {
        verdict.notes.push(Note::HiddenDirection);
    }
    if spoofed || disguised || double_ext {
        verdict.bin = Bin::Exe;
        verdict.evidence_bin = Bin::Exe;
        verdict.decision = Decision::Hold(HoldReason::DisguisedProgram);
        return verdict;
    }
    if let (Some(named), Some(found)) = (by_ext, by_contents)
        && named != found
    {
        verdict.bin = found;
        verdict.evidence_bin = found;
        verdict.evidence.push(Evidence::Extension(ext));
        verdict.evidence.push(Evidence::Contents(found));
        verdict.decision = Decision::Hold(HoldReason::NameContentsDisagree);
        return verdict;
    }

    let mut points = 0;
    if let Some(bin) = by_ext {
        verdict.bin = bin;
        verdict.evidence.push(Evidence::Extension(ext));
        points += POINTS_EXTENSION;
    }
    if let Some(bin) = by_contents {
        verdict.bin = bin;
        verdict.evidence.push(Evidence::Contents(bin));
        points += POINTS_CONTENTS;
    }
    if package {
        verdict.evidence.push(Evidence::Package);
        points += POINTS_CONTENTS;
    }
    if verdict.evidence.is_empty() {
        verdict.evidence.push(Evidence::Nothing);
    }
    verdict.evidence_bin = verdict.bin;

    if let Some(host) = &facts.host {
        match memory.get(host, verdict.bin) {
            Some(Learned::Ask) => {
                verdict.evidence.push(Evidence::LearnedVoid(host.clone()));
                verdict.decision = Decision::Hold(HoldReason::VoidedSource);
            }
            Some(Learned::RouteTo(to)) => {
                verdict.bin = to;
                verdict.evidence.push(Evidence::LearnedChoice(host.clone()));
                points += POINTS_CHOICE;
            }
            None => {
                verdict.evidence.push(Evidence::Site(host.clone()));
                points += POINTS_SITE;
            }
        }
    }
    if facts.bytes == 0 {
        verdict.notes.push(Note::Empty);
    }
    verdict.confidence = points.clamp(0, 100) as u8;
    verdict
}

/// True for names like `invoice.pdf.exe`.
fn inner_ext_is_not_program(name: &str) -> bool {
    let mut parts = name.rsplit('.');
    parts.next();
    let inner = parts.next();
    let has_more = parts.next().is_some();
    has_more
        && inner
            .and_then(|i| bin_for_ext(&i.to_lowercase()))
            .is_some_and(|b| b != Bin::Exe)
}

#[cfg(test)]
mod tests {
    use super::*;
    use Decision::{Hold, Move, Skip, Wait};

    fn file(name: &str, contents: Option<Bin>) -> Facts {
        Facts {
            name: name.into(),
            bytes: 1024,
            kind: ItemKind::File,
            hidden: false,
            in_use: false,
            contents,
            host: None,
        }
    }

    fn run(facts: &Facts) -> Verdict {
        judge(facts, &Memory::default(), Options::default())
    }

    #[test]
    fn names_of_every_shape_are_sorted() {
        let long = format!("{}.pdf", "Annual-Report-".repeat(12));
        let cases: Vec<(&str, Option<Bin>, Bin)> = vec![
            (&long, Some(Bin::Pdf), Bin::Pdf),
            ("README", None, Bin::Random),
            (".env", None, Bin::Random),
            ("notes.", None, Bin::Random),
            ("SCAN.PDF", Some(Bin::Pdf), Bin::Pdf),
            ("backup.tar.gz", Some(Bin::Zip), Bin::Zip),
            ("my file & (draft) #2!.docx", Some(Bin::Zip), Bin::Doc),
            ("تقرير المبيعات النهائي.pdf", Some(Bin::Pdf), Bin::Pdf),
            ("🎉 party 🎉.jpg", Some(Bin::Image), Bin::Image),
            ("二零二六年度财务报告.pdf", Some(Bin::Pdf), Bin::Pdf),
            ("empty.txt", None, Bin::Doc),
            ("app.jar", Some(Bin::Zip), Bin::Random),
            ("installer.msi", None, Bin::Exe),
        ];
        for (name, contents, bin) in cases {
            let verdict = run(&file(name, contents));
            assert_eq!(verdict.decision, Move, "{name}");
            assert_eq!(verdict.bin, bin, "{name}");
        }
    }

    #[test]
    fn disguised_programs_are_held() {
        for (name, contents) in [
            ("invoice.pdf.exe", Some(Bin::Exe)),
            ("invoice.pdf.scr", None),
            ("INVOICE.PDF.EXE", Some(Bin::Exe)),
            ("photo.jpg", Some(Bin::Exe)),
            ("no_extension", Some(Bin::Exe)),
            ("photo\u{202e}gpj.exe", Some(Bin::Exe)),
        ] {
            assert_eq!(
                run(&file(name, contents)).decision,
                Hold(HoldReason::DisguisedProgram),
                "{name}"
            );
        }
    }

    #[test]
    fn real_programs_are_not_held() {
        for name in ["setup.exe", "my.app.v2.exe", "tool.1.2.3.exe"] {
            assert_eq!(run(&file(name, Some(Bin::Exe))).decision, Move, "{name}");
        }
    }

    #[test]
    fn name_and_contents_disagreeing_is_held() {
        let verdict = run(&file("report.pdf", Some(Bin::Image)));
        assert_eq!(verdict.decision, Hold(HoldReason::NameContentsDisagree));
        assert_eq!(verdict.bin, Bin::Image);
        let zip = run(&file("report.pdf", Some(Bin::Zip)));
        assert_eq!(zip.decision, Hold(HoldReason::NameContentsDisagree));
    }

    #[test]
    fn items_left_alone() {
        let cases = [
            ("movie.mp4.crdownload", SkipReason::Unfinished),
            ("movie.mp4.part", SkipReason::Unfinished),
            ("desktop.ini", SkipReason::SystemFile),
            ("Thumbs.db", SkipReason::SystemFile),
            ("~$budget.xlsx", SkipReason::SystemFile),
            ("App.lnk", SkipReason::Shortcut),
            ("GAME.LNK", SkipReason::Shortcut),
            ("Elden Ring.url", SkipReason::Shortcut),
            ("Old App.pif", SkipReason::Shortcut),
            ("Tool.appref-ms", SkipReason::Shortcut),
            ("Site.website", SkipReason::Shortcut),
            ("CON.pdf", SkipReason::ReservedName),
        ];
        for (name, reason) in cases {
            assert_eq!(run(&file(name, None)).decision, Skip(reason), "{name}");
        }
        let mut hidden = file("secret.txt", None);
        hidden.hidden = true;
        assert_eq!(run(&hidden).decision, Skip(SkipReason::Hidden));
    }

    #[test]
    fn folders_and_links() {
        let item = |name: &str, kind| Facts {
            kind,
            ..file(name, None)
        };
        let on = Options { move_folders: true };
        let none = Memory::default();
        assert_eq!(
            run(&item("Project", ItemKind::Folder)).decision,
            Skip(SkipReason::FoldersOff)
        );
        assert_eq!(
            judge(&item("Project", ItemKind::Folder), &none, on).decision,
            Move
        );
        assert_eq!(
            judge(&item("PDF", ItemKind::Folder), &none, on).decision,
            Skip(SkipReason::DestinationFolder)
        );
        assert_eq!(
            judge(&item("Junction", ItemKind::Link), &none, on).decision,
            Skip(SkipReason::Link)
        );
    }

    #[test]
    fn a_file_in_use_waits() {
        let mut facts = file("setup.exe", Some(Bin::Exe));
        facts.in_use = true;
        assert_eq!(run(&facts).decision, Wait);
    }

    #[test]
    fn evidence_adds_up_and_is_capped() {
        let mut facts = file("a.pdf", Some(Bin::Pdf));
        assert_eq!(run(&facts).confidence, 100);
        facts.contents = None;
        assert_eq!(run(&facts).confidence, 40);
        facts.host = Some("bank.com".into());
        assert_eq!(run(&facts).confidence, 50);
    }

    #[test]
    fn a_void_holds_the_next_file_from_that_site() {
        let mut memory = Memory::default();
        memory.voided("BANK.com", Bin::Pdf);
        let mut facts = file("b.pdf", Some(Bin::Pdf));
        facts.host = Some("bank.com".into());
        let verdict = judge(&facts, &memory, Options::default());
        assert_eq!(verdict.decision, Hold(HoldReason::VoidedSource));
    }

    #[test]
    fn a_choice_clears_the_hold_and_routes_the_next_file() {
        let mut memory = Memory::default();
        memory.voided("bank.com", Bin::Pdf);
        memory.chose("bank.com", Bin::Pdf, Bin::Doc);
        let mut facts = file("c.pdf", Some(Bin::Pdf));
        facts.host = Some("bank.com".into());
        let verdict = judge(&facts, &memory, Options::default());
        assert_eq!(verdict.decision, Move);
        assert_eq!(verdict.bin, Bin::Doc);
        assert_eq!(verdict.evidence_bin, Bin::Pdf);
    }

    #[test]
    fn a_learned_choice_never_clears_a_disguised_program() {
        let mut memory = Memory::default();
        memory.chose("site.com", Bin::Exe, Bin::Exe);
        let mut facts = file("invoice.pdf.exe", Some(Bin::Exe));
        facts.host = Some("site.com".into());
        let verdict = judge(&facts, &memory, Options::default());
        assert_eq!(verdict.decision, Hold(HoldReason::DisguisedProgram));
    }

    #[test]
    fn an_empty_file_gets_a_note() {
        let mut facts = file("empty.txt", None);
        facts.bytes = 0;
        assert!(run(&facts).notes.contains(&Note::Empty));
    }
}
