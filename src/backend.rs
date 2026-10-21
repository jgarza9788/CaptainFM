//! `Backend`: the QML-facing singleton for state, file operations and previews.
//!
//! Structured data crosses the boundary as JSON strings; long-running work runs on threads and
//! reports back through signals.

use std::pin::Pin;
use std::process::{Command, Stdio};

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;

use crate::{devices, ops, plugins, preview, state};

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
        #[qinvokable]
        fn env(&self, name: &QString) -> QString;
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

        /// Copy or move `paths` (JSON array) into `dest`. Reports through `opDone`.
        #[qinvokable]
        fn paste(self: Pin<&mut Self>, paths: &QString, dest: &QString, cut: bool);
        #[qinvokable]
        fn trash(self: Pin<&mut Self>, paths: &QString);
        /// Permanently delete `paths`. Not undoable.
        #[qinvokable]
        fn remove(self: Pin<&mut Self>, paths: &QString);
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
        #[qinvokable]
        #[rust_name = "copy_text"]
        fn copyText(&self, text: &QString);

        #[qinvokable]
        #[rust_name = "open_path"]
        fn openPath(&self, path: &QString);
        #[qinvokable]
        #[rust_name = "open_terminal"]
        fn openTerminal(&self, dir: &QString);

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
        fn runPlugin(self: Pin<&mut Self>, plugin: &QString, action: &QString, ctx: &QString, call_id: i32);

        #[qsignal]
        #[cxx_name = "pluginResult"]
        fn plugin_result(self: Pin<&mut Self>, call_id: i32, json: QString);

        /// Number of paths currently indexed for deep search.
        #[qinvokable]
        #[rust_name = "index_paths"]
        fn indexPaths(&self) -> i32;
        /// Disks and remotes for the sidebar; answered by `sidebarDevices`.
        #[qinvokable]
        #[rust_name = "list_sidebar_devices"]
        fn listSidebarDevices(self: Pin<&mut Self>);

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
        let Ok(re) = regex::RegexBuilder::new(pattern).case_insensitive(insensitive).build() else {
            return Vec::new();
        };
        // Regex results keep the list's own order.
        return items.iter().enumerate().filter(|(_, s)| re.is_match(s)).map(|(i, _)| i).collect();
    }
    let mut scored: Vec<(i32, usize)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, s)| crate::fuzzy::ordered_terms_score(query, s).map(|sc| (sc, i)))
        .collect();
    if scored.is_empty() && fuzzy_fallback {
        let fuzzy = crate::fuzzy::Query::parse(query);
        scored = items.iter().enumerate().filter_map(|(i, s)| fuzzy.score(s, s).map(|sc| (sc, i))).collect();
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, i)| i).collect()
}

fn file_url(path: &str) -> String {
    format!("file://{}", ops::percent_encode_path(path))
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

    /// Run `work` off the UI thread and deliver its outcome through `opDone`.
    /// Operations run one at a time in submission order, so undo always sees finished work.
    fn run_op(self: Pin<&mut Self>, work: impl FnOnce() -> ops::Outcome + Send + 'static) {
        let qt_thread = self.qt_thread();
        op_queue(Box::new(move || {
            let json = work().json();
            let _ = qt_thread.queue(move |backend| backend.op_done(q(&json)));
        }));
    }

    fn paste(self: Pin<&mut Self>, paths: &QString, dest: &QString, cut: bool) {
        let (paths, dest) = (parse_paths(paths), dest.to_string());
        self.run_op(move || ops::paste(&paths, &dest, cut));
    }

    fn trash(self: Pin<&mut Self>, paths: &QString) {
        let paths = parse_paths(paths);
        self.run_op(move || ops::trash(&paths));
    }

    fn remove(self: Pin<&mut Self>, paths: &QString) {
        let paths = parse_paths(paths);
        self.run_op(move || ops::delete(&paths));
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
        let pairs: Vec<(String, String)> = serde_json::from_str(&pairs.to_string()).unwrap_or_default();
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
        let state_dir = state::store_file().parent().map(std::path::Path::to_path_buf);
        if let Some(dir) = state_dir {
            let _ = std::fs::create_dir_all(&dir);
            let qt = self.qt_thread();
            let watcher = crate::watch::watch(&dir, false, std::time::Duration::from_millis(150), move || {
                let _ = qt.queue(|b| {
                    state::reset_store();
                    b.bookmarks_changed();
                });
            });
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

    fn run_plugin(self: Pin<&mut Self>, plugin: &QString, action: &QString, ctx: &QString, call_id: i32) {
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
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let json = serde_json::to_string(&devices::list()).unwrap_or_else(|_| "[]".into());
            let _ = qt_thread.queue(move |b| b.sidebar_devices(q(&json)));
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
        let (kind, device_path, path, removable) =
            (field("kind"), field("device"), field("path"), dev.get("removable").and_then(|v| v.as_bool()).unwrap_or(false));
        if kind == "disk" {
            self.report_mount(move || devices::eject_disk(&device_path, removable));
        } else {
            self.report_mount(move || devices::unmount_remote(&path));
        }
    }

    fn connect_remote(self: Pin<&mut Self>, url: &QString) {
        self.mount_with_prompts(url.to_string());
    }

    fn answer_prompt(&self, text: &QString, cancel: bool) {
        devices::answer_prompt(if cancel { None } else { Some(text.to_string()) });
    }

    /// Run a mount-like job on a thread and report it through `mountDone`.
    fn report_mount(self: Pin<&mut Self>, work: impl FnOnce() -> devices::MountResult + Send + 'static) {
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let json = serde_json::to_string(&work()).unwrap_or_default();
            let _ = qt_thread.queue(move |b| b.mount_done(q(&json)));
        });
    }

    /// Mount a GVfs URI, asking the UI for credentials when gio prompts for them.
    fn mount_with_prompts(self: Pin<&mut Self>, uri: String) {
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let answers = devices::begin_prompting();
            let asker = qt_thread.clone();
            let result = devices::mount_uri(&uri, |prompt| {
                let prompt = prompt.to_owned();
                let _ = asker.queue(move |b| b.credentials_needed(q(&prompt)));
                answers.recv_timeout(std::time::Duration::from_secs(180)).ok().flatten()
            });
            let json = serde_json::to_string(&result).unwrap_or_default();
            let _ = qt_thread.queue(move |b| b.mount_done(q(&json)));
        });
    }

    fn exists(&self, path: &QString) -> bool {
        std::fs::symlink_metadata(path.to_string()).is_ok()
    }

    fn is_dir(&self, path: &QString) -> bool {
        std::path::Path::new(&path.to_string()).is_dir()
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
        spawn_detached("xdg-terminal-exec", &[], Some(&dir));
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
        std::thread::spawn(move || {
            let json = preview::preview(&path).to_string();
            let _ = qt_thread.queue(move |backend| backend.preview_ready(q(&path), q(&json)));
        });
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
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
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
    fn file_list_round_trip() {
        let body = format!("cut\n{}\n{}", file_url("/a b/c"), file_url("/d"));
        let (paths, cut) = parse_file_list(&body);
        assert!(cut);
        assert_eq!(paths, ["/a b/c", "/d"]);
    }

    #[test]
    fn palette_ranking_modes() {
        let items: Vec<String> = ["Close tab", "New tab", "Next tab", "Open terminal"].map(String::from).to_vec();
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
