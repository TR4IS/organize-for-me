# Architecture

## The parts

| Crate | Job | Touches files? |
|---|---|---|
| `ledger-core` | Decides what should happen. No dependencies and no unsafe code. | Never |
| `ledger-platform` | Reads facts about files, watches a folder and carries out moves. Windows only. | Yes |
| `app` | The running app: receipt, log, settings, window and tray, using Tauri. | Only through `ledger-platform`, plus its own data |
| `ui` | The receipt screens, in plain HTML, CSS and JavaScript with no build step. | Never |

Planned: `ledger-ai` for AI votes.

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

## How the app runs

A background thread calls `Engine::tick` twice a second. The watcher lists the folder, and
any item that has stopped changing goes through the path above. When something changes, the
engine sends the whole receipt to the screens, which redraw it. Buttons call commands in
`app/src/main.rs`, which return the new state.

The screens make no decisions. Reasons arrive as English text keys, and `ui/app.js`
translates them.

## Key files

- `crates/ledger-core/src/judge.rs`: the Ledger's rules and evidence points
- `crates/ledger-core/src/guard.rs`: which folders are refused
- `crates/ledger-platform/src/mover.rs`: the only code that changes the disk
- `crates/ledger-platform/src/journal.rs`: the crash record, stored as one tab-separated
  line per event
- `crates/ledger-platform/src/watch.rs`: how new items are noticed
- `crates/ledger-platform/tests/safety.rs`: the promises in `SAFETY.md`, on real files
- `app/src/engine.rs`: the receipt, log and watching loop, testable with no window
- `app/src/main.rs`: the window, the tray and the commands the screens call
- `ui/app.js`: all text in both languages, and the drawing

## Decisions

**No dependencies in the engine.** Each one is code trusted with people's files. The one
Windows call is declared directly, so there is no need for a large bindings crate. Tauri
and its plugins stay in `app`, away from the code that moves files.

**Polling, not change notifications.** Windows notifications can overflow and drop events in
a burst. A listing twice a second is cheap, and it cannot miss a file that is still there.

**No frontend build step.** The screens are plain files, so there is no npm supply chain and
nothing to compile.

**Only new files.** Sorting what is already in a folder is how an organizer hurts people, so
it is not a default. It could return later as a clear, separate action with a preview.

**The journal is plain text.** After a problem, a person can open it and read what
happened. The rare damaged line can be skipped without losing the others.

**Names can be taken at any moment.** `name_candidates` only suggests names. The
no-replace rename decides, so another program taking a name cannot cause an overwrite.

**Windows first.** Other systems need their own no-replace move, shortcut rules and
guard lists, so `ledger-platform` refuses to compile on them rather than running unsafely.
