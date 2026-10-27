# Making a release

A release is a chain of small scripts in `scripts/release/`. Each one does one part, is commented line by line, and
can be run alone or chained with `all.sh`. `~/Work/CaptainFM` is the Dev folder and `~/Work/CaptainFM-release` is a
second checkout of `master`: releases are built from `master` only. `X.Y.Z` is the version in `Cargo.toml`.

```sh
cd ~/Work/CaptainFM
scripts/release/all.sh            # parts 10 to 60: everything except publishing
# ...look at dist-release/, edit dist-release/notes.md, commit docs/perf/ ...
scripts/release/70-publish.sh     # public: shows everything, asks for a yes
```

## The parts

| Part | File | What it does | Time |
|---|---|---|---|
| 0 | `00-vars.sh` | Sets the variables (below). Sourced by every other part, never run. | |
| 1 | `10-tools.sh` | Checks the programs are installed and `gh` is logged in; downloads the AppImage tools to `~/Work/CaptainFM-tools` and records their checksums. Safe to repeat. | seconds |
| 2 | `20-merge.sh` | Makes the `master` checkout if needed, shows what is coming, asks, then merges the Dev branch with `--no-ff`. Pushes nothing. | seconds |
| 3 | `30-build.sh` | Builds the Arch package (unit tests run inside it), the tarball and the AppImage from that same binary, and `SHA256SUMS`, into `dist-release/vX.Y.Z/`. Needs a clean `master`. | ~12 min |
| 4 | `40-podman.sh` | Installs the package, and runs the AppImage, in a clean Arch container: prints the version and draws a headless screenshot. | minutes |
| 5 | `50-perf.sh` | Builds the `dist` binary and runs `scripts/perf.py`; saves a report in `docs/perf/`. | minutes |
| 6 | `60-notes.sh` | Drafts `dist-release/notes.md` from the commits since the last tag, the limits and the install lines. | seconds |
| 7 | `70-publish.sh` | Pushes `master`, tags `vX.Y.Z`, creates the GitHub release. Public. Refuses without a yes, and refuses if the tag exists. | seconds |
| | `all.sh` | Runs the parts in order and stops at the first failure. Stops before 70 unless given `--publish`. | |

## Variables

Set in `00-vars.sh` with defaults. Change one for a single run with an environment variable or a flag; `all.sh`
parses the flags once and every part it starts sees them.

| Variable | Flag | Default |
|---|---|---|
| `DEV` | | the repo the scripts are in |
| `REL` | | `$DEV/../CaptainFM-release` |
| `TOOLS` | `--tools` | `~/Work/CaptainFM-tools` |
| `DEV_BRANCH` | `--dev-branch` | the branch checked out in `$DEV` |
| `VERSION` | `--version` | read from `Cargo.toml` (the only place it is set; scheme `0.0.N`) |
| `CODENAME` | `--codename` | read from `qml/Settings.qml` (edited by hand each release: v0.0.1 *Ahab*, v0.0.2 *Blackbeard*) |
| `OUT_ROOT`, `OUT` | `--out` | `$DEV/dist-release`, `$OUT_ROOT/vX.Y.Z` |
| `SKIP_TESTS` | `--skip-tests` | `0`. Skips lint and tests in part 3, only for a rebuild of something already tested |
| `ASSUME_YES` | `--yes` | `0`. Answers the merge and publish questions; use with care |

```sh
scripts/release/30-build.sh --skip-tests          # one part, with a flag
DEV_BRANCH=Dev2 scripts/release/20-merge.sh       # one part, with a variable
scripts/release/all.sh --from 30 --to 40          # a range of parts
scripts/release/all.sh --skip merge --skip perf   # all but some (names: tools merge build podman perf notes publish)
```

## Before you start
On the Dev branch everything is committed (part 2 refuses otherwise), the version is set in `Cargo.toml`, and the
codename is set in `qml/Settings.qml`. Merging a Dev branch into `master` is the owner's call: part 2 asks first, and if
the merge reports conflicts it stops so you can sort them out.

## What stays by hand
- **Perf:** compare the report's "Change" column with the last one. A clear, repeated slowdown or more memory is a
  reason to look before publishing. Commit the report on the Dev branch.
- **Notes:** `notes.md` is a draft with an `EDIT:` marker; write "What is new" in plain words and delete the marker
  (part 7 refuses while it is there). Keep the limits from the Reference below.
- **Publish:** show the file list, `SHA256SUMS` and `notes.md`, and get a yes first. Never move or delete a tag that is
  already published without the owner saying so. To add files to an existing release:
  `gh release upload vX.Y.Z <files> --clobber`.

## After
Update the README install section to link the release, move what shipped from `todo.md` to `docs/done_vXXX.md`, and
(later) push `PKGBUILD` and `.SRCINFO` to the AUR once the tag gives real source checksums. When done with the
`master` checkout: `cd ~/Work/CaptainFM && git worktree remove ../CaptainFM-release` (part 2 makes it again).

---

## Reference

**What is supported:** Arch Linux with Omarchy and Hyprland (Wayland, Qt 6.12). Shipped: the Arch package, a tarball
(`install.sh` into `/usr/local` or `PREFIX=`; links against this machine's Qt, so it only suits a similar Qt), and the
AppImage. Not shipped, on purpose: `.deb`, `.rpm` and Flatpak (each needs a build against that distro's own Qt).
X11, other desktops and other CPUs are untested.

**AppImage limits (put them in the notes):**
- Built here it needs glibc 2.44 or newer, so it starts on rolling distros only (it does not start on Debian 13).
  Older distros need a build inside an older distro's container, not done yet.
- Host libraries are not bundled: `libGL`, `libEGL`, `libOpenGL`, `fontconfig`, `freetype`, `harfbuzz`, `libxkbcommon`,
  `libwayland-client`, `libdrm`, `libgbm` and at least one font. Tools such as gvfs, udisks and rclone are found on `PATH`.
- `scripts/appimage.sh` adds the Wayland and offscreen platform plugins and the `wayland-*` plugin folders, and writes its
  own `AppRun` (linuxdeploy's starts the binary through a symlink, so Qt finds no platform plugin). It can also be run
  alone: `CFM_BIN=<binary> ./scripts/appimage.sh <outdir>`, or without `CFM_BIN` on `target/dist/captainfm`.

**In every package:** the `captainfm` binary (QML and the five Lua preview plugins are compiled in) plus the `cfm`
symlink, `captainfm.desktop` (`Exec=captainfm %U`, mime `inode/directory`), the two icons in `packaging/`, and `LICENSE`.
Runtime dependencies: `qt6-base`, `qt6-declarative`, `qt6-multimedia`, `fuse3`, any font (`ttf-font` on Arch; with none,
every letter is a box). Optional: `gvfs`, `udisks2`, `ffmpeg`, `imagemagick`, `poppler`, `libarchive`, `sqlite`,
`wl-clipboard`, `libsecret`, and `rclone` (a separate package: list it as optional, never bundle it).

**Rules:** never run `captainfm --default`, `--install` or the portal setup from a package script (the default folder
handler is the user's choice, and `--install` writes into `$HOME`). The PKGBUILD needs `options=('!lto')` (makepkg's
`-flto` objects cannot be read by rust-lld). `sha256sums` in the PKGBUILD is `SKIP` until a tag exists.
