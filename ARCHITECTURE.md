# Architecture

## The parts

| Crate | Job | Touches files? |
|---|---|---|
| `ledger-core` | Decides what should happen. No dependencies and no unsafe code. | Never |
| `ledger-platform` | Reads facts about files and carries out moves. Windows only. | Yes |

Planned: `ledger-ai` for AI votes, and the Tauri `app` with the receipt screens.

Keeping the decisions apart from the disk means thousands of cases can be tested
instantly, and only one small crate needs careful review for file safety.

## How one file moves through the code

```text
sort::sort_item
 ├─ sort::check_folder     is this folder allowed?          ledger_core::guard
 ├─ inspect::inspect       gather facts, read-only
 ├─ judge::judge           Move, Hold, Wait or Skip         ledger_core::judge
 └─ mover::move_to         only when the decision is Move
     ├─ journal::intend    forced to disk first
     ├─ MoveFileExW        fails if the name is taken or the drive differs
     └─ journal::done      or abort; a taken name tries the next candidate
```

## Key files

- `crates/ledger-core/src/judge.rs`: the Ledger's rules and evidence points
- `crates/ledger-core/src/guard.rs`: which folders are refused
- `crates/ledger-platform/src/mover.rs`: the only code that changes the disk
- `crates/ledger-platform/src/journal.rs`: the crash record, stored as one tab-separated
  line per event
- `crates/ledger-platform/tests/safety.rs`: the promises in `SAFETY.md`, on real files

## Decisions

**No dependencies so far.** Each one is code trusted with people's files. The one
Windows call is declared directly, so there is no need for a large bindings crate.

**The journal is plain text.** After a problem, a person can open it and read what
happened. The rare damaged line can be skipped without losing the others.

**Names can be taken at any moment.** `name_candidates` only suggests names. The
no-replace rename decides, so another program taking a name cannot cause an overwrite.

**Windows first.** Other systems need their own no-replace move, shortcut rules and
guard lists, so `ledger-platform` refuses to compile on them rather than running unsafely.
