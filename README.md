# Organize for Me / رتب لي

A Windows app that sorts your downloads into folders and shows every move on a
receipt. It never deletes a file, never overwrites one, and every move can be put back.

**Status: beta, not released.** The app runs, sorts real files and has an installer.
There is no update check or AI connection yet, and the installer is not code signed,
so Windows may warn before running it.

## Build the installer

You need [Rust](https://rustup.rs) and the Tauri build tool (`cargo install tauri-cli --locked`).

```bash
cargo tauri build
```

The installer lands in `target\release\bundle\nsis`. It installs for the current user,
so it needs no administrator rights.

## Run the app

You need [Rust](https://rustup.rs) and Windows, with WebView2 (included in Windows 11).

```bash
cargo run -p organize-for-me
```

Only files that arrive after you press Start are sorted. Files already in the folder
stay where they are.

## Run the tests

You need [Rust](https://rustup.rs) and Windows.

```bash
cargo test --workspace
```

Before committing:

```bash
cargo fmt --all
```

```bash
cargo clippy --workspace --all-targets
```

The tests only touch temporary folders under `target\tmp`. If a second drive is
available, one test also creates and removes a temporary folder on it.

## Where to read next

- [SAFETY.md](SAFETY.md): the promises the app makes, and the tests that hold it to them
- [ARCHITECTURE.md](ARCHITECTURE.md): how the code fits together
- [CONTRIBUTING.md](CONTRIBUTING.md): the rules for changing it

AI tools were used to help build this app.
