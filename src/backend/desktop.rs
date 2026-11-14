//! Desktop integration: clipboard, drag and drop, opening paths and terminals, and the palette bookkeeping.

use super::*;

impl qobject::Backend {
    pub(super) fn copy_text(&self, text: &QString) {
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

    pub(super) fn open_path(&self, path: &QString) {
        spawn_detached("xdg-open", &[&path.to_string()], None);
    }

    pub(super) fn open_terminal(&self, dir: &QString) {
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

    pub(super) fn drag_payload(&self, paths: &QString) -> QString {
        let paths: Vec<String> = serde_json::from_str(&paths.to_string()).unwrap_or_default();
        q(&drag_payload(&paths).to_string())
    }

    pub(super) fn drop_paths(&self, urls: &QString, text: &QString) -> QString {
        let urls: Vec<String> = serde_json::from_str(&urls.to_string()).unwrap_or_default();
        q(&serde_json::json!(drop_paths(&urls, &text.to_string())).to_string())
    }

    pub(super) fn same_device(&self, paths: &QString, dest: &QString) -> bool {
        let paths: Vec<String> = serde_json::from_str(&paths.to_string()).unwrap_or_default();
        same_device(&paths, &dest.to_string())
    }

    pub(super) fn drop_urls(&self, urls: &QString, text: &QString) -> QString {
        let urls: Vec<String> = serde_json::from_str(&urls.to_string()).unwrap_or_default();
        q(&serde_json::json!(drop_urls(&urls, &text.to_string())).to_string())
    }

    pub(super) fn download_urls(self: Pin<&mut Self>, urls: &QString, dest: &QString) {
        let (urls, dest) = (parse_paths(urls), dest.to_string());
        self.run_op(move || ops::download(&urls, &dest));
    }

    pub(super) fn system_picker(&self) -> bool {
        crate::cli::portal_installed()
    }

    pub(super) fn set_system_picker(&self, on: bool) -> QString {
        let (ok, message) = match crate::cli::set_portal(on) {
            Ok(m) => (true, m),
            Err(e) => (false, e),
        };
        q(&serde_json::json!({"ok": ok, "message": message}).to_string())
    }

    pub(super) fn clipboard_write(&self, paths: &QString, cut: bool) {
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

    pub(super) fn clipboard_read(&self) -> QString {
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
}
