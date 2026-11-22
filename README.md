# PookieClip

A private clipboard history for Linux. PookieClip keeps everything you copy on your own machine, so you can find it and copy it again later: text, links, code, colors, images and files.

It is built as a better GPaste. Copied files are saved as PookieClip's own copies, so an entry still pastes long after the original was moved, renamed or deleted.

## Features

- Search your history and filter it by type: text, links, code, colors, images and files
- Copied images and screenshots are saved with a preview, and paste back as the same image
- Click an entry, or press Enter, to put it back on the clipboard
- Pin entries you want to keep. Pinned entries are never removed automatically
- Copied files and folders show as a tree. Copy everything, one file, or a selection
- PookieClip keeps its own copies of copied files and images, up to 10 GB, so they still paste after the originals are gone. The oldest unpinned copies are dropped first when space runs out
- Copies made by password managers are skipped and never saved
- Undo after removing an entry or clearing the history
- Open and hide the panel from the top bar icon or with **Ctrl+Alt+H**

## Privacy

Your clipboard history never leaves your machine. PookieClip makes no network requests: the fonts are bundled, and links in the About section open in your browser only when you click them.

Your history is stored in a local SQLite database, together with the saved file copies, in:

```
~/.local/share/io.github.gracepetermutiibwa.pookieclip/
```

Unpinned entries older than 30 days are removed, and the history keeps at most 1,000 unpinned entries.

## Requirements

- GNOME 45 to 50, on Wayland or X11

On Wayland, apps are not allowed to read the clipboard. PookieClip ships a small GNOME Shell extension that watches the clipboard and passes each change to the app over the session D-Bus. Support for KDE and other desktops is planned.

## Install

**Ubuntu 22.04 and newer, Debian 12 and newer**

Add the PookieClip apt repository once:

```
curl -fsSL https://gracepetermutiibwa.github.io/pookieclip/key.gpg | sudo gpg --dearmor -o /usr/share/keyrings/pookieclip.gpg
echo "deb [signed-by=/usr/share/keyrings/pookieclip.gpg] https://gracepetermutiibwa.github.io/pookieclip stable main" | sudo tee /etc/apt/sources.list.d/pookieclip.list
```

Then install it:

```
sudo apt update && sudo apt install pookieclip
```

New versions arrive with `sudo apt upgrade`.

**Fedora**

Download the `.rpm` from the [Releases](https://github.com/GracePeterMutiibwa/pookieclip/releases) page, then:

```
sudo dnf install ./PookieClip-*.x86_64.rpm
```

Then:

1. Open PookieClip once from your app grid. This turns on its GNOME extension and starts PookieClip at login from then on.
2. Log out and back in. GNOME loads new extensions only at login.

After that, PookieClip's icon is in the top bar and Ctrl+Alt+H opens the panel.

## Change the shortcut

```
gsettings --schemadir /usr/share/gnome-shell/extensions/pookieclip@gracepetermutiibwa.github.io/schemas set org.gnome.shell.extensions.pookieclip toggle-panel "['<Super><Shift>v']"
```

## Uninstall

**Ubuntu and Debian**

```
sudo apt remove pookieclip
```

To also remove the apt repository:

```
sudo rm /etc/apt/sources.list.d/pookieclip.list /usr/share/keyrings/pookieclip.gpg
```

**Fedora**

```
sudo dnf remove pookie-clip
```

To also delete your history, the saved file copies and the login entry:

```
rm -rf ~/.local/share/io.github.gracepetermutiibwa.pookieclip
rm -f ~/.config/autostart/io.github.gracepetermutiibwa.pookieclip.desktop
```

## Build from source

PookieClip is a [Tauri 2](https://tauri.app) app with a plain HTML, CSS and JavaScript interface and a Rust backend.

1. Install Rust from [rustup.rs](https://rustup.rs).
2. Install the system libraries (Ubuntu and Debian):

   ```
   sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev librsvg2-dev
   ```

3. Install the Tauri CLI:

   ```
   cargo install tauri-cli --version "^2"
   ```

4. Link the GNOME extension from the repo, turn it on, then log out and back in:

   ```
   ln -s "$PWD/gnome-extension/pookieclip@gracepetermutiibwa.github.io" ~/.local/share/gnome-shell/extensions/
   gnome-extensions enable pookieclip@gracepetermutiibwa.github.io
   ```

5. Run it in development mode from the repo root:

   ```
   cargo tauri dev
   ```

6. Build the `.deb` and `.rpm` packages:

   ```
   cargo tauri build
   ```

   The packages are written to `src-tauri/target/release/bundle/`.

## Releasing

Bump `version` in `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml`, commit, then push a matching tag:

```
git tag v0.1.2 && git push origin v0.1.2
```

The Release workflow builds the `.deb` and `.rpm`, attaches them to the GitHub release and publishes the signed apt repository on GitHub Pages.

## Project layout

```
.github/           Release workflow
gnome-extension/   GNOME Shell extension: clipboard watcher, top bar icon, shortcut
src-tauri/         Rust backend: history database, saved file copies, clipboard writer
ui/                The panel, a single HTML file
assets/            Source artwork for the app icon
```

## Author

Built by [Grace Peter Mutiibwa](https://gracepeter.clearnique.com).

## License

MIT, see [LICENSE](LICENSE).

Fonts: [IBM Plex Sans and IBM Plex Mono](https://github.com/IBM/plex), licensed under the SIL Open Font License 1.1.

Interface icons from [coolicons](https://github.com/krystonschwarze/coolicons) by Kryston Schwarze, licensed CC BY 4.0.
