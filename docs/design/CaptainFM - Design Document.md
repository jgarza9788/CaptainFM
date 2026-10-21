# CaptainFM — Design Document

A fast, keyboard-first file manager for Omarchy and Hyprland · Oct 20, 2026 · @Justin Garza

## Overview

CaptainFM is a file manager for Omarchy on Hyprland: a Qt 6 QML interface over a Rust core. It opens any folder in under 50 ms, fuzzy-searches a whole drive within one frame, recolors live with the active Omarchy theme, and is driven entirely from the keyboard. It replaces Nautilus as the `inode/directory` handler. The feature set is inspired by File Pilot on Windows; the name, UI and code are original.

**Goals**

- Instant: first paint of a 100k-entry folder in under 100 ms; search results within one frame.
- At home on Omarchy: no window chrome, live theme sync, system monospace font, sane at half-screen tile width.
- Keyboard-first: modal vim-style keys, a leader key with hints, every action in a command palette and rebindable.
- Smooth: QML animations for panes, overlays and the zoom slider, without dropping frames.
- Lean: one binary under 15 MB on top of system Qt 6, no daemon.

**Non-goals for 1.0**

- X11, desktop-environment integration beyond freedesktop standards, cloud sync, tagging databases, an embedded terminal.

**Users**

- Omarchy and Hyprland users who want a GUI file manager that matches the rest of their setup.
- Developers and homelab admins working in large repos, media libraries and network shares.

## Key decisions

**Name: CaptainFM (`captainfm`, alias `cfm`).** The `-fm` ending follows Linux file-manager tradition (pcmanfm, spacefm), so the name says what it is. Package, binary and Wayland app-id are `captainfm`. The AUR package also installs a `cfm` symlink, and anyone can set their own shell alias. Confirm `captainfm` and `cfm` are free on the AUR, crates.io and GitHub before publishing.

**Toolkit: Qt 6 Qt Quick (QML) for the interface, Rust for everything else, joined in-process by `cxx-qt`.** QML gives fluid animated UI, a theme that is just a set of bindable properties, and the ecosystem Hyprland users already script bars and widgets in. Rust keeps listing, indexing and file operations fast and safe. The Rust core exposes list models straight to QML, so a 100k-row folder never crosses a process boundary or gets serialized.

| Option | Verdict | Reason |
| --- | --- | --- |
| Qt 6 QML + Rust core via cxx-qt | **Chosen** | Fluid UI, property-based theming, in-process models for big folders, native DnD and clipboard |
| Quickshell front end + Rust backend over stdio | Rejected | Flea's approach. Quickshell is built for shells and widgets, and rows travel as JSON between processes |
| GTK4 + Rust, no libadwaita | Rejected | Mature and CSS-themeable, but static widgets and less animation headroom than QML |
| hyprtoolkit (C++) | Revisit later | Most Hyprland-native, still young (0.5.x) |
| iced (Rust) | Rejected | Weaker DnD, clipboard and accessibility today |

**Prior art: Flea.** [Flea](https://github.com/thisisgm/flea) is already a keyboard-first, MIT-licensed file manager for Omarchy, with a Quickshell QML front end over a Rust backend. It reads the live Omarchy palette and has a `flea --default` command that takes over `inode/directory` and Omarchy's file-manager shortcuts. CaptainFM borrows those integration ideas and differs in its build and focus:

|  | Flea | CaptainFM (planned) |
| --- | --- | --- |
| Front end | Quickshell (QML) | Standalone Qt Quick app (QML) |
| UI to core link | Rust child process, newline-delimited JSON | Rust in-process via cxx-qt list models |
| Runtime dependencies | Omarchy and Quickshell required | Qt 6 only; Omarchy integration optional |
| Focus | Fast keyboard-first browsing | Modal vim keys + leader, split panes, drive-wide index, batch rename, file-picker portal |

Before committing, try Flea for a week. If it covers most of this, contributing to it may be the faster path.

## Feature scope

The MVP is the nine features that make CaptainFM worth switching to on day one; the rest follow in 1.x.

| Feature | What it does | Release |
| --- | --- | --- |
| Instant listing | Async directory reads into a Rust list model, virtualized QML ListView, live inotify refresh | MVP |
| Tabs and panes | Numbered tabs, up to 4 panes, saved layouts | MVP |
| Modal keys + leader | vim motions, `space` leader with a hint popup, counts (`5j`) | MVP |
| Jump bar | Path autocomplete, recent folders, bookmarks, frecency ranking | MVP |
| Deep search | Whole-folder or whole-drive flattened view, fuzzy match, extension and date filters | MVP |
| Inspector | Side preview for text (syntax highlighted), images, PDFs, folders, archives | MVP |
| Command palette | Every action searchable; rebind hotkeys inline | MVP |
| Folder sizes | Recursive sizes computed in the background, sortable | MVP |
| Omarchy theme sync | Colors from the active theme, live reload on theme change | MVP |
| Batch rename | Template tokens, regex, case, live preview, undoable | 1.1 |
| File picker portal | `xdg-desktop-portal` FileChooser backend; floating picker for other apps | 1.1 |
| Remotes and devices | SMB, SFTP, MTP via GVfs; UDisks2 mount and eject | 1.1 |
| Media preview | Video and audio scrubbing in the inspector via Qt Multimedia | 1.1 |
| Tree and Miller columns | Alternate layouts | 1.2 |
| Undo/redo | Journaled copy, move, rename, trash | 1.2 |
| Plugins | QML panels and Lua actions | 2.0 |
