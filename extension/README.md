# Organize for Me: browser extension

When Organize for Me sorts a download, the browser's own downloads list still points at the old
place and calls the file "Deleted". This extension adds a toolbar button with its own list of recent
downloads, showing where each one is now, with **Open** and **Show file** buttons that still work.

It works in Chrome, Edge, Brave, Thorium and other Chromium browsers.

## How it finds a file

The extension asks the app over the browser's native messaging. The installer registers the app for
this, under each browser's `NativeMessagingHosts` key. Uninstalling removes those keys again.

The browser starts `organize-for-me.exe` with the extension's address. In that mode the app only
reads its move journal (`app/src/chrome_host.rs`). It never moves a file and never opens its window.

- **Where:** the extension sends the download paths and gets back where each one is now.
- **Open** and **Show file:** these only work for files the journal moved. Programs (`.exe`, `.msi`,
  scripts) are only shown in their folder, never started from here.
- Only this extension's ID may connect (`allowed_origins` in `app/windows/chrome-host.json`).

Without the app installed, the list still works for files that never moved.

## Try it

1. Install Organize for Me 3.0.0-beta.7 or newer. The installer puts the extension in the app's own
   folder, at `%LOCALAPPDATA%\Organize for Me\extension`.
2. In the app, open Settings and click **Browser extension**. It opens that folder.
3. Open `chrome://extensions`, turn on **Developer mode**, click **Load unpacked**, and choose that
   folder.
4. Pin the extension and click it.

Load it from the app's folder, not from Downloads. The browser runs it straight from the folder you
pick, so if that folder is moved or deleted, the extension disappears. In the app's folder it is out
of the way, and each app update brings the matching version. Restart the browser after an update to
pick it up.

The `key` in `manifest.json` fixes the extension's ID to `bafeecmlihhlblhcdpabbnlhmblgocdm`, which
is the ID the app trusts. If the extension is published to the Chrome Web Store, add the store's ID
to `allowed_origins` as well.
