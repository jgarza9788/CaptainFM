# CaptainFM how-to guide

A task-by-task guide to using CaptainFM day to day. Keys are given for both keymaps: **FP** (File Pilot style,
the default) and **Vim** (modal). Press `Ctrl-Shift-K` any time to switch; your choice is remembered.

Contents: [The window](#the-window) · [Moving around](#moving-around) · [Selecting and file operations](#selecting-and-file-operations) ·
[Finding things](#finding-things) · [Pins](#pins) · [Tabs, panes and layouts](#tabs-panes-and-layouts) · [Views](#views) ·
[The preview pane](#the-preview-pane) · [Batch rename](#batch-rename) · [The command palette](#the-command-palette) ·
[Customizing keys](#customizing-keys) · [Devices and remotes](#devices-and-remotes) · [Themes and look](#themes-and-look) · [Settings](#settings) ·
[Other programs](#other-programs) · [Plugins](#plugins) · [Configuration files](#configuration-files) · [Key tables](#key-tables) ·
[Troubleshooting](#troubleshooting)

---

## The window

![Main view](images/main.png)

1. **Header** — the sidebar button (top left), a mode badge (`FILES`/`NORMAL`, `SELECT`/`VISUAL`, `SEARCH`), clickable
   breadcrumbs, and the selection and item counts.
2. **Sidebar** — a global search box, then **PINNED**, **PLACES**, **DRIVES** (with a usage bar that turns yellow at 80% and red at 90%; hover for the free space) and **REMOTE**. Click a
   section title to fold it. The button at the top left cycles it: *open → icons only → hidden*.
3. **Tabs** — numbered; click, or use `Ctrl-1…9`.
4. **File list** — name, size and modified. Click a column title to sort. Folder sizes fill in as they are computed.
5. **Preview pane** — what is under the cursor (toggle with `Ctrl-I` / `i`).
6. **Status line** — key hints for the current mode, index state and the zoom slider.

On narrow tiles the preview hides below 760 px and the sidebar collapses to icons below 640 px.

## Moving around

| Task | FP | Vim |
| --- | --- | --- |
| Up / down | `↑` `↓` | `k` `j` (`5j` moves five) |
| Open a folder or file | `Enter` or `→` | `l` or `Enter` |
| Go to the parent | `←`, `Backspace` or `Alt-↑` | `h`, `-` or `Backspace` |
| History back / forward | `Alt-←` / `Alt-→` | `H` / `L` |
| First / last item | `Home` / `End` | `gg` / `G` (`12G` for row 12) |
| Half page | | `Ctrl-d` / `Ctrl-u` |
| Jump to an item by name | just type its first letters | |
| Go to a folder | `Ctrl-L` | `z` |
| Reload | `F5` | `Space v r` |

**Go to (`Ctrl-L`).** Type a path (`~/Pro` completes), or words (`home lab`), or `re:regex`. Results are your pins,
frequently and recently used folders (ranked by frecency), and common places like Downloads. `Tab` completes,
`Enter` goes. Every folder you visit is remembered.

## Selecting and file operations

| Task | FP | Vim |
| --- | --- | --- |
| Mark / unmark and move on | `Space` | `m` |
| Extend a selection | `Shift-↑` `Shift-↓` | `v`, then `j` `k` |
| Select all | `Ctrl-A` | `Ctrl-a` |
| Copy / cut / paste | `Ctrl-C` `Ctrl-X` `Ctrl-V` | `yy` `dd` `p` |
| Move to trash | `Del` | `dt` |
| Delete permanently (asks first) | `Shift-Del` | `dD` |
| Rename | `F2` | `r` |
| New file / new folder | `Ctrl-N` / `Ctrl-Shift-N` | `a` / `A` |
| Undo / redo | `Ctrl-Z` / `Ctrl-Y` | `u` / `Ctrl-r` |
| Copy path / name | `Ctrl-Shift-C` | `yp` / `yn` |
| Copy / move to the other pane | `F7` / `F8` | `F5` / `F6` |
| Open with the default app | `Ctrl-O` | `o` |
| Terminal here | ``Ctrl-` `` | `!` |

- Copy and cut use the system clipboard in the same format as Nautilus, so pasting works in other file managers.
- Name clashes never overwrite: pasting `a.txt` next to `a.txt` creates `a (copy).txt`.
- Trash follows the freedesktop spec. **Undo works for copy, move, rename, new items and trash** (permanent
  delete cannot be undone). Undo is per session.
- **Drag and drop** shows what you are carrying (the first name, with a `+N` badge for more) and what releasing will
  do: the target is outlined and says "Copy to docs", "Move to docs", "Link in docs", "Download to docs" or
  "Already here". Drop on the **trash** place in the sidebar to trash the files. Drops onto any sidebar place or
  folder work too. Under Wayland the compositor decides copy or move from your modifier keys, so a link needs
  Settings > *Dropping files* > *always link (symlink)*. It works between CaptainFM and other apps (tested against Nautilus, Firefox and Chromium, both directions). Drop onto a
  folder row or a pane; like Windows: `Ctrl` copies, `Shift` moves, `Alt` (or `Ctrl+Shift`) links, and with no key a drop moves within one drive and copies between drives, unless
  you fix it in Settings. Dragged files are offered as a `text/uri-list` (file managers, upload fields) and as plain
  paths (editors, terminals). Dropping files from other apps works the same. **Browsers:** files dragged into Firefox
  and Chromium arrive as real files, and an image or link dragged out of a browser is downloaded into the folder you
  drop it on (needs `curl`).
- Right-click or press `Menu` (`Space c` in Vim) for the **context menu**: a searchable list with "Open with" for
  every app that handles the file type, standard actions, and your pinned actions (`Ctrl-D` pins one).

## Finding things

**Filter the current folder** — `F3` (FP) or `/` (Vim). Type words; the list narrows live. Filters you can add:

| Token | Meaning |
| --- | --- |
| `ext:rs` or `ext:png,jpg,tar.gz` | extension |
| `type:d` / `type:f` | folders / files |
| `size>10M`, `size<1k` | size |
| `newer:7d`, `older:2w` | modified time (`s m h d w y`) |
| `re:^img_\d+` | regular expression instead of words |

Words match in order as substrings (`read me` finds `read me first.txt`); anything else matches fuzzily.

**Deep search** — `Ctrl-F` searches everything under this folder, `Ctrl-Shift-F` the whole drive; results appear in the pane
as a flat list with the match highlighted and the folder underneath.

![Deep search](images/search.png)

The first search builds an index in the background (about 2 s for 340k files); after that results arrive in
roughly one frame. The chips show the scope, the filters you typed, and "N matches · M paths · T ms". In results:
`Enter` opens, `Ctrl-Enter` opens its folder in a new tab, `Alt-Enter` reveals it in its folder, `Esc` goes back.

**The sidebar search box** does the same drive-wide search from anywhere. Type there, `Enter` or `↓` to move into the
results, `Esc` to clear.

## Pins

Pin the things you use all the time and they appear under **PINNED** in the sidebar, live.

- `Ctrl-B` (Vim `Space p f`) pins or unpins the current folder.
- `Ctrl-D` (Vim `Space p i`) pins or unpins the item under the cursor, a file or a folder. Clicking a pinned file opens it.
- Right-click a pinned entry to unpin it. Pins made in another CaptainFM window show up immediately.
- Pins also lead the **Go to** list and the palette's folder results.

## Tabs, panes and layouts

| Task | FP | Vim |
| --- | --- | --- |
| New tab / close tab | `Ctrl-T` / `Ctrl-W` | `Ctrl-t` / `q` |
| Next / previous tab | `Ctrl-Tab` / `Ctrl-Shift-Tab` | `gt` / `gT` |
| Go to tab *n* | `Ctrl-1…9` | `Alt-1…9` |
| Search open tabs | `Alt-T` | `Space t t` |
| Open a folder in a new tab | `Ctrl-Enter` | `Ctrl-Enter` |
| Split beside / below | `Ctrl-\` / `Ctrl-Shift-\` | `Ctrl-w v` / `Ctrl-w s` |
| Next / previous pane | `Tab` / `Shift-Tab` | `Tab` / `Shift-Tab` |
| Close pane / keep only this one | `Ctrl-Shift-W` | `Ctrl-w c` / `Ctrl-w o` |
| Swap panes | `Ctrl-Shift-X` | `Space w x` |
| Cycle pane layout | | `Space w l` |

- Up to four panes per tab. Drag a pane's `⠿` grip onto another pane to swap them; drag a tab to reorder.
- **Save a layout** with `Space l s` (name it) and bring it back with `Space l l`. Layouts hold your tabs, panes and views.
- The last session (tabs, panes, views, sort, hidden files, zoom, sidebar state) is restored on start.
- Open folders from the shell into the running window: `captainfm --tab ~/Projects ~/Music`.

## Views

Switch the view of the active pane (all remembered per pane and saved with your layout):

| View | FP | Vim | Notes |
| --- | --- | --- | --- |
| List | `Alt-1` | `Space v l` | the default |
| Tree | `Alt-2` | `Space v t` | folders expand in place (`→` expands, `←` collapses) |
| Miller columns | `Alt-3` | `Space v m` | parent, current and child side by side |
| Icons | `Alt-4` | `Space v g` | thumbnails, from the freedesktop thumbnail cache or the image itself |
| Columns | `Alt-6` | `Space v c` | a compact multi-column name list |
| Group by | `Alt-5` | `Space v b` | cycles none → type → date → size, with section headers |

![Icons view](images/icons.png)

Also: `Ctrl-H` shows hidden files, `Ctrl-=` / `Ctrl--` / `Ctrl-0` zoom (or `Ctrl`+scroll, or the status-bar slider),
`Space v d` cycles row spacing, `Space v a` turns animations off.

## The preview pane

Press `Ctrl-I` (`i`) to toggle it. It previews:

| Kind | What you see |
| --- | --- |
| Source and text | syntax highlighting and line numbers |
| Images | the image, size and EXIF; HEIC, AVIF, TIFF, PSD, RAW and more are converted and cached |
| Video | a poster frame, transport buttons, seek bar, hover-to-scrub |
| Audio | cover art or a waveform, plus tags |
| PDF, e-book, comic | the first page or the cover |
| Word, Excel, PowerPoint, OpenDocument | the extracted text |
| Markdown | rendered |
| CSV / TSV | an aligned table; JSON is pretty-printed |
| SQLite, certificates, fonts | tables and schema; subject and dates; a specimen |
| Archives and folders | the contents |
| Anything else | a hex dump |

![Video preview](images/video.png)

**Video and audio controls.** Buttons for back 10 s, play/pause, forward 10 s, loop and mute, a draggable seek bar, and
the time. Click the picture to play or pause; hover a paused video to scrub. Hotkeys: `Ctrl-M` play/pause,
`Ctrl-←` / `Ctrl-→` seek, `Ctrl-Shift-M` mute (Vim: `Space m p`, `m h`, `m l`, `m m`, `m r`). The note under the player
always shows your current bindings. The header has quick buttons to open the item, copy its path or pin it.

Optional tools improve previews: `ffmpeg` (video, audio), `imagemagick` (converted images, EXIF), `poppler` (PDFs),
`libarchive` (archives, covers, Office text), `sqlite3`, `openssl`. A missing tool only disables its own preview.

## Batch rename

Select several files and press `F2` (Vim `R`).

![Batch rename](images/batch-rename.png)

Pick a **mode** (or `Alt-1…4`):

- **replace** — find and replace text in the names.
- **template** — build names from tokens: `{name}` stem, `{ext}` extension with dot, `{old}` full old name, `{n}` or `{n:3}`
  counter (zero padded), `{created:%Y-%m-%d}` / `{modified:%H%M}` with a [strftime](https://man7.org/linux/man-pages/man3/strftime.3.html)
  format, `{date}`, `{parent}`, `{id}` short random id, `{uuid}`.
- **regex** — a pattern and a replacement using `$1`, `$2`…
- **case** — keep, lower, upper, title, snake or kebab case (the extension is left alone).

Set the counter's **start** and **step**, and whether files are numbered in **name** or **date** order. The preview
shows every rename; counters are highlighted, and a name that already exists is flagged **skipped**. `Enter` applies
the rest as one operation, and `Ctrl-Z` undoes the whole batch.

## The command palette

`Ctrl-P` (Vim `Space Space` or `:`) lists every action with its category and current key.

![Command palette](images/palette.png)

- Type words in order (`split pane`, `new tab`) or `re:regex`. Matches are highlighted.
- **No command matches?** The palette switches to folders: your pins, recent and matching folders. `Enter` goes there.
  Path-like input (`~/…`, `/…`) goes straight to folders.
- `Enter` runs; `Tab` (or `Ctrl-K`) **binds a key** to the highlighted command: press the keys, then `Enter`.
  `Ctrl-X` removes its bindings. `Ctrl-A` adds **aliases** (typing an alias puts that command first). `Ctrl-D` pins it
  to the context menu. Unbound commands show a dashed `+ bind`.

## Customizing keys

Bindings you make in the palette are saved per keymap to `~/.config/captainfm/keys.json`:

```json
{
  "fp":  { "<C-d>": "pin.item", "<A-r>": "file.rename" },
  "vim": { "x h": "view.hidden", "<Space> q": "" }
}
```

Key syntax: plain keys (`j`, `J`), modifiers `<C-x>` (Ctrl), `<A-x>` (Alt), `<S-x>`; named keys `<CR> <Esc> <Tab> <Space>
<BS> <Del> <Up> <Down> <Left> <Right> <Home> <End> <PageUp> <PageDown> <F1>…`; numpad `<KP-5>`; sequences are
space-separated (`g g`, `<Space> f f`, `<C-w> v`). An empty value unbinds. Command ids are listed in `qml/Commands.qml`.

In Vim keys, pressing `Space`, `g`, `y`, `d`, `z` or `Ctrl-w` and pausing shows a popup of what can follow:

![Which-key](images/leader.png)

## Devices and remotes

- **Drives** under the sidebar's DRIVES show usage (yellow at 80%, red at 90%).
- `Space g d` (palette: "Devices and remotes") lists disks and phones; `Enter` mounts and opens. `Space g e` unmounts or
  ejects. Needs `udisks2`.
- `Space g c` ("Connect to server") takes `smb://host/share`, `sftp://user@host/`, `ftp://…` or `dav://…`; if a password is
  needed you are prompted. Mounted servers show under REMOTE. Needs `gvfs` (`gvfs-smb`, `gvfs-mtp`…).

## Themes and look

CaptainFM reads the active Omarchy theme (`~/.local/state/omarchy/current/theme/colors.toml`) and your system font,
and **updates live** when you run `omarchy theme set …`. Without Omarchy it falls back to Catppuccin Mocha.

![Three themes](images/themes.png)

Look-and-feel switches: zoom, row spacing (compact/normal/roomy), animations on/off, sidebar state, hidden files.

## Settings

Press `Ctrl-,` (`Space ,` in Vim, or *Settings…* in the palette). `↑` `↓` pick a row, `←` `→` or `Enter` change it,
`Enter` on a text row edits it, `Esc` closes. Changes apply at once.

| Section | Setting |
| --- | --- |
| General | on start (restore the last session, home, or a start folder) · key style · default view · single or double click to open · terminal command |
| Appearance | font family · row spacing · animations · file sizes (1024 or 1000) · date format |
| Browsing | hidden files · preview pane · sidebar · sort by and order · group by |
| Files | what a drop does (like Windows, always copy, always move, always link) · ask before trashing |
| System | use CaptainFM as the file picker for other apps |

Most of these live in `~/.config/captainfm/settings.json` (only values that differ from the defaults are written);
the view, sort and key settings are saved with the session.

## Other programs

- **Make it your default folder handler:** `captainfm --default` (undo with `--default off`). It does not touch your
  keybindings.
- **File picker for other apps:** `captainfm --install-portal` (or *File picker for other apps* in Settings) registers
  CaptainFM with xdg-desktop-portal, sets it as the `FileChooser` in your portals.conf and restarts the portal.
  Open, Save and choose-folder dialogs in browsers and other apps then use a CaptainFM window with an Open/Save
  bar, filters and multi-select. In that window `Enter`, double-click or middle-click on a file chooses it (it is
  never opened in another app), and the dialog closes as soon as you choose; with several files marked, `Enter`
  returns them all. Undo with `captainfm --install-portal off`. Float it with a Hyprland window rule
  for class `captainfm` and a title that starts with the dialog's title.
- **"Show in folder"** from browsers and chat apps: while CaptainFM is the folder handler it serves
  `org.freedesktop.FileManager1`. `captainfm --install-dbus` adds D-Bus activation so it also starts on demand.
- **Open folders in the running window:** `captainfm --tab <path…>`; open a file's folder with `--select <path>`.

## Plugins

A plugin is a folder in `~/.config/captainfm/plugins/<name>/` with a `plugin.json`:

```json
{
  "name": "Git",
  "panels":  [{ "id": "status", "title": "Git status", "qml": "Status.qml", "side": "right" }],
  "actions": [{ "id": "status", "title": "Refresh status" },
              { "id": "copy_branch", "title": "Copy branch name", "keys": ["<Space> x b"] }]
}
```

Actions are Lua functions in `main.lua` (or the file named by `"file"`), run in a sandbox with only the `string`, `table`,
`math` and `utf8` libraries. They receive `ctx = { path, cursor, selection, home }` and talk to the app through `cfm`:

| Function | Does |
| --- | --- |
| `cfm.notify(text, kind)` | toast (`info`, `warn`, `error`) |
| `cfm.navigate(path)`, `cfm.select(path)`, `cfm.open(path)` | navigate, move the cursor, open externally |
| `cfm.copy_text(text)`, `cfm.command(id)` | clipboard, run any command |
| `cfm.run(cmd, {args}, cwd)` | run a program (30 s limit) → `{ok, code, stdout, stderr}` |
| `cfm.read(path, max)`, `cfm.exists`, `cfm.is_dir`, `cfm.list(dir)` | read-only file access |
| `cfm.env(name)`, `cfm.home()`, `cfm.log(text)` | environment and logging |

An action that **returns a value** hands it to a panel. A panel is a QML `Item` with a `property var api`
(`api.path`, `api.cursor`, `api.selection()`, `api.theme`, `api.call(plugin, action, callback)`, `api.navigate`, `api.toast`).
Actions and panels appear in the command palette (`Plugin: …`, `toggle panel`) and can be bound to keys. Scripts are
stopped after 10 seconds. Examples: [`examples/plugins/`](../examples/plugins).

## Configuration files

| File | Holds |
| --- | --- |
| `~/.config/captainfm/settings.json` | your settings (see [Settings](#settings)) |
| `~/.config/captainfm/session.json` | tabs, panes, views, sort, group-by, zoom, keymap, sidebar state (written automatically) |
| `~/.config/captainfm/keys.json` | your key overrides, per keymap |
| `~/.config/captainfm/aliases.json` | command aliases: `{ "del": "file.trash" }` |
| `~/.config/captainfm/layouts.json` | named layouts |
| `~/.config/captainfm/plugins/` | plugins |
| `~/.local/share/captainfm/state.json` | pins and recent folders (frecency) |
| `~/.local/share/Trash/` | the trash (shared with every freedesktop app) |
| `~/.cache/captainfm/` | cached preview images (safe to delete) |

All of them honor `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and `XDG_CACHE_HOME`.

## Key tables

### File Pilot keys

| Keys | Action |
| --- | --- |
| arrows, `Home` `End` `PgUp` `PgDn` | move · `←`/`Backspace`/`Alt-↑` parent · `→`/`Enter` open · `Alt-←/→` history |
| `Shift`+arrows · `Space` · `Ctrl-A` | extend selection · toggle mark · select all |
| `Ctrl-C` `Ctrl-X` `Ctrl-V` | copy, cut, paste |
| `Del` · `Shift-Del` | trash · delete permanently |
| `F2` · `Ctrl-N` · `Ctrl-Shift-N` | rename (batch if several) · new file · new folder |
| `Ctrl-Z` · `Ctrl-Y` | undo · redo |
| `Ctrl-L`, `Ctrl-G`, `Alt-D` | go to |
| `Ctrl-F` · `Ctrl-Shift-F` · `F3` | search folder · search drive · filter |
| `Ctrl-T` `Ctrl-W` `Ctrl-Tab` `Ctrl-1…9` `Alt-T` | tabs |
| `Ctrl-\` `Ctrl-Shift-\` `Tab` `Ctrl-Shift-W` `Ctrl-Shift-X` | panes |
| `F7` `F8` · `Ctrl-Enter` · `Alt-Enter` | copy / move to other pane · open in new tab · reveal |
| `Ctrl-P` `Ctrl-Shift-P` `Ctrl-K` | command palette |
| `Ctrl-H` `Ctrl-I` `Ctrl-=` `Ctrl--` `Ctrl-0` | hidden · preview · zoom |
| `Ctrl-B` `Ctrl-D` | pin folder · pin item |
| `Ctrl-Shift-C` · ``Ctrl-` `` · `Ctrl-O` | copy path · terminal · open externally |
| `Alt-1…6` · `F5` · `Ctrl-Shift-B` · `Ctrl-Shift-K` · `Ctrl-Q` | views · reload · sidebar · keymap · quit |
| `Ctrl-M` `Ctrl-←` `Ctrl-→` `Ctrl-Shift-M` | play/pause · seek · mute |

### Vim keys

| Keys | Action |
| --- | --- |
| `j` `k` `h` `l` `gg` `G` `Ctrl-d` `Ctrl-u` | move (counts work: `5j`, `12G`) |
| `Enter` `o` `H` `L` `-` | open · open externally · history · parent |
| `z` / `Ctrl-l` | go to |
| `/` · `Ctrl-f` · `Space f f` · `Space f d` | filter · deep search |
| `m` `v` `Ctrl-a` | mark · visual · select all |
| `yy` `dd` `p` · `dt` `dD` · `u` `Ctrl-r` | copy cut paste · trash delete · undo redo |
| `r` `a` `A` `R` · `yp` `yn` · `!` | rename new-file new-folder batch-rename · copy path/name · terminal |
| `.` `i` | hidden · preview |
| `Ctrl-t` `gt` `gT` `q` `Alt-1…9` `Space t t` | tabs |
| `Tab` `Ctrl-w v/s/c/o` `Space w x` `Space w l` | panes |
| `F5` `F6` | copy / move to other pane |
| `Space Space` `:` `Ctrl-p` | command palette |
| `Space p f` `Space p i` | pin folder · pin item |
| `Space v …` | `t` tree, `m` Miller, `g` icons, `c` columns, `l` list, `b` group, `h` hidden, `i` preview, `s` sidebar, `a` animations, `d` spacing, `+`/`-`/`z` zoom |
| `Space l s` `Space l l` | save / load layout |
| `Space g d` `g e` `g c` | devices · eject · connect to server |
| `Space m p/h/l/m/r` | media play, back, forward, mute, loop |
| `Space k` · `Ctrl-q` `ZZ` | switch keymap · quit |

## Troubleshooting

- **Icons look like empty boxes.** Use a [Nerd Font](https://www.nerdfonts.com/) as your Omarchy monospace font; without one
  CaptainFM falls back to plain characters.
- **A preview is blank or says a tool is needed.** Install the helper named in [The preview pane](#the-preview-pane).
- **Video will not play.** It uses Qt Multimedia with FFmpeg; install `qt6-multimedia-ffmpeg`.
- **Remotes or phones do not appear.** Install and start `gvfs` and the matching backend (`gvfs-smb`, `gvfs-mtp`), and `udisks2` for disks.
- **Search says 0 paths.** The drive index builds on first use; give it a moment for large drives. System caches,
  `.git` and `node_modules` are skipped on purpose.
- **Keys do nothing in the palette or a prompt.** Those overlays take over the keyboard; `Esc` closes them.
- **Reset everything.** Close CaptainFM and delete `~/.config/captainfm/` (pins and recent folders are in `~/.local/share/captainfm/`).
- **Report a problem.** Run it from a terminal (`captainfm`) and include the output; `CFM_DEBUG=1` adds timings.
