# Releasing

## One-time setup

1. Turn on multi-factor login for GitHub. SignPath requires it for every team member.
2. Push the code to the public repository named in `REPO_URL` (`app/src/main.rs`).
3. Apply at <https://signpath.org/apply>. SignPath reviews the project before accepting it.
4. In SignPath:
   - Turn on multi-factor login.
   - Link the repository and install the SignPath GitHub app.
   - Paste `.signpath/artifact-configuration.xml` as the project's artifact configuration.
   - Create an API token for the CI user, and require manual approval on the signing policy.
5. In the GitHub repository settings, under Secrets and variables, Actions, add:
   - Secret `SIGNPATH_API_TOKEN`
   - Variables `SIGNPATH_ORGANIZATION_ID`, `SIGNPATH_PROJECT_SLUG` and `SIGNPATH_POLICY_SLUG`

## Each release

1. Set the same version in `app/tauri.conf.json` and `app/Cargo.toml`.
   If a dependency changed, run `cargo about generate about.hbs -o THIRD-PARTY-LICENSES.html`.
2. Commit, then tag it `v` plus that version, and push the tag.
3. Approve the signing request in SignPath. The workflow waits up to 6 hours.
4. Check the draft release on GitHub, then publish it.

## Before the first stable 3.x release

Version 2 installs updates by itself from the newest release that is not a pre-release.
Mark every 3.x beta as a pre-release, so version 2 users are left alone. Before a stable
3.x, decide how they should move over, because version 2 expects files this app does not
publish. `docs/version.json` also belongs to version 2, so leave it in place.

## What gets signed

Only the installer. The app inside it and the uninstaller stay unsigned, because Tauri
packs them into the installer during the build, and the workflow signs the finished file.
