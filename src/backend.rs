//! `Backend`: the QML-facing singleton for state, file operations and previews.
//!
//! Structured data crosses the boundary as JSON strings; long-running work runs on threads and
//! reports back through signals.

use std::pin::Pin;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;

use crate::transfer::{self, Ctl, Policy};
use crate::{devices, listing, ops, plugins, preview, remotes, state};

unsafe extern "C" {
    fn cfm_held_modifiers() -> i32;
}

#[cxx_qt::bridge]
mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        type Backend = super::BackendRust;

        #[qinvokable]
        fn home(&self) -> QString;
        /// The version from Cargo.toml, the one number every place shows.
        #[qinvokable]
        fn version(&self) -> QString;
        #[qinvokable]
        fn env(&self, name: &QString) -> QString;
        /// Milliseconds since the program started, for the `CFM_DEBUG` startup timings.
        #[qinvokable]
        #[rust_name = "since_start_ms"]
        fn sinceStartMs(&self) -> i32;
        #[qinvokable]
        #[rust_name = "read_config"]
        fn readConfig(&self, name: &QString) -> QString;
        #[qinvokable]
        #[rust_name = "write_config"]
        fn writeConfig(&self, name: &QString, json: &QString) -> bool;

        #[qinvokable]
        fn visit(&self, path: &QString);
        #[qinvokable]
        fn jump(&self, query: &QString) -> QString;
        #[qinvokable]
        fn bookmarks(&self) -> QString;
        /// Pin or unpin `path`; returns whether it is pinned afterwards. Emits `bookmarksChanged`.
        #[qinvokable]
        #[rust_name = "toggle_bookmark"]
        fn toggleBookmark(self: Pin<&mut Self>, path: &QString) -> bool;

        /// The set of bookmarks changed (here or in another CaptainFM window).
        #[qsignal]
        #[cxx_name = "bookmarksChanged"]
        fn bookmarks_changed(self: Pin<&mut Self>);
        #[qinvokable]
        #[rust_name = "mount_root"]
        fn mountRoot(&self, path: &QString) -> QString;
        #[qinvokable]
        #[rust_name = "search_root"]
        fn searchRoot(&self, path: &QString) -> QString;

        /// Copy or move `paths` (JSON array) into `dest`. Names that `dest` already has are asked about
        /// first (`pasteConflicts`); with none, it starts at once. Reports through `opDone`.
        #[qinvokable]
        fn paste(self: Pin<&mut Self>, paths: &QString, dest: &QString, cut: bool);
        /// Copy or move, answering name clashes with `policy`: "replace", "skip" or "both".
        #[qinvokable]
        #[rust_name = "paste_with"]
        fn pasteWith(
            self: Pin<&mut Self>,
            paths: &QString,
            dest: &QString,
            cut: bool,
            policy: &QString,
        );
        /// Every copy and move, as a JSON array with progress, speed and time left.
        #[qinvokable]
        fn transfers(&self) -> QString;
        #[qinvokable]
        #[rust_name = "transfer_cancel"]
        fn transferCancel(&self, id: i32);
        #[qinvokable]
        #[rust_name = "transfer_pause"]
        fn transferPause(&self, id: i32, paused: bool);
        /// Forget the transfers that are over.
        #[qinvokable]
        #[rust_name = "transfer_clear"]
        fn transferClear(&self);
        /// Make symbolic links to `paths` (JSON array) in `dest`. Reports through `opDone`.
        #[qinvokable]
        fn link(self: Pin<&mut Self>, paths: &QString, dest: &QString);
        /// Extract archives (JSON array) next to themselves, into a folder named after each when `into_folder`. Reports through `opDone`.
        #[qinvokable]
        fn extract(self: Pin<&mut Self>, paths: &QString, into_folder: bool);
        #[qinvokable]
        fn trash(self: Pin<&mut Self>, paths: &QString);
        /// Permanently delete `paths`. Not undoable.
        #[qinvokable]
        fn remove(self: Pin<&mut Self>, paths: &QString);
        /// How many items are in the home trash.
        #[qinvokable]
        #[rust_name = "trash_count"]
        fn trashCount(&self) -> i32;
        /// Delete everything in the home trash for good; reports through `opDone`.
        #[qinvokable]
        #[rust_name = "empty_trash"]
        fn emptyTrash(self: Pin<&mut Self>);
        #[qinvokable]
        #[rust_name = "rename_to"]
        fn renameTo(self: Pin<&mut Self>, path: &QString, new_name: &QString);
        #[qinvokable]
        fn create(self: Pin<&mut Self>, dir: &QString, name: &QString, is_dir: bool);
        #[qinvokable]
        fn undo(self: Pin<&mut Self>);
        #[qinvokable]
        fn redo(self: Pin<&mut Self>);
        /// `pairs` is a JSON array of `[from, to]` full paths.
        #[qinvokable]
        #[rust_name = "rename_many"]
        fn renameMany(self: Pin<&mut Self>, pairs: &QString);

        #[qinvokable]
        #[rust_name = "is_dir"]
        fn isDir(&self, path: &QString) -> bool;
        #[qinvokable]
        fn exists(&self, path: &QString) -> bool;
        /// Expand a leading `~`.
        #[qinvokable]
        #[rust_name = "expand_path"]
        fn expandPath(&self, path: &QString) -> QString;

        /// Preview a batch rename: `opts` is JSON (see `batch::Opts`); returns `{"rows": [...]}`
        /// or `{"error": "..."}`.
        #[qinvokable]
        #[rust_name = "rename_preview"]
        fn renamePreview(&self, paths: &QString, opts: &QString) -> QString;

        /// Indices of `items` (JSON array of strings) matching `query`, best first.
        #[qinvokable]
        fn rank(&self, items: &QString, query: &QString) -> QString;
        /// Like `rank` but without the scattered-letters fallback (a miss really is a miss).
        #[qinvokable]
        #[rust_name = "rank_strict"]
        fn rankStrict(&self, items: &QString, query: &QString) -> QString;
        /// Qt::KeyboardModifier flags held right now (Shift 0x2000000, Ctrl 0x4000000, Alt 0x8000000).
        #[qinvokable]
        #[rust_name = "held_modifiers"]
        fn heldModifiers(&self) -> i32;
        /// Palette ranking: literal matches, else guesses (word forms, synonyms, typos), with the commands
        /// chosen before for this query first. `items` and `ids` are parallel JSON arrays.
        #[qinvokable]
        #[rust_name = "rank_commands"]
        fn rankCommands(
            &self,
            items: &QString,
            ids: &QString,
            query: &QString,
            learn: bool,
        ) -> QString;
        /// Forget what the palette learned from your picks.
        #[qinvokable]
        #[rust_name = "forget_palette_history"]
        fn forgetPaletteHistory(&self);
        /// Note that a feature (a command id) was used today. Kept on this computer; see `state::FeatureUse`.
        #[qinvokable]
        #[rust_name = "mark_feature"]
        fn markFeature(&self, id: &QString);
        /// How often each feature was used in the last four weeks, as JSON.
        #[qinvokable]
        #[rust_name = "feature_report"]
        fn featureReport(&self) -> QString;
        #[qinvokable]
        #[rust_name = "forget_feature_use"]
        fn forgetFeatureUse(&self);
        /// Remember that `id` was chosen for what was typed in the palette.
        #[qinvokable]
        #[rust_name = "record_choice"]
        fn recordChoice(&self, query: &QString, id: &QString);
        #[qinvokable]
        #[rust_name = "copy_text"]
        fn copyText(&self, text: &QString);

        #[qinvokable]
        #[rust_name = "open_path"]
        fn openPath(&self, path: &QString);
        #[qinvokable]
        #[rust_name = "open_terminal"]
        fn openTerminal(&self, dir: &QString);

        /// What a drag out of CaptainFM carries, as JSON `{"uriList", "plain"}`, for `paths` (JSON array).
        #[qinvokable]
        #[rust_name = "drag_payload"]
        fn dragPayload(&self, paths: &QString) -> QString;
        /// Local paths (JSON array) in what was dropped: `urls` is a JSON array of URL strings, `text` the plain-text payload.
        #[qinvokable]
        #[rust_name = "drop_paths"]
        fn dropPaths(&self, urls: &QString, text: &QString) -> QString;
        /// Whether every path in `paths` (JSON array) lives on the same filesystem as the folder `dest`: a drop
        /// moves within one drive and copies between drives, like Windows.
        #[qinvokable]
        #[rust_name = "same_device"]
        fn sameDevice(&self, paths: &QString, dest: &QString) -> bool;
        /// `http(s)` URLs (JSON array) in a drop, e.g. a link or image dragged from a browser; same arguments as `dropPaths`.
        #[qinvokable]
        #[rust_name = "drop_urls"]
        fn dropUrls(&self, urls: &QString, text: &QString) -> QString;
        /// Download `urls` (JSON array) into `dest`. Reports through `opDone`.
        #[qinvokable]
        #[rust_name = "download_urls"]
        fn downloadUrls(self: Pin<&mut Self>, urls: &QString, dest: &QString);
        /// Whether CaptainFM is the system file chooser for other apps (xdg-desktop-portal).
        #[qinvokable]
        #[rust_name = "system_picker"]
        fn systemPicker(&self) -> bool;
        /// Make CaptainFM the system file chooser, or stop. Returns JSON `{"ok": bool, "message": string}`.
        #[qinvokable]
        #[rust_name = "set_system_picker"]
        fn setSystemPicker(&self, on: bool) -> QString;

        /// Put `paths` (JSON array) on the system clipboard as a file list.
        #[qinvokable]
        #[rust_name = "clipboard_write"]
        fn clipboardWrite(&self, paths: &QString, cut: bool);
        /// JSON `{"paths": [...], "cut": bool}` from the system clipboard, or `{}`.
        #[qinvokable]
        #[rust_name = "clipboard_read"]
        fn clipboardRead(&self) -> QString;

        /// Build a preview of `path`; answered by `previewReady`.
        #[qinvokable]
        #[rust_name = "request_preview"]
        fn requestPreview(self: Pin<&mut Self>, path: &QString);

        /// Register on the session bus (org.freedesktop.FileManager1). Call once at startup.
        #[qinvokable]
        #[rust_name = "start_services"]
        fn startServices(self: Pin<&mut Self>);
        /// Picker mode: print the chosen paths (JSON) to stdout and exit successfully.
        #[qinvokable]
        #[rust_name = "pick_done"]
        fn pickDone(&self, json: &QString);
        /// Picker mode: exit with status 1 (cancelled).
        #[qinvokable]
        #[rust_name = "pick_cancel"]
        fn pickCancel(&self);

        /// Another program asked for a folder or item to be shown (JSON `{kind, paths}`).
        #[qsignal]
        #[cxx_name = "fileManagerRequest"]
        fn file_manager_request(self: Pin<&mut Self>, json: QString);

        /// Cached freedesktop thumbnail for `path` as a file URL, or "".
        #[qinvokable]
        fn thumbnail(&self, path: &QString) -> QString;

        /// Applications that can open `path`, as JSON `[{id, name}]`.
        #[qinvokable]
        #[rust_name = "open_with_list"]
        fn openWithList(&self, path: &QString) -> QString;
        #[qinvokable]
        #[rust_name = "open_with"]
        fn openWith(&self, app_id: &QString, path: &QString);

        /// Installed plugins as JSON (see `plugins::list`).
        #[qinvokable]
        #[rust_name = "list_plugins"]
        fn listPlugins(&self) -> QString;
        /// Run a plugin's Lua action; answered by `pluginResult` (JSON array of effects).
        #[qinvokable]
        #[rust_name = "run_plugin"]
        fn runPlugin(
            self: Pin<&mut Self>,
            plugin: &QString,
            action: &QString,
            ctx: &QString,
            call_id: i32,
        );

        #[qsignal]
        #[cxx_name = "pluginResult"]
        fn plugin_result(self: Pin<&mut Self>, call_id: i32, json: QString);

        /// Number of paths currently indexed for deep search.
        #[qinvokable]
        #[rust_name = "index_paths"]
        fn indexPaths(&self) -> i32;
        /// Start watching for mounts and unmounts (once); each change asks for a new `sidebarDevices`.
        #[qinvokable]
        #[rust_name = "watch_devices"]
        fn watchDevices(self: Pin<&mut Self>);
        /// Disks and remotes for the sidebar; answered by `sidebarDevices`.
        #[qinvokable]
        #[rust_name = "list_sidebar_devices"]
        fn listSidebarDevices(self: Pin<&mut Self>);

        /// Look up how full one remote or cloud drive is (called when its row is hovered or opened). Answers a minute-old
        /// result at once; otherwise asks on a thread. Reports through `spaceReady`.
        #[qinvokable]
        #[rust_name = "request_space"]
        fn requestSpace(self: Pin<&mut Self>, path: &QString);
        /// `{used, total, usage}` for the mount at `path` (JSON), or an empty string when it reports nothing.
        #[qsignal]
        #[cxx_name = "spaceReady"]
        fn space_ready(self: Pin<&mut Self>, path: QString, json: QString);
        /// A copy or move was queued, so the transfer list should start refreshing.
        #[qsignal]
        #[cxx_name = "transferStarted"]
        fn transfer_started(self: Pin<&mut Self>);
        #[qsignal]
        #[cxx_name = "sidebarDevices"]
        fn sidebar_devices(self: Pin<&mut Self>, json: QString);

        /// Refresh the device list; answered by `devicesListed` (JSON array).
        #[qinvokable]
        #[rust_name = "list_devices"]
        fn listDevices(self: Pin<&mut Self>);
        /// Mount `device` (one JSON object from `devicesListed`); answered by `mountDone`.
        #[qinvokable]
        #[rust_name = "mount_device"]
        fn mountDevice(self: Pin<&mut Self>, device: &QString);
        /// Unmount, and eject removable media; answered by `mountDone`.
        #[qinvokable]
        #[rust_name = "eject_device"]
        fn ejectDevice(self: Pin<&mut Self>, device: &QString);
        /// Mount a server URL (smb://, sftp://, ...); may emit `credentialsNeeded`.
        #[qinvokable]
        #[rust_name = "connect_remote"]
        fn connectRemote(self: Pin<&mut Self>, url: &QString);
        /// Connect a profile from the connection form: a JSON profile plus `password` and `save`;
        /// answered by `mountDone`.
        #[qinvokable]
        #[rust_name = "connect_profile"]
        fn connectProfile(self: Pin<&mut Self>, json: &QString);
        /// Connect a saved connection by id, using the keyring password if one was kept.
        #[qinvokable]
        #[rust_name = "connect_saved"]
        fn connectSaved(self: Pin<&mut Self>, id: &QString);
        /// Connect whatever `path` lives on (a pin on a share that is not mounted); answers through `mountDone`.
        #[qinvokable]
        #[rust_name = "connect_for"]
        fn connectFor(self: Pin<&mut Self>, path: &QString);
        /// The connection `path` belongs to, mounted or not ("" for none). Reads no remote.
        #[qinvokable]
        #[rust_name = "home_label"]
        fn homeLabel(&self, path: &QString) -> QString;
        /// The folder last visited inside `root`, or "".
        #[qinvokable]
        #[rust_name = "last_visit_under"]
        fn lastVisitUnder(&self, root: &QString) -> QString;
        /// Bring back the dead remote `folder` is on; answers through `mountDone` (its path is `folder`).
        #[qinvokable]
        fn reconnect(self: Pin<&mut Self>, folder: &QString);
        /// Saved connections as a JSON array (never includes passwords).
        #[qinvokable]
        #[rust_name = "saved_connections"]
        fn savedConnections(&self) -> QString;
        /// Forget a saved connection and its keyring password; answered by `mountDone`.
        #[qinvokable]
        #[rust_name = "delete_connection"]
        fn deleteConnection(self: Pin<&mut Self>, id: &QString);
        /// Save changes to a saved connection (JSON profile with `id` and `password`); answered by `mountDone`.
        #[qinvokable]
        #[rust_name = "update_connection"]
        fn updateConnection(self: Pin<&mut Self>, json: &QString);
        /// Reply to `credentialsNeeded`; `cancel` aborts the connection.
        #[qinvokable]
        #[rust_name = "answer_prompt"]
        fn answerPrompt(&self, text: &QString, cancel: bool);

        #[qsignal]
        #[cxx_name = "devicesListed"]
        fn devices_listed(self: Pin<&mut Self>, json: QString);
        #[qsignal]
        #[cxx_name = "mountDone"]
        fn mount_done(self: Pin<&mut Self>, json: QString);
        #[qsignal]
        #[cxx_name = "credentialsNeeded"]
        fn credentials_needed(self: Pin<&mut Self>, prompt: QString);

        /// A paste would hit names `dest` already has: `{paths, dest, cut, names}`. Answer with `pasteWith`.
        #[qsignal]
        #[cxx_name = "pasteConflicts"]
        fn paste_conflicts(self: Pin<&mut Self>, json: QString);
        #[qsignal]
        #[cxx_name = "opDone"]
        fn op_done(self: Pin<&mut Self>, result: QString);
        #[qsignal]
        #[cxx_name = "previewReady"]
        fn preview_ready(self: Pin<&mut Self>, path: QString, json: QString);
    }

    impl cxx_qt::Threading for Backend {}
}

#[derive(Default)]
pub struct BackendRust {
    /// Keeps the FileManager1 bus connection alive.
    bus: Option<zbus::blocking::Connection>,
    /// Keeps the `org.captainfm.App` connection (for `--tab`) alive.
    app_bus: Option<zbus::blocking::Connection>,
    /// Watches the state folder so bookmarks pinned elsewhere show up live.
    state_watch: Option<crate::watch::Watch>,
}

fn q(s: &str) -> QString {
    QString::from(s)
}

fn parse_paths(json: &QString) -> Vec<String> {
    serde_json::from_str(&json.to_string()).unwrap_or_default()
}

/// Order `items` for a picker query. `re:<pattern>` or `/<pattern>/` is a regular expression
/// (smart case); anything else is zoxide-style ordered terms, falling back to fuzzy matching
/// when no title contains all the terms.
fn rank_items(items: &[String], query: &str, fuzzy_fallback: bool) -> Vec<usize> {
    let pattern = query
        .strip_prefix("re:")
        .or_else(|| query.strip_prefix('/').and_then(|r| r.strip_suffix('/')));
    if let Some(pattern) = pattern {
        let insensitive = !pattern.chars().any(char::is_uppercase);
        // An unfinished pattern matches nothing rather than everything.
        let Ok(re) = regex::RegexBuilder::new(pattern)
            .case_insensitive(insensitive)
            .build()
        else {
            return Vec::new();
        };
        // Regex results keep the list's own order.
        return items
            .iter()
            .enumerate()
            .filter(|(_, s)| re.is_match(s))
            .map(|(i, _)| i)
            .collect();
    }
    let mut scored: Vec<(i32, usize)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, s)| crate::fuzzy::ordered_terms_score(query, s).map(|sc| (sc, i)))
        .collect();
    if scored.is_empty() && fuzzy_fallback {
        let fuzzy = crate::fuzzy::Query::parse(query);
        scored = items
            .iter()
            .enumerate()
            .filter_map(|(i, s)| fuzzy.score(s, s).map(|sc| (sc, i)))
            .collect();
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, i)| i).collect()
}

fn file_url(path: &str) -> String {
    format!("file://{}", ops::percent_encode_path(path))
}

/// The drag payload for `paths`: an RFC 2483 `text/uri-list` for file managers and upload fields, and the
/// plain paths (one per line) for editors and terminals.
fn drag_payload(paths: &[String]) -> serde_json::Value {
    let uri_list: String = paths
        .iter()
        .map(|p| format!("{}\r\n", file_url(p)))
        .collect();
    serde_json::json!({"uriList": uri_list, "plain": paths.join("\n")})
}

/// A yes/no lookup QML waits for. On a remote it gets about 150 ms on a helper thread and is then taken as
/// yes, so a slow server never freezes the UI; whoever acts on the answer finds out soon enough.
fn bounded_lookup(path: String, look: fn(&str) -> bool) -> bool {
    match listing::remote_root(&path) {
        None => look(&path),
        Some(root) => bounded_lookup_on(&root, path, look, Duration::from_millis(150)),
    }
}

/// Remotes that did not answer in time, and when. For a while after, lookups there do not wait at all:
/// the sidebar asks about every pin in turn, and five pins on a hung server would otherwise freeze it
/// for five waits, each leaving a thread stuck behind.
static SLOW_REMOTES: std::sync::Mutex<Vec<(String, Instant)>> = std::sync::Mutex::new(Vec::new());
const SLOW_FOR: Duration = Duration::from_secs(10);

fn bounded_lookup_on(root: &str, path: String, look: fn(&str) -> bool, wait: Duration) -> bool {
    let recently_slow = |slow: &mut Vec<(String, Instant)>| {
        slow.retain(|(_, at)| at.elapsed() < SLOW_FOR);
        slow.iter().any(|(r, _)| r == root)
    };
    if recently_slow(&mut SLOW_REMOTES.lock().unwrap()) {
        return true;
    }
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(look(&path));
    });
    rx.recv_timeout(wait).unwrap_or_else(|_| {
        let mut slow = SLOW_REMOTES.lock().unwrap();
        if !recently_slow(&mut slow) {
            slow.push((root.to_owned(), Instant::now()));
        }
        true
    })
}

/// Whether moving `paths` into `dest` stays on one filesystem. Remote mounts are compared by their mount
/// point, so hovering a drag over a remote folder never waits on the server.
fn same_device(paths: &[String], dest: &str) -> bool {
    use std::os::unix::fs::MetadataExt;
    if paths.is_empty() {
        return false;
    }
    let dest_remote = listing::remote_root(dest);
    if dest_remote.is_some() || paths.iter().any(|p| listing::is_remote(p)) {
        return paths.iter().all(|p| listing::remote_root(p) == dest_remote);
    }
    let Ok(target) = std::fs::metadata(dest) else {
        return false;
    };
    !paths.is_empty()
        && paths.iter().all(|p| {
            // A link is judged by the link itself, since that is what a move relocates.
            std::fs::symlink_metadata(p).is_ok_and(|m| m.dev() == target.dev())
        })
}

/// Local paths in a drop: `file:` URLs (also `file://localhost/...`), else absolute paths or `file:` URLs
/// one per line in `text`. Remote schemes (http, smb, ...) are not local files and are skipped.
fn drop_paths(urls: &[String], text: &str) -> Vec<String> {
    let local = |s: &str| -> Option<String> {
        let s = s.trim();
        let rest = s
            .strip_prefix("file://localhost")
            .or_else(|| s.strip_prefix("file://"));
        match rest {
            Some(r) if r.starts_with('/') => Some(percent_decode(r)),
            Some(_) => None,
            None => s.starts_with('/').then(|| s.to_owned()),
        }
    };
    let mut out: Vec<String> = urls.iter().filter_map(|u| local(u)).collect();
    if out.is_empty() {
        out = text
            .lines()
            .filter_map(local)
            .filter(|p| std::path::Path::new(p).exists())
            .collect();
    }
    out.dedup();
    out
}

/// Web addresses in a drop: `http(s)` URLs in `urls`, else one per line in `text` (browsers offer a dragged link or
/// image as text plus their own formats, not as a URI list).
fn drop_urls(urls: &[String], text: &str) -> Vec<String> {
    let web = |s: &str| {
        let s = s.trim();
        (s.starts_with("http://") || s.starts_with("https://")).then(|| s.to_owned())
    };
    let mut out: Vec<String> = urls.iter().filter_map(|u| web(u)).collect();
    if out.is_empty() {
        let lines: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        // Only when every line is an address: selected text that merely contains one is not a download.
        if !lines.is_empty() && lines.iter().all(|l| web(l).is_some() && !l.contains(' ')) {
            out = lines.iter().filter_map(|l| web(l)).collect();
        }
    }
    out.dedup();
    out
}

fn parse_file_list(text: &str) -> (Vec<String>, bool) {
    let mut cut = false;
    let mut paths = Vec::new();
    for line in text.lines() {
        match line.trim() {
            "cut" => cut = true,
            "copy" | "" => {}
            l if l.starts_with("file://") => {
                let raw = &l["file://".len()..];
                paths.push(percent_decode(raw));
            }
            _ => {}
        }
    }
    (paths, cut)
}

use crate::dbus::percent_decode;

impl qobject::Backend {
    fn home(&self) -> QString {
        q(&state::home().to_string_lossy())
    }

    fn version(&self) -> QString {
        q(env!("CARGO_PKG_VERSION"))
    }

    fn since_start_ms(&self) -> i32 {
        crate::START
            .get()
            .map_or(0, |t| t.elapsed().as_millis() as i32)
    }

    fn env(&self, name: &QString) -> QString {
        q(&std::env::var(name.to_string()).unwrap_or_default())
    }

    fn read_config(&self, name: &QString) -> QString {
        q(&state::read_config(&name.to_string()))
    }

    fn write_config(&self, name: &QString, json: &QString) -> bool {
        state::write_config(&name.to_string(), &json.to_string())
    }

    fn visit(&self, path: &QString) {
        state::record_visit(&path.to_string());
    }

    fn jump(&self, query: &QString) -> QString {
        let items = state::jump_candidates(&query.to_string());
        q(&serde_json::to_string(&items).unwrap_or_else(|_| "[]".into()))
    }

    fn bookmarks(&self) -> QString {
        q(&serde_json::to_string(&state::bookmarks()).unwrap_or_else(|_| "[]".into()))
    }

    fn toggle_bookmark(mut self: Pin<&mut Self>, path: &QString) -> bool {
        let pinned = state::toggle_bookmark(&path.to_string());
        self.as_mut().bookmarks_changed();
        pinned
    }

    fn mount_root(&self, path: &QString) -> QString {
        q(&state::mount_root(&path.to_string()))
    }

    fn search_root(&self, path: &QString) -> QString {
        q(&state::search_root(&path.to_string()))
    }

    /// Run `work` off the UI thread and deliver its outcome through `opDone`.
    /// Operations run one at a time in submission order, so undo always sees finished work.
    fn run_op(self: Pin<&mut Self>, work: impl FnOnce() -> ops::Outcome + Send + 'static) {
        let qt_thread = self.qt_thread();
        op_queue(Box::new(move || {
            let json = work().json();
            let _ = qt_thread.queue(move |backend| backend.op_done(q(&json)));
        }));
    }

    /// Queue a copy or move that shows up in the transfer list.
    fn start_paste(
        mut self: Pin<&mut Self>,
        paths: Vec<String>,
        dest: String,
        cut: bool,
        policy: Policy,
    ) {
        let ctl = Ctl::register(if cut { "Moved" } else { "Copied" }, &dest, paths.len());
        self.as_mut().transfer_started();
        self.run_op(move || ops::paste_tracked(&ctl, &paths, &dest, cut, policy));
    }

    fn link(self: Pin<&mut Self>, paths: &QString, dest: &QString) {
        let (paths, dest) = (parse_paths(paths), dest.to_string());
        self.run_op(move || ops::link(&paths, &dest));
    }

    fn extract(self: Pin<&mut Self>, paths: &QString, into_folder: bool) {
        let paths = parse_paths(paths);
        self.run_op(move || ops::extract(&paths, into_folder));
    }

    fn paste(mut self: Pin<&mut Self>, paths: &QString, dest: &QString, cut: bool) {
        let (list, dest) = (parse_paths(paths), dest.to_string());
        let names = ops::conflicts(&list, &dest);
        if names.is_empty() {
            self.start_paste(list, dest, cut, Policy::Both);
        } else {
            let json = serde_json::json!({"paths": list, "dest": dest, "cut": cut, "names": names,
                "replaceDeletes": ops::deletes_for_good(&dest)});
            self.as_mut().paste_conflicts(q(&json.to_string()));
        }
    }

    fn paste_with(
        self: Pin<&mut Self>,
        paths: &QString,
        dest: &QString,
        cut: bool,
        policy: &QString,
    ) {
        self.start_paste(
            parse_paths(paths),
            dest.to_string(),
            cut,
            Policy::parse(&policy.to_string()),
        );
    }

    fn transfers(&self) -> QString {
        q(&transfer::snapshot())
    }

    fn transfer_cancel(&self, id: i32) {
        transfer::cancel(id);
    }

    fn transfer_pause(&self, id: i32, paused: bool) {
        transfer::pause(id, paused);
    }

    fn transfer_clear(&self) {
        transfer::clear_finished();
    }

    fn trash(self: Pin<&mut Self>, paths: &QString) {
        let paths = parse_paths(paths);
        self.run_op(move || ops::trash(&paths));
    }

    fn remove(self: Pin<&mut Self>, paths: &QString) {
        let paths = parse_paths(paths);
        self.run_op(move || ops::delete(&paths));
    }

    fn trash_count(&self) -> i32 {
        ops::trash_count() as i32
    }

    fn empty_trash(self: Pin<&mut Self>) {
        self.run_op(ops::empty_trash);
    }

    fn rename_to(self: Pin<&mut Self>, path: &QString, new_name: &QString) {
        let (path, name) = (path.to_string(), new_name.to_string());
        self.run_op(move || ops::rename(&path, &name));
    }

    fn create(self: Pin<&mut Self>, dir: &QString, name: &QString, is_dir: bool) {
        let (dir, name) = (dir.to_string(), name.to_string());
        self.run_op(move || ops::create(&dir, &name, is_dir));
    }

    fn undo(self: Pin<&mut Self>) {
        self.run_op(|| ops::step_history(false));
    }

    fn redo(self: Pin<&mut Self>) {
        self.run_op(|| ops::step_history(true));
    }

    fn rename_many(self: Pin<&mut Self>, pairs: &QString) {
        let pairs: Vec<(String, String)> =
            serde_json::from_str(&pairs.to_string()).unwrap_or_default();
        self.run_op(move || ops::rename_many(&pairs));
    }

    fn rename_preview(&self, paths: &QString, opts: &QString) -> QString {
        let paths = parse_paths(paths);
        let opts: crate::batch::Opts = serde_json::from_str(&opts.to_string()).unwrap_or_default();
        let json = match crate::batch::preview(&paths, &opts) {
            Ok(rows) => serde_json::json!({ "rows": rows }),
            Err(e) => serde_json::json!({ "error": e }),
        };
        q(&json.to_string())
    }

    fn start_services(mut self: Pin<&mut Self>) {
        // Follow bookmark changes made by other windows: watch the state file's folder.
        let state_dir = state::store_file()
            .parent()
            .map(std::path::Path::to_path_buf);
        if let Some(dir) = state_dir {
            let _ = std::fs::create_dir_all(&dir);
            let qt = self.qt_thread();
            let watcher = crate::watch::watch(
                &dir,
                false,
                std::time::Duration::from_millis(150),
                move || {
                    let _ = qt.queue(|b| {
                        state::reset_store();
                        b.bookmarks_changed();
                    });
                },
            );
            self.as_mut().rust_mut().state_watch = watcher;
        }
        // Only own org.freedesktop.FileManager1 when CaptainFM is the folder handler (or when
        // forced), so running it never steals "show in folder" from the user's file manager.
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            // Always answer `captainfm --tab` from the command line.
            let app_notify = qt_thread.clone();
            let app_conn = crate::dbus::serve_app(move |json| {
                let _ = app_notify.queue(move |b| b.file_manager_request(q(&json)));
            });
            let _ = qt_thread.queue(move |mut b| b.as_mut().rust_mut().app_bus = app_conn);
            if !crate::cli::is_default_handler() && std::env::var_os("CFM_DBUS").is_none() {
                return;
            }
            let notify = qt_thread.clone();
            let connection = crate::dbus::serve_file_manager(move |json| {
                let _ = notify.queue(move |b| b.file_manager_request(q(&json)));
            });
            let _ = qt_thread.queue(move |mut b| b.as_mut().rust_mut().bus = connection);
        });
    }

    fn pick_done(&self, json: &QString) {
        use std::io::Write;
        let mut out = std::io::stdout();
        let _ = writeln!(out, "{}", json);
        let _ = out.flush();
        std::process::exit(0);
    }

    fn pick_cancel(&self) {
        std::process::exit(1);
    }

    fn thumbnail(&self, path: &QString) -> QString {
        q(&crate::thumbnails::lookup(&path.to_string()).unwrap_or_default())
    }

    fn open_with_list(&self, path: &QString) -> QString {
        let apps = crate::openwith::apps_for(&path.to_string());
        q(&serde_json::to_string(&apps).unwrap_or_else(|_| "[]".into()))
    }

    fn open_with(&self, app_id: &QString, path: &QString) {
        crate::openwith::launch(&app_id.to_string(), &path.to_string());
    }

    fn list_plugins(&self) -> QString {
        q(&plugins::list().to_string())
    }

    fn run_plugin(
        self: Pin<&mut Self>,
        plugin: &QString,
        action: &QString,
        ctx: &QString,
        call_id: i32,
    ) {
        let (plugin, action) = (plugin.to_string(), action.to_string());
        let ctx: serde_json::Value = serde_json::from_str(&ctx.to_string()).unwrap_or_default();
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let json = plugins::run_action(&plugin, &action, &ctx).to_string();
            let _ = qt_thread.queue(move |b| b.plugin_result(call_id, q(&json)));
        });
    }

    fn index_paths(&self) -> i32 {
        crate::index::total_paths() as i32
    }

    fn list_sidebar_devices(self: Pin<&mut Self>) {
        use std::sync::atomic::{AtomicBool, Ordering};
        // One listing at a time: a hung network mount must not pile up threads. A request that comes in
        // meanwhile is answered by one more listing afterwards.
        static RUNNING: AtomicBool = AtomicBool::new(false);
        static AGAIN: AtomicBool = AtomicBool::new(false);
        if RUNNING.swap(true, Ordering::SeqCst) {
            AGAIN.store(true, Ordering::SeqCst);
            return;
        }
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            loop {
                AGAIN.store(false, Ordering::SeqCst);
                let json = serde_json::to_string(&devices::list()).unwrap_or_else(|_| "[]".into());
                let _ = qt_thread.queue(move |b| b.sidebar_devices(q(&json)));
                if !AGAIN.load(Ordering::SeqCst) {
                    RUNNING.store(false, Ordering::SeqCst);
                    return;
                }
            }
        });
    }

    fn request_space(mut self: Pin<&mut Self>, path: &QString) {
        use std::collections::HashSet;
        use std::sync::Mutex;
        use std::time::Instant;
        static CACHE: Mutex<Option<devices::SpaceCache>> = Mutex::new(None);
        static ASKING: Mutex<Option<HashSet<String>>> = Mutex::new(None);
        let path = path.to_string();
        let json = |v: Option<(u64, u64)>| {
            match v {
            Some((used, total)) if total > 0 => serde_json::json!({"used": used, "total": total, "usage": (used as f64 * 100.0 / total as f64).round() as i64}).to_string(),
            _ => String::new(),
        }
        };
        let cached = CACHE
            .lock()
            .unwrap()
            .get_or_insert_with(Default::default)
            .get(&path, Instant::now());
        if let Some(found) = cached {
            self.as_mut().space_ready(q(&path), q(&json(found)));
            return;
        }
        // One question per remote at a time; a hung server must not collect threads.
        if !ASKING
            .lock()
            .unwrap()
            .get_or_insert_with(Default::default)
            .insert(path.clone())
        {
            return;
        }
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let found = devices::remote_space(&path);
            CACHE
                .lock()
                .unwrap()
                .get_or_insert_with(Default::default)
                .put(&path, Instant::now(), found);
            ASKING
                .lock()
                .unwrap()
                .get_or_insert_with(Default::default)
                .remove(&path);
            let _ = qt_thread.queue(move |b| b.space_ready(q(&path), q(&json(found))));
        });
    }

    fn watch_devices(self: Pin<&mut Self>) {
        static STARTED: std::sync::Once = std::sync::Once::new();
        let qt_thread = self.qt_thread();
        STARTED.call_once(|| {
            devices::watch_changes(move || {
                let _ = qt_thread.queue(|b| b.list_sidebar_devices());
            });
        });
    }

    fn list_devices(self: Pin<&mut Self>) {
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let json = serde_json::to_string(&devices::list()).unwrap_or_else(|_| "[]".into());
            let _ = qt_thread.queue(move |b| b.devices_listed(q(&json)));
        });
    }

    fn mount_device(self: Pin<&mut Self>, device: &QString) {
        let dev: serde_json::Value = serde_json::from_str(&device.to_string()).unwrap_or_default();
        let field = |k: &str| dev.get(k).and_then(|v| v.as_str()).unwrap_or("").to_owned();
        let (kind, device_path, uri) = (field("kind"), field("device"), field("uri"));
        let this = self;
        if kind == "disk" {
            this.report_mount(move || devices::mount_disk(&device_path));
        } else {
            this.mount_with_prompts(uri);
        }
    }

    fn eject_device(self: Pin<&mut Self>, device: &QString) {
        let dev: serde_json::Value = serde_json::from_str(&device.to_string()).unwrap_or_default();
        let field = |k: &str| dev.get(k).and_then(|v| v.as_str()).unwrap_or("").to_owned();
        let (kind, device_path, path, removable) = (
            field("kind"),
            field("device"),
            field("path"),
            dev.get("removable")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        );
        if kind == "disk" {
            self.report_mount(move || devices::eject_disk(&device_path, removable));
        } else {
            // A gvfs mount with no FUSE directory has only its uri to unmount by.
            let target = if path.is_empty() { field("uri") } else { path };
            self.report_mount(move || remotes::unmount(&target));
        }
    }

    fn connect_remote(self: Pin<&mut Self>, url: &QString) {
        self.mount_with_prompts(url.to_string());
    }

    fn connect_profile(self: Pin<&mut Self>, json: &QString) {
        let v: serde_json::Value = serde_json::from_str(&json.to_string()).unwrap_or_default();
        let profile: remotes::Profile = serde_json::from_value(v.clone()).unwrap_or_default();
        let password = v
            .get("password")
            .and_then(|p| p.as_str())
            .unwrap_or("")
            .to_owned();
        let save = v.get("save").and_then(|p| p.as_bool()).unwrap_or(false);
        self.connect_job(move |ask| remotes::connect(profile, password, save, ask));
    }

    fn connect_saved(self: Pin<&mut Self>, id: &QString) {
        let id = id.to_string();
        self.connect_job(move |ask| remotes::connect_saved(&id, ask));
    }

    fn connect_for(self: Pin<&mut Self>, path: &QString) {
        let path = path.to_string();
        self.connect_job(move |ask| remotes::connect_for(&path, ask));
    }

    fn home_label(&self, path: &QString) -> QString {
        q(&remotes::home_label(&path.to_string()))
    }

    fn last_visit_under(&self, root: &QString) -> QString {
        q(&state::last_visit_under(&root.to_string()).unwrap_or_default())
    }

    fn reconnect(self: Pin<&mut Self>, folder: &QString) {
        let folder = folder.to_string();
        let root = listing::remote_root(&folder).unwrap_or_else(|| folder.clone());
        self.connect_job(move |ask| remotes::reconnect(&folder, &root, ask));
    }

    fn saved_connections(&self) -> QString {
        q(&serde_json::to_string(&remotes::load()).unwrap_or_else(|_| "[]".into()))
    }

    fn delete_connection(self: Pin<&mut Self>, id: &QString) {
        let id = id.to_string();
        self.report_mount(move || remotes::delete(&id));
    }

    fn update_connection(self: Pin<&mut Self>, json: &QString) {
        let v: serde_json::Value = serde_json::from_str(&json.to_string()).unwrap_or_default();
        let profile: remotes::Profile = serde_json::from_value(v.clone()).unwrap_or_default();
        let password = v
            .get("password")
            .and_then(|p| p.as_str())
            .unwrap_or("")
            .to_owned();
        self.report_mount(move || remotes::update(profile, &password));
    }

    fn answer_prompt(&self, text: &QString, cancel: bool) {
        devices::answer_prompt(if cancel { None } else { Some(text.to_string()) });
    }

    /// Run a mount-like job on a thread and report it through `mountDone`.
    fn report_mount(
        self: Pin<&mut Self>,
        work: impl FnOnce() -> devices::MountResult + Send + 'static,
    ) {
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let json = serde_json::to_string(&work()).unwrap_or_default();
            let _ = qt_thread.queue(move |b| b.mount_done(q(&json)));
        });
    }

    /// Mount a GVfs URI, asking the UI for credentials when gio prompts for them.
    fn mount_with_prompts(self: Pin<&mut Self>, uri: String) {
        self.connect_job(move |ask| devices::mount_uri(&uri, ask));
    }

    /// Run a connect job on a thread; `ask` relays credential prompts to the UI and waits for the answer.
    fn connect_job(
        self: Pin<&mut Self>,
        work: impl FnOnce(&dyn Fn(&str) -> Option<String>) -> devices::MountResult + Send + 'static,
    ) {
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let answers = devices::begin_prompting();
            let asker = qt_thread.clone();
            let result = work(&|prompt: &str| {
                let prompt = prompt.to_owned();
                let _ = asker.queue(move |b| b.credentials_needed(q(&prompt)));
                answers
                    .recv_timeout(std::time::Duration::from_secs(180))
                    .ok()
                    .flatten()
            });
            let json = serde_json::to_string(&result).unwrap_or_default();
            let _ = qt_thread.queue(move |b| b.mount_done(q(&json)));
        });
    }

    fn exists(&self, path: &QString) -> bool {
        bounded_lookup(path.to_string(), |p| std::fs::symlink_metadata(p).is_ok())
    }

    fn is_dir(&self, path: &QString) -> bool {
        bounded_lookup(path.to_string(), |p| std::path::Path::new(p).is_dir())
    }

    fn expand_path(&self, path: &QString) -> QString {
        q(&state::expand_tilde(&path.to_string()))
    }

    fn rank(&self, items: &QString, query: &QString) -> QString {
        let items: Vec<String> = serde_json::from_str(&items.to_string()).unwrap_or_default();
        let order = rank_items(&items, query.to_string().trim(), true);
        q(&serde_json::to_string(&order).unwrap_or_else(|_| "[]".into()))
    }

    fn rank_strict(&self, items: &QString, query: &QString) -> QString {
        let items: Vec<String> = serde_json::from_str(&items.to_string()).unwrap_or_default();
        let order = rank_items(&items, query.to_string().trim(), false);
        q(&serde_json::to_string(&order).unwrap_or_else(|_| "[]".into()))
    }

    fn held_modifiers(&self) -> i32 {
        // SAFETY: a plain getter defined in src/cpp/modifiers.cpp.
        unsafe { cfm_held_modifiers() }
    }

    fn rank_commands(
        &self,
        items: &QString,
        ids: &QString,
        query: &QString,
        learn: bool,
    ) -> QString {
        let items: Vec<String> = serde_json::from_str(&items.to_string()).unwrap_or_default();
        let ids: Vec<String> = serde_json::from_str(&ids.to_string()).unwrap_or_default();
        let query = query.to_string();
        let query = query.trim();
        // Regular expressions keep the plain ranking.
        let order = if query.starts_with("re:")
            || (query.len() > 1 && query.starts_with('/') && query.ends_with('/'))
        {
            rank_items(&items, query, false)
        } else {
            let usage = if learn {
                state::Usage::load()
            } else {
                state::Usage::default()
            };
            crate::fuzzy::predict_rank(&items, &ids, query, &usage)
        };
        q(&serde_json::to_string(&order).unwrap_or_else(|_| "[]".into()))
    }

    fn record_choice(&self, query: &QString, id: &QString) {
        let (query, id) = (query.to_string(), id.to_string());
        state::Usage::update(|u| u.record(&query, &id, state::now()));
    }

    fn mark_feature(&self, id: &QString) {
        state::FeatureUse::mark(&id.to_string());
    }

    fn feature_report(&self) -> QString {
        q(&state::FeatureUse::report((state::now() / 86_400) as u32).to_string())
    }

    fn forget_feature_use(&self) {
        state::FeatureUse::forget();
    }

    fn forget_palette_history(&self) {
        state::Usage::forget();
    }

    fn copy_text(&self, text: &QString) {
        let Ok(mut child) = Command::new("wl-copy")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            return;
        };
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            let _ = stdin.write_all(text.to_string().as_bytes());
        }
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }

    fn open_path(&self, path: &QString) {
        spawn_detached("xdg-open", &[&path.to_string()], None);
    }

    fn open_terminal(&self, dir: &QString) {
        let dir = dir.to_string();
        // `terminal` in settings.json is a command line (`kitty --single-instance`); default xdg-terminal-exec.
        let settings: serde_json::Value =
            serde_json::from_str(&state::read_config("settings")).unwrap_or_default();
        let custom = settings
            .get("terminal")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .trim()
            .to_owned();
        let mut words = custom.split_whitespace();
        match words.next() {
            Some(program) => {
                let rest: Vec<&str> = words.collect();
                spawn_detached(program, &rest, Some(&dir));
            }
            None => spawn_detached("xdg-terminal-exec", &[], Some(&dir)),
        }
    }

    fn drag_payload(&self, paths: &QString) -> QString {
        let paths: Vec<String> = serde_json::from_str(&paths.to_string()).unwrap_or_default();
        q(&drag_payload(&paths).to_string())
    }

    fn drop_paths(&self, urls: &QString, text: &QString) -> QString {
        let urls: Vec<String> = serde_json::from_str(&urls.to_string()).unwrap_or_default();
        q(&serde_json::json!(drop_paths(&urls, &text.to_string())).to_string())
    }

    fn same_device(&self, paths: &QString, dest: &QString) -> bool {
        let paths: Vec<String> = serde_json::from_str(&paths.to_string()).unwrap_or_default();
        same_device(&paths, &dest.to_string())
    }

    fn drop_urls(&self, urls: &QString, text: &QString) -> QString {
        let urls: Vec<String> = serde_json::from_str(&urls.to_string()).unwrap_or_default();
        q(&serde_json::json!(drop_urls(&urls, &text.to_string())).to_string())
    }

    fn download_urls(self: Pin<&mut Self>, urls: &QString, dest: &QString) {
        let (urls, dest) = (parse_paths(urls), dest.to_string());
        self.run_op(move || ops::download(&urls, &dest));
    }

    fn system_picker(&self) -> bool {
        crate::cli::portal_installed()
    }

    fn set_system_picker(&self, on: bool) -> QString {
        let (ok, message) = match crate::cli::set_portal(on) {
            Ok(m) => (true, m),
            Err(e) => (false, e),
        };
        q(&serde_json::json!({"ok": ok, "message": message}).to_string())
    }

    fn clipboard_write(&self, paths: &QString, cut: bool) {
        let paths = parse_paths(paths);
        if paths.is_empty() {
            return;
        }
        let mut body = String::from(if cut { "cut" } else { "copy" });
        for p in &paths {
            body.push('\n');
            body.push_str(&file_url(p));
        }
        let Ok(mut child) = Command::new("wl-copy")
            .args(["--type", "x-special/gnome-copied-files"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            return;
        };
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            let _ = stdin.write_all(body.as_bytes());
        }
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }

    fn clipboard_read(&self) -> QString {
        let output = |args: &[&str]| {
            Command::new("wl-paste")
                .args(args)
                .stderr(Stdio::null())
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        };
        let types = output(&["--list-types"]).unwrap_or_default();
        let text = if types.lines().any(|t| t == "x-special/gnome-copied-files") {
            output(&["--no-newline", "--type", "x-special/gnome-copied-files"])
        } else if types.lines().any(|t| t == "text/uri-list") {
            output(&["--no-newline", "--type", "text/uri-list"])
        } else {
            None
        };
        let Some(text) = text else { return q("{}") };
        let (paths, cut) = parse_file_list(&text);
        q(&serde_json::json!({"paths": paths, "cut": cut}).to_string())
    }

    fn request_preview(self: Pin<&mut Self>, path: &QString) {
        let path = path.to_string();
        let qt_thread = self.qt_thread();
        let deliver = move |path: String| {
            let json = preview::preview(&path).to_string();
            let _ = qt_thread.queue(move |backend| backend.preview_ready(q(&path), q(&json)));
        };
        if !listing::is_remote(&path) {
            std::thread::spawn(move || deliver(path));
            return;
        }
        // On a remote, one preview read at a time: moving the cursor along queues only the latest file.
        static REMOTE: LatestOnly<String> = LatestOnly::new();
        if REMOTE.offer(path) {
            std::thread::spawn(move || {
                while let Some(path) = REMOTE.next() {
                    deliver(path);
                }
            });
        }
    }
}

/// A one-slot queue with a single worker: a new item replaces the one waiting, so the worker only ever
/// picks up the latest. `offer` says whether to start the worker; the worker calls `next` until `None`.
struct LatestOnly<T> {
    state: std::sync::Mutex<(bool, Option<T>)>,
}

impl<T> LatestOnly<T> {
    const fn new() -> Self {
        LatestOnly {
            state: std::sync::Mutex::new((false, None)),
        }
    }

    fn offer(&self, item: T) -> bool {
        let mut s = self.state.lock().unwrap();
        s.1 = Some(item);
        !std::mem::replace(&mut s.0, true)
    }

    fn next(&self) -> Option<T> {
        let mut s = self.state.lock().unwrap();
        let item = s.1.take();
        s.0 = item.is_some();
        item
    }
}

type Job = Box<dyn FnOnce() + Send>;

/// Hand a job to the single file-operation worker thread (started on first use).
fn op_queue(job: Job) {
    use std::sync::{OnceLock, mpsc};
    static TX: OnceLock<mpsc::Sender<Job>> = OnceLock::new();
    let tx = TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::spawn(move || {
            for job in rx {
                job();
            }
        });
        tx
    });
    let _ = tx.send(job);
}

/// Start a program that outlives this call, reaping it in the background.
fn spawn_detached(program: &str, args: &[&str], cwd: Option<&str>) {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    if let Ok(mut child) = cmd.spawn() {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drag_payload_carries_uris_and_plain_paths() {
        let v = drag_payload(&["/a b/c#1".to_owned(), "/d".to_owned()]);
        assert_eq!(v["uriList"], "file:///a%20b/c%231\r\nfile:///d\r\n");
        assert_eq!(v["plain"], "/a b/c#1\n/d");
    }

    #[test]
    fn drops_accept_local_urls_and_ignore_remote_ones() {
        let urls = [
            "file:///a%20b".to_owned(),
            "file://localhost/x".to_owned(),
            "https://example.com/f".to_owned(),
            "smb://nas/s".to_owned(),
        ];
        assert_eq!(drop_paths(&urls, ""), ["/a b", "/x"]);
        assert!(drop_paths(&["https://example.com/f".to_owned()], "").is_empty());
        assert_eq!(drop_paths(&[], "/tmp\nnot a path\nfile:///tmp"), ["/tmp"]);
    }

    static LOOKS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    fn hung_lookup(_: &str) -> bool {
        LOOKS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(400));
        false
    }

    fn quick_no(_: &str) -> bool {
        false
    }

    #[test]
    fn a_remote_that_does_not_answer_is_not_waited_on_again() {
        let wait = Duration::from_millis(60);
        let root = "/test/slow-remote";
        let t = Instant::now();
        assert!(
            bounded_lookup_on(root, format!("{root}/a"), hung_lookup, wait),
            "no answer counts as yes"
        );
        assert!(t.elapsed() >= wait);
        // The second and third lookups there return at once, without asking the server.
        let t = Instant::now();
        assert!(bounded_lookup_on(
            root,
            format!("{root}/b"),
            hung_lookup,
            wait
        ));
        assert!(bounded_lookup_on(
            root,
            format!("{root}/c"),
            hung_lookup,
            wait
        ));
        assert!(t.elapsed() < wait);
        assert_eq!(LOOKS.load(std::sync::atomic::Ordering::SeqCst), 1);
        // Another remote still gets its real answer.
        assert!(!bounded_lookup_on(
            "/test/other-remote",
            "/test/other-remote/x".into(),
            quick_no,
            wait
        ));
    }

    #[test]
    fn only_the_latest_preview_waits_while_one_is_read() {
        let q = LatestOnly::new();
        assert!(q.offer("a"), "the first offer starts the worker");
        assert_eq!(q.next(), Some("a"));
        // While "a" is being read, the cursor moves on: only the last one waits.
        assert!(!q.offer("b"));
        assert!(!q.offer("c"));
        assert_eq!(q.next(), Some("c"));
        assert_eq!(q.next(), None, "the worker stops");
        assert!(q.offer("d"), "and the next offer starts a new one");
    }

    #[test]
    fn same_device_compares_filesystems() {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_owned());
        assert!(same_device(std::slice::from_ref(&home), &home));
        assert!(!same_device(&[], &home));
        assert!(!same_device(&["/nonexistent/x".to_owned()], &home));
        assert!(!same_device(&[home], "/nonexistent"));
    }

    #[test]
    fn web_drops_become_downloads() {
        assert_eq!(
            drop_urls(&["https://a/b.png".to_owned(), "file:///x".to_owned()], ""),
            ["https://a/b.png"]
        );
        assert_eq!(drop_urls(&[], "https://a/b.png\r\n"), ["https://a/b.png"]);
        assert!(drop_urls(&[], "see https://a/b.png for details").is_empty());
        assert!(drop_urls(&[], "plain words").is_empty());
    }

    #[test]
    fn file_list_round_trip() {
        let body = format!("cut\n{}\n{}", file_url("/a b/c"), file_url("/d"));
        let (paths, cut) = parse_file_list(&body);
        assert!(cut);
        assert_eq!(paths, ["/a b/c", "/d"]);
    }

    #[test]
    fn palette_ranking_modes() {
        let items: Vec<String> = ["Close tab", "New tab", "Next tab", "Open terminal"]
            .map(String::from)
            .to_vec();
        assert_eq!(rank_items(&items, "new tab", true), [1]);
        // A single term ranks word-start matches first ("New tab" and "Next tab" start with n/t words).
        let r = rank_items(&items, "t", true);
        assert_eq!(r.len(), 4);
        assert!(r[0] == 1 || r[0] == 3);
        assert_eq!(rank_items(&items, "re:^(new|next)", true), [1, 2]);
        assert_eq!(rank_items(&items, "/tab$/", true), [0, 1, 2]);
        assert!(rank_items(&items, "re:(", true).is_empty());
        // No title has "nwtb" as substrings, so it falls back to fuzzy (unless disabled).
        assert_eq!(rank_items(&items, "nwtb", true), [1]);
        assert!(rank_items(&items, "nwtb", false).is_empty());
    }

    #[test]
    fn decode_bad_escape_is_literal() {
        assert_eq!(percent_decode("50%"), "50%");
        assert_eq!(percent_decode("%zz"), "%zz");
    }
}
