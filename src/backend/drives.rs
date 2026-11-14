//! Drives, remotes and saved connections: listing, mounting, ejecting, connecting and the mount prompts.

use super::*;

impl qobject::Backend {
    pub(super) fn list_sidebar_devices(self: Pin<&mut Self>) {
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

    pub(super) fn request_space(mut self: Pin<&mut Self>, path: &QString) {
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
            .locked()
            .get_or_insert_with(Default::default)
            .get(&path, Instant::now());
        if let Some(found) = cached {
            self.as_mut().space_ready(q(&path), q(&json(found)));
            return;
        }
        // One question per remote at a time; a hung server must not collect threads.
        if !ASKING
            .locked()
            .get_or_insert_with(Default::default)
            .insert(path.clone())
        {
            return;
        }
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let found = devices::remote_space(&path);
            CACHE
                .locked()
                .get_or_insert_with(Default::default)
                .put(&path, Instant::now(), found);
            ASKING
                .locked()
                .get_or_insert_with(Default::default)
                .remove(&path);
            let _ = qt_thread.queue(move |b| b.space_ready(q(&path), q(&json(found))));
        });
    }

    pub(super) fn watch_devices(self: Pin<&mut Self>) {
        static STARTED: std::sync::Once = std::sync::Once::new();
        let qt_thread = self.qt_thread();
        STARTED.call_once(|| {
            devices::watch_changes(move || {
                let _ = qt_thread.queue(|b| b.list_sidebar_devices());
            });
        });
    }

    pub(super) fn list_devices(self: Pin<&mut Self>) {
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let json = serde_json::to_string(&devices::list()).unwrap_or_else(|_| "[]".into());
            let _ = qt_thread.queue(move |b| b.devices_listed(q(&json)));
        });
    }

    pub(super) fn mount_device(self: Pin<&mut Self>, device: &QString) {
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

    pub(super) fn eject_device(self: Pin<&mut Self>, device: &QString) {
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

    pub(super) fn connect_remote(self: Pin<&mut Self>, url: &QString) {
        self.mount_with_prompts(url.to_string());
    }

    pub(super) fn connect_profile(self: Pin<&mut Self>, json: &QString) {
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

    pub(super) fn connect_saved(self: Pin<&mut Self>, id: &QString) {
        let id = id.to_string();
        self.connect_job(move |ask| remotes::connect_saved(&id, ask));
    }

    pub(super) fn connect_for(self: Pin<&mut Self>, path: &QString) {
        let path = path.to_string();
        self.connect_job(move |ask| remotes::connect_for(&path, ask));
    }

    pub(super) fn home_label(&self, path: &QString) -> QString {
        q(&remotes::home_label(&path.to_string()))
    }

    pub(super) fn last_visit_under(&self, root: &QString) -> QString {
        q(&state::last_visit_under(&root.to_string()).unwrap_or_default())
    }

    pub(super) fn reconnect(self: Pin<&mut Self>, folder: &QString) {
        let folder = folder.to_string();
        let root = listing::remote_root(&folder).unwrap_or_else(|| folder.clone());
        self.connect_job(move |ask| remotes::reconnect(&folder, &root, ask));
    }

    pub(super) fn saved_connections(&self) -> QString {
        q(&serde_json::to_string(&remotes::load()).unwrap_or_else(|_| "[]".into()))
    }

    pub(super) fn delete_connection(self: Pin<&mut Self>, id: &QString) {
        let id = id.to_string();
        self.report_mount(move || remotes::delete(&id));
    }

    pub(super) fn update_connection(self: Pin<&mut Self>, json: &QString) {
        let v: serde_json::Value = serde_json::from_str(&json.to_string()).unwrap_or_default();
        let profile: remotes::Profile = serde_json::from_value(v.clone()).unwrap_or_default();
        let password = v
            .get("password")
            .and_then(|p| p.as_str())
            .unwrap_or("")
            .to_owned();
        self.report_mount(move || remotes::update(profile, &password));
    }

    pub(super) fn answer_prompt(&self, text: &QString, cancel: bool) {
        devices::answer_prompt(if cancel { None } else { Some(text.to_string()) });
    }

    /// Run a mount-like job on a thread and report it through `mountDone`.
    pub(super) fn report_mount(
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
    pub(super) fn mount_with_prompts(self: Pin<&mut Self>, uri: String) {
        self.connect_job(move |ask| devices::mount_uri(&uri, ask));
    }

    /// Run a connect job on a thread; `ask` relays credential prompts to the UI and waits for the answer.
    pub(super) fn connect_job(
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
}
