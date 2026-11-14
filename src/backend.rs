//! `Backend`: the QML-facing singleton for state, file operations and previews.
//!
//! Structured data crosses the boundary as JSON strings; long-running work runs on threads and
//! reports back through signals.

use crate::lock::MutexExt;
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

mod desktop;
mod drives;
mod fileops;
mod helpers;
use helpers::*;

#[cfg(test)]
mod tests;
