# CaptainFM

**A fast, keyboard-first file manager for Omarchy and Hyprland.** A Qt 6 QML interface over a Rust core,
joined in-process by [cxx-qt](https://github.com/KDAB/cxx-qt). It opens a 100k-entry folder in about
100 ms, searches a whole drive in about 20 ms, and recolors itself live with your Omarchy theme.

The feature set is inspired by [File Pilot](https://filepilot.tech/) on Windows; the name, UI and code are original.

![CaptainFM main view](docs/images/main.png)

## Highlights

- **Instant everything.** Async folder reads with live refresh, background folder sizes, a drive-wide
  index for fuzzy search (`Ctrl-F`), and a global search box at the top of the sidebar.
- **Two ways to drive it.** File Pilot style keys by default (Ctrl shortcuts, arrows, type to find), or modal
  Vim keys with a `Space` leader and a which-key popup. Switch with `Ctrl-Shift-K`.
- **One command palette for everything** (`Ctrl-P`). Words in order or `re:regex`, aliases, inline key
  rebinding, and when no command matches it becomes a go-to for folders.
- **Real previews.** Syntax-highlighted code, images (including HEIC, TIFF, RAW), video with transport
  controls, audio, PDFs, e-books, Office documents, Markdown, CSV tables, SQLite, certificates, fonts, archives.
- **Tabs, up to four panes, saved layouts**; list, tree, Miller, icons (thumbnails) and columns views;
  group by type, date or size.
- **Pins, jump bar, batch rename, undo/redo**, devices and remotes (SMB, SFTP, MTP), drag and drop.
- **Fits Omarchy:** live theme sync, your system font, frameless, sane at half-screen tile width, a
  file-picker portal and `FileManager1` so other apps can use it.
- **Extensible** with Lua actions and QML side panels.
- Lean: one ~10 MB binary on top of Qt 6, no daemon.

| | |
| --- | --- |
| ![Command palette](docs/images/palette.png) | ![Deep search](docs/images/search.png) |
| ![Batch rename](docs/images/batch-rename.png) | ![Icons view with thumbnails](docs/images/icons.png) |
| ![Video preview](docs/images/video.png) | ![Which-key popup](docs/images/leader.png) |
| ![Context menu](docs/images/context-menu.png) | ![Group by](docs/images/group-by.png) |

Every color comes from the active Omarchy theme:

![Three themes](docs/images/themes.png)

## Install

Needs Rust, Qt 6 (`qt6-declarative`, `qt6-multimedia`) and a C++ toolchain. Optional helpers unlock more previews
and features: `ffmpeg`, `imagemagick`, `poppler`, `libarchive` (`bsdtar`), `sqlite`, `openssl`, `wl-clipboard`,
`gvfs`, `udisks2`.

```sh
git clone https://github.com/jgarza9788/CaptainFM.git
cd CaptainFM
./build.sh          # release build; installs ~/.local/bin/captainfm (+ cfm alias), desktop entry and icon
```

CaptainFM does not change your default file manager or keybindings on its own. To make it the folder handler
(and undo that later):

```sh
captainfm --default        # inode/directory -> CaptainFM
captainfm --default off    # restore the previous handler
```

## Quick start

| Do this | Keys (File Pilot) | Keys (Vim) |
| --- | --- | --- |
| Move, open, go up | arrows, `Enter`, `Backspace` | `j` `k` `l` `h` |
| Run any command | `Ctrl-P` | `Space Space` or `:` |
| Go to a folder | `Ctrl-L` | `z` |
| Search this folder / the drive | `Ctrl-F` / `Ctrl-Shift-F` | `Space f f` / `Space f d` |
| Copy, cut, paste | `Ctrl-C` `Ctrl-X` `Ctrl-V` | `yy` `dd` `p` |
| Rename / batch rename | `F2` | `r` / `R` |
| Trash / undo | `Del` / `Ctrl-Z` | `dt` / `u` |
| Pin the current folder | `Ctrl-B` | `Space p f` |
| New tab, split pane | `Ctrl-T`, `Ctrl-\` | `Ctrl-T`, `Ctrl-w v` |

Everything else is in the **[how-to guide](docs/HOWTO.md)**: daily use step by step, every view, searching, pins,
batch rename, devices and remotes, themes, configuration, plugins, full key tables and troubleshooting.

## Command line

```
captainfm [--select <path>] [path...]   open folders as tabs; --select highlights an item
captainfm --tab <path...>               open folders as tabs in the already running window
captainfm --install                     install the desktop entry and icon for this user
captainfm --default [off]               become (or stop being) the folder handler
captainfm --install-portal [off]        use CaptainFM as the file picker (xdg-desktop-portal)
captainfm --install-dbus                let other apps open folders in CaptainFM (FileManager1)
```

## Configuration and plugins

Settings, key overrides, aliases, layouts and plugins live under `~/.config/captainfm/`; pins and recent folders
under `~/.local/share/captainfm/`. Plugins add Lua actions and QML side panels; two examples are in
[`examples/plugins/`](examples/plugins). Details: [configuration files](docs/HOWTO.md#configuration-files) and
[plugins](docs/HOWTO.md#plugins).

## Development

```sh
cargo test                      # ~120 unit tests: fuzzy matching, listing, index, file ops and undo, pins, D-Bus, plugins, previews
cargo clippy --release --all-targets
./scripts/screenshots.sh        # regenerate docs/images from a synthetic home folder (headless)
```

The UI can be driven headlessly for screenshots and smoke tests:

```sh
export QT_QPA_PLATFORM=offscreen
CFM_SIZE=1400x860 CFM_KEYS="<Down> <Down> @view.tree <Right>" CFM_SHOT=/tmp/shot.png captainfm /some/folder
```

`CFM_KEYS` is a space-separated list of key tokens (`j`, `<C-d>`, `<Space>`), `@command.id`, `=text` (types into
the focused field; `_` is a space), `!accept`, `!next`, `!mode:template`, `!clear` and `!play`. `CFM_SHOT` saves
a window image and quits, `CFM_DEBUG=1` prints timings, `CFM_KEYMAP=vim|fp` picks the keymap. Point
`XDG_CONFIG_HOME`, `XDG_DATA_HOME` and `XDG_CACHE_HOME` somewhere temporary to keep test runs away from your state.

Layout: `src/` Rust (`dir_model`, `listing`, `index`, `fuzzy`, `ops`, `preview`, `preview_extra`, `devices`,
`batch`, `plugins`, `dbus`, `theme`, `state`, `thumbnails`), `qml/` interface, `packaging/` desktop assets,
`scripts/` tooling. The original design document is in [`docs/design/`](docs/design).

## License

[MIT](LICENSE) © 2026 Justin Garza.
