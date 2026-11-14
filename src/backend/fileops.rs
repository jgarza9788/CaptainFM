//! File operations, undo, transfers and batch rename: the bridge methods that change files.

use super::*;

impl qobject::Backend {
    /// Run `work` off the UI thread and deliver its outcome through `opDone`.
    /// Operations run one at a time in submission order, so undo always sees finished work.
    pub(super) fn run_op(
        self: Pin<&mut Self>,
        work: impl FnOnce() -> ops::Outcome + Send + 'static,
    ) {
        let qt_thread = self.qt_thread();
        op_queue(Box::new(move || {
            let json = work().json();
            let _ = qt_thread.queue(move |backend| backend.op_done(q(&json)));
        }));
    }

    /// Queue a copy or move that shows up in the transfer list.
    pub(super) fn start_paste(
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

    pub(super) fn link(self: Pin<&mut Self>, paths: &QString, dest: &QString) {
        let (paths, dest) = (parse_paths(paths), dest.to_string());
        self.run_op(move || ops::link(&paths, &dest));
    }

    pub(super) fn extract(self: Pin<&mut Self>, paths: &QString, into_folder: bool) {
        let paths = parse_paths(paths);
        self.run_op(move || ops::extract(&paths, into_folder));
    }

    pub(super) fn paste(mut self: Pin<&mut Self>, paths: &QString, dest: &QString, cut: bool) {
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

    pub(super) fn paste_with(
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

    pub(super) fn transfers(&self) -> QString {
        q(&transfer::snapshot())
    }

    pub(super) fn transfer_cancel(&self, id: i32) {
        transfer::cancel(id);
    }

    pub(super) fn transfer_pause(&self, id: i32, paused: bool) {
        transfer::pause(id, paused);
    }

    pub(super) fn transfer_clear(&self) {
        transfer::clear_finished();
    }

    pub(super) fn trash(self: Pin<&mut Self>, paths: &QString) {
        let paths = parse_paths(paths);
        self.run_op(move || ops::trash(&paths));
    }

    pub(super) fn remove(self: Pin<&mut Self>, paths: &QString) {
        let paths = parse_paths(paths);
        self.run_op(move || ops::delete(&paths));
    }

    pub(super) fn trash_count(&self) -> i32 {
        ops::trash_count() as i32
    }

    pub(super) fn empty_trash(self: Pin<&mut Self>) {
        self.run_op(ops::empty_trash);
    }

    pub(super) fn rename_to(self: Pin<&mut Self>, path: &QString, new_name: &QString) {
        let (path, name) = (path.to_string(), new_name.to_string());
        self.run_op(move || ops::rename(&path, &name));
    }

    pub(super) fn create(self: Pin<&mut Self>, dir: &QString, name: &QString, is_dir: bool) {
        let (dir, name) = (dir.to_string(), name.to_string());
        self.run_op(move || ops::create(&dir, &name, is_dir));
    }

    pub(super) fn undo(self: Pin<&mut Self>) {
        self.run_op(|| ops::step_history(false));
    }

    pub(super) fn redo(self: Pin<&mut Self>) {
        self.run_op(|| ops::step_history(true));
    }

    pub(super) fn rename_many(self: Pin<&mut Self>, pairs: &QString) {
        let pairs: Vec<(String, String)> =
            serde_json::from_str(&pairs.to_string()).unwrap_or_default();
        self.run_op(move || ops::rename_many(&pairs));
    }

    pub(super) fn rename_preview(&self, paths: &QString, opts: &QString) -> QString {
        let paths = parse_paths(paths);
        let opts: crate::batch::Opts = serde_json::from_str(&opts.to_string()).unwrap_or_default();
        let json = match crate::batch::preview(&paths, &opts) {
            Ok(rows) => serde_json::json!({ "rows": rows }),
            Err(e) => serde_json::json!({ "error": e }),
        };
        q(&json.to_string())
    }
}
