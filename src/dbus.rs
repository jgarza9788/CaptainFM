//! D-Bus integration: `org.freedesktop.FileManager1` (so "show in folder" opens CaptainFM) and the
//! `org.freedesktop.impl.portal.FileChooser` backend that lets other apps use CaptainFM as their
//! file picker (`captainfm --portal`).

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;

use serde_json::{Value as Json, json};
use zbus::blocking::Connection;
use zbus::blocking::connection::Builder;
use zbus::interface;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

pub const FILE_MANAGER_NAME: &str = "org.freedesktop.FileManager1";
pub const APP_NAME: &str = "org.captainfm.App";
pub const PORTAL_NAME: &str = "org.freedesktop.impl.portal.desktop.captainfm";

/// Decode `file://` URIs (and plain paths) into local paths.
pub fn uris_to_paths(uris: &[String]) -> Vec<String> {
    uris.iter()
        .map(|u| match u.strip_prefix("file://") {
            Some(rest) => percent_decode(rest),
            None => u.clone(),
        })
        .collect()
}

pub fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

// ---- FileManager1 ----------------------------------------------------------------------------

type Callback = Arc<dyn Fn(String) + Send + Sync>;

struct FileManager {
    on_request: Callback,
}

impl FileManager {
    fn request(&self, kind: &str, uris: &[String]) {
        (self.on_request)(json!({ "kind": kind, "paths": uris_to_paths(uris) }).to_string());
    }
}

#[interface(name = "org.freedesktop.FileManager1")]
impl FileManager {
    fn show_folders(&self, uris: Vec<String>, _startup_id: String) {
        self.request("folders", &uris);
    }

    fn show_items(&self, uris: Vec<String>, _startup_id: String) {
        self.request("items", &uris);
    }

    fn show_item_properties(&self, uris: Vec<String>, _startup_id: String) {
        self.request("items", &uris);
    }
}

/// Own `org.freedesktop.FileManager1` for the lifetime of the returned connection. Returns `None`
/// when another file manager already owns the name (that one keeps serving it).
pub fn serve_file_manager(
    on_request: impl Fn(String) + Send + Sync + 'static,
) -> Option<Connection> {
    Builder::session()
        .ok()?
        .name(FILE_MANAGER_NAME)
        .ok()?
        .serve_at(
            "/org/freedesktop/FileManager1",
            FileManager {
                on_request: Arc::new(on_request),
            },
        )
        .ok()?
        .build()
        .ok()
}

/// Own `org.captainfm.App` so `captainfm --tab <path>` can hand folders to this running window.
pub fn serve_app(on_request: impl Fn(String) + Send + Sync + 'static) -> Option<Connection> {
    Builder::session()
        .ok()?
        .name(APP_NAME)
        .ok()?
        .serve_at(
            "/org/captainfm/App",
            FileManager {
                on_request: Arc::new(on_request),
            },
        )
        .ok()?
        .build()
        .ok()
}

/// Ask a running CaptainFM to open `paths` as tabs. Returns false when none is running.
pub fn forward_to_running(paths: &[String]) -> bool {
    let Ok(conn) = Connection::session() else {
        return false;
    };
    let uris: Vec<String> = paths
        .iter()
        .map(|p| format!("file://{}", crate::ops::percent_encode_path(p)))
        .collect();
    conn.call_method(
        Some(APP_NAME),
        "/org/captainfm/App",
        Some("org.freedesktop.FileManager1"),
        "ShowFolders",
        &(uris, "cli"),
    )
    .is_ok()
}

// ---- FileChooser portal backend --------------------------------------------------------------

/// What the picker window is asked to do, derived from the portal's `options` dictionary.
#[derive(Debug, Default, PartialEq)]
pub struct PickRequest {
    pub title: String,
    pub save: bool,
    pub directory: bool,
    pub multiple: bool,
    pub folder: String,
    pub name: String,
    pub accept_label: String,
    /// `[{name, patterns}]` (glob patterns only).
    pub filters: Vec<(String, Vec<String>)>,
}

fn opt<T>(options: &HashMap<String, OwnedValue>, key: &str) -> Option<T>
where
    T: TryFrom<OwnedValue>,
{
    options
        .get(key)
        .and_then(|v| v.try_clone().ok())
        .and_then(|v| T::try_from(v).ok())
}

fn bytes_to_path(mut bytes: Vec<u8>) -> String {
    while bytes.last() == Some(&0) {
        bytes.pop();
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

pub fn request_from_options(
    title: &str,
    save: bool,
    options: &HashMap<String, OwnedValue>,
) -> PickRequest {
    let filters: Vec<(String, Vec<(u32, String)>)> = opt(options, "filters").unwrap_or_default();
    PickRequest {
        title: title.to_owned(),
        save,
        directory: opt::<bool>(options, "directory").unwrap_or(false),
        multiple: opt::<bool>(options, "multiple").unwrap_or(false),
        folder: opt::<Vec<u8>>(options, "current_folder")
            .map(bytes_to_path)
            .unwrap_or_default(),
        name: opt::<String>(options, "current_name").unwrap_or_default(),
        accept_label: opt::<String>(options, "accept_label").unwrap_or_default(),
        filters: filters
            .into_iter()
            .map(|(name, pats)| {
                (
                    name,
                    pats.into_iter()
                        .filter(|(kind, _)| *kind == 0)
                        .map(|(_, p)| p)
                        .collect(),
                )
            })
            .collect(),
    }
}

impl PickRequest {
    pub fn to_json(&self) -> Json {
        json!({
            "title": self.title,
            "save": self.save,
            "directory": self.directory,
            "multiple": self.multiple,
            "folder": self.folder,
            "name": self.name,
            "acceptLabel": self.accept_label,
            "filters": self.filters.iter().map(|(n, p)| json!({"name": n, "patterns": p})).collect::<Vec<_>>(),
        })
    }
}

/// This program's executable. After an upgrade replaces the file, Linux reports the old path with a
/// " (deleted)" suffix; the new version lives at the path without it.
fn running_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let text = exe.to_string_lossy();
    Some(PathBuf::from(
        text.strip_suffix(" (deleted)").unwrap_or(&text),
    ))
}

/// Modification time and size of the installed executable, to notice an upgrade.
fn exe_stamp() -> Option<(std::time::SystemTime, u64)> {
    let meta = std::fs::metadata(running_exe()?).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// Run the picker window as a child process; `Some(uris)` when the user chose something.
fn run_picker(req: &PickRequest) -> Option<Vec<String>> {
    let exe = running_exe()?;
    let output = Command::new(exe)
        .arg("--pick")
        .arg(req.to_json().to_string())
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let v: Json = serde_json::from_slice(&output.stdout).ok()?;
    let uris: Vec<String> = v
        .get("paths")?
        .as_array()?
        .iter()
        .filter_map(|p| p.as_str())
        .map(|p| format!("file://{}", crate::ops::percent_encode_path(p)))
        .collect();
    (!uris.is_empty()).then_some(uris)
}

type PortalReply = (u32, HashMap<String, OwnedValue>);

fn reply(uris: Option<Vec<String>>) -> PortalReply {
    match uris {
        Some(uris) => {
            let mut results = HashMap::new();
            if let Ok(v) = OwnedValue::try_from(Value::from(uris)) {
                results.insert("uris".to_owned(), v);
            }
            (0, results)
        }
        None => (1, HashMap::new()),
    }
}

struct FileChooser;

#[interface(name = "org.freedesktop.impl.portal.FileChooser")]
impl FileChooser {
    #[zbus(property, name = "version")]
    fn version(&self) -> u32 {
        3
    }

    async fn open_file(
        &self,
        _handle: ObjectPath<'_>,
        _app_id: String,
        _parent_window: String,
        title: String,
        options: HashMap<String, OwnedValue>,
    ) -> PortalReply {
        let req = request_from_options(&title, false, &options);
        reply(blocking::unblock(move || run_picker(&req)).await)
    }

    async fn save_file(
        &self,
        _handle: ObjectPath<'_>,
        _app_id: String,
        _parent_window: String,
        title: String,
        options: HashMap<String, OwnedValue>,
    ) -> PortalReply {
        let req = request_from_options(&title, true, &options);
        reply(blocking::unblock(move || run_picker(&req)).await)
    }

    async fn save_files(
        &self,
        _handle: ObjectPath<'_>,
        _app_id: String,
        _parent_window: String,
        title: String,
        options: HashMap<String, OwnedValue>,
    ) -> PortalReply {
        // Saving several files: pick the destination folder; the app appends its own names.
        let mut req = request_from_options(&title, false, &options);
        req.directory = true;
        reply(blocking::unblock(move || run_picker(&req)).await)
    }
}

/// Serve the portal backend until the process is killed. Returns an exit code.
pub fn run_portal() -> i32 {
    let built = Builder::session()
        .and_then(|b| b.name(PORTAL_NAME))
        .and_then(|b| b.serve_at("/org/freedesktop/portal/desktop", FileChooser))
        .and_then(|b| b.build());
    let started_with = exe_stamp();
    match built {
        // Stay up while the session bus does: a backend must not outlive the bus it was started on, nor
        // the version it was started from (D-Bus activation starts the new one on the next request).
        Ok(connection) => loop {
            std::thread::sleep(std::time::Duration::from_secs(5));
            if started_with.is_some() && exe_stamp() != started_with {
                return 0;
            }
            let alive = connection
                .call_method(
                    Some("org.freedesktop.DBus"),
                    "/org/freedesktop/DBus",
                    Some("org.freedesktop.DBus"),
                    "GetId",
                    &(),
                )
                .is_ok();
            if !alive {
                return 0;
            }
        },
        Err(e) => {
            eprintln!("captainfm: cannot start the file chooser portal: {e}");
            1
        }
    }
}

// ---- installation files ------------------------------------------------------------------------

fn data_home() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| crate::state::home().join(".local/share"))
}

pub fn portal_file() -> PathBuf {
    data_home().join("xdg-desktop-portal/portals/captainfm.portal")
}

pub fn portal_service_file() -> PathBuf {
    data_home().join(format!("dbus-1/services/{PORTAL_NAME}.service"))
}

pub fn file_manager_service_file() -> PathBuf {
    data_home().join(format!("dbus-1/services/{FILE_MANAGER_NAME}.service"))
}

/// Desktops named in `XDG_CURRENT_DESKTOP` (`Hyprland`, `GNOME:Unity`, ...); Hyprland when unset.
pub fn current_desktops() -> Vec<String> {
    let names: Vec<String> = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .split(':')
        .filter(|d| !d.is_empty())
        .map(str::to_owned)
        .collect();
    if names.is_empty() {
        vec!["Hyprland".to_owned()]
    } else {
        names
    }
}

pub fn portal_definition() -> String {
    format!(
        "[portal]\nDBusName={PORTAL_NAME}\nInterfaces=org.freedesktop.impl.portal.FileChooser;\nUseIn={};\n",
        current_desktops().join(";")
    )
}

const FILE_CHOOSER_KEY: &str = "org.freedesktop.impl.portal.FileChooser";

/// The portal configuration file that applies to this desktop: the first existing
/// `<desktop>-portals.conf`, else `portals.conf`.
pub fn portals_conf_path() -> PathBuf {
    let dir = crate::state::config_dir()
        .parent()
        .map(PathBuf::from)
        .unwrap_or_default()
        .join("xdg-desktop-portal");
    current_desktops()
        .iter()
        .map(|d| dir.join(format!("{}-portals.conf", d.to_lowercase())))
        .find(|p| p.exists())
        .unwrap_or_else(|| dir.join("portals.conf"))
}

/// Whether `conf` already names CaptainFM as the file chooser.
pub fn file_chooser_preferred(conf: &str) -> bool {
    conf.lines().any(|l| {
        l.split_once('=').is_some_and(|(k, v)| {
            k.trim() == FILE_CHOOSER_KEY && v.split(';').any(|b| b.trim() == "captainfm")
        })
    })
}

/// `conf` with the file chooser preference set to CaptainFM (`on`) or removed; every other line is kept.
pub fn with_file_chooser(conf: &str, on: bool) -> String {
    let mut lines: Vec<&str> = conf
        .lines()
        .filter(|l| {
            l.split_once('=')
                .is_none_or(|(k, _)| k.trim() != FILE_CHOOSER_KEY)
        })
        .collect();
    if on {
        let entry = format!("{FILE_CHOOSER_KEY}=captainfm");
        match lines.iter().position(|l| l.trim() == "[preferred]") {
            Some(i) => lines.insert(i + 1, &entry),
            None => {
                lines.push("[preferred]");
                lines.push(&entry);
            }
        }
        return lines.join("\n") + "\n";
    }
    // Drop a `[preferred]` header that is now empty.
    if let Some(i) = lines.iter().position(|l| l.trim() == "[preferred]")
        && lines[i + 1..]
            .iter()
            .take_while(|l| !l.trim_start().starts_with('['))
            .all(|l| l.trim().is_empty())
    {
        lines.remove(i);
    }
    let text = lines.join("\n");
    if text.trim().is_empty() {
        String::new()
    } else {
        text + "\n"
    }
}

pub fn service_definition(name: &str, exec: &str) -> String {
    format!("[D-BUS Service]\nName={name}\nExec={exec}\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uris_decode_to_paths() {
        let uris = vec![
            "file:///home/u/My%20Docs/a.txt".to_owned(),
            "/plain/path".to_owned(),
        ];
        assert_eq!(
            uris_to_paths(&uris),
            ["/home/u/My Docs/a.txt", "/plain/path"]
        );
    }

    #[test]
    fn options_become_a_pick_request() {
        let mut options: HashMap<String, OwnedValue> = HashMap::new();
        options.insert(
            "multiple".into(),
            OwnedValue::try_from(Value::from(true)).unwrap(),
        );
        options.insert(
            "current_name".into(),
            OwnedValue::try_from(Value::from("report.pdf")).unwrap(),
        );
        options.insert(
            "current_folder".into(),
            OwnedValue::try_from(Value::from(b"/home/u\0".to_vec())).unwrap(),
        );
        let req = request_from_options("Open", true, &options);
        assert!(req.multiple && req.save && !req.directory);
        assert_eq!(req.name, "report.pdf");
        assert_eq!(req.folder, "/home/u");
    }

    #[test]
    fn cancelled_pick_replies_with_code_one() {
        assert_eq!(reply(None).0, 1);
        let (code, results) = reply(Some(vec!["file:///x".into()]));
        assert_eq!(code, 0);
        assert!(results.contains_key("uris"));
    }

    #[test]
    fn file_chooser_preference_is_added_and_removed() {
        let conf = "[preferred]\ndefault=hyprland;gtk\n";
        let on = with_file_chooser(conf, true);
        assert_eq!(
            on,
            "[preferred]\norg.freedesktop.impl.portal.FileChooser=captainfm\ndefault=hyprland;gtk\n"
        );
        assert!(file_chooser_preferred(&on) && !file_chooser_preferred(conf));
        assert_eq!(with_file_chooser(&on, true), on, "idempotent");
        assert_eq!(with_file_chooser(&on, false), conf);
        assert_eq!(
            with_file_chooser("", true),
            "[preferred]\norg.freedesktop.impl.portal.FileChooser=captainfm\n"
        );
        assert_eq!(with_file_chooser(&with_file_chooser("", true), false), "");
    }

    #[test]
    fn running_exe_ignores_the_deleted_marker() {
        // The test binary is not deleted, so the path is returned as is and exists.
        assert!(
            running_exe()
                .is_some_and(|p| p.exists() && !p.to_string_lossy().ends_with("(deleted)"))
        );
        assert!(exe_stamp().is_some());
    }

    #[test]
    fn portal_definition_names_the_interface() {
        assert!(portal_definition().contains("org.freedesktop.impl.portal.FileChooser"));
        assert!(service_definition("n", "/bin/x --portal").contains("Exec=/bin/x --portal"));
    }
}
