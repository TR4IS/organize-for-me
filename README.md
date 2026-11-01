# Organize for Me / رتب لي

A Windows app that sorts your downloads into folders and shows every move on a
receipt. It never deletes a file, never overwrites one, and every move can be put back.

**Status: early development, not released.** The decision rules and the safe file
mover work and are tested. There is no app window, folder watcher or AI connection yet.

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
