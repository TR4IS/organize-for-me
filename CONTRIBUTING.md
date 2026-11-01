# Contributing

## Before you commit

```bash
cargo fmt --all
```

```bash
cargo clippy --workspace --all-targets
```

```bash
cargo test --workspace
```

All three must pass. Public items must be documented, or the build fails.

## Rules

- **All disk changes go through `mover.rs`.** Do not add another way to move, rename,
  delete or write user files. The linter blocks the common standard library calls.
- **New safety behavior needs a test on real files**, added to `safety.rs` and named
  in `SAFETY.md`.
- **Decisions belong in `ledger-core`**, where they can be tested without a disk.
- **Comments say why**, not what the next line does.
- **Add a dependency only when it clearly earns its place**, and say why in the commit.

## Testing screens

`cargo run -p organize-for-me` runs the app. In debug builds, set `OFM_DATA_DIR` to a
temporary folder to keep test receipts apart from your real ones. Screens contain no
decisions, so a rule belongs in a Rust test, not a screen test.

## Adding a file type

Add the extension to `bin_for_ext` in `kind.rs`, and its signature to `sniff` if it has a
reliable one. Add a test for both. Leave a signature out if several types share it.

## Adding an operating system

Windows-only behavior lives in `ledger-platform`. A new system needs its own no-replace
rename, shortcut rules, guard lists and a passing `safety.rs` on real hardware before
anyone is offered it. A plain rename is not acceptable, because it can replace files.
