# Organize for Me / رتب لي

A Windows app that sorts your downloads into folders and shows every move on a
receipt. It never deletes a file, never overwrites one, and every move can be put back.

**Status: beta, not released.** The app runs, sorts real files and has an installer.
It can check GitHub for a new version when you ask, or at each start if you turn that
on, and it only opens the release page. There is no AI connection yet. Signing is set
up but has not been approved yet, so Windows may warn before running the installer.

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

## Code signing policy

Free code signing provided by [SignPath.io](https://about.signpath.io), certificate by
[SignPath Foundation](https://signpath.org).

- Committers and reviewers: [TR4IS](https://github.com/TR4IS)
- Approvers: [TR4IS](https://github.com/TR4IS)

Privacy: this program will not transfer any information to other networked systems
unless specifically requested by the user or the person installing or operating it.
The update check is off by default. When used, it asks GitHub for the latest release,
and [GitHub's privacy statement](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement) applies to that request.

See [RELEASING.md](RELEASING.md) for how a release is built and signed.

## License

[MIT](LICENSE).

## Where to read next

- [SAFETY.md](SAFETY.md): the promises the app makes, and the tests that hold it to them
- [ARCHITECTURE.md](ARCHITECTURE.md): how the code fits together
- [CONTRIBUTING.md](CONTRIBUTING.md): the rules for changing it

AI tools were used to help build this app.
